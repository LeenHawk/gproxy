//! Credential cycles: one row per period of each upstream window a credential
//! has, with the USD this deployment settled against it.
//!
//! # Where the boundaries come from
//!
//! Every credential keeps cycles; what differs is who draws the lines. A
//! window the upstream reports on gets its boundaries from the reports
//! (`observed`); until then — and forever, for a window nobody reports on —
//! they are derived here from the declared `QuotaWindow` (`local`). A
//! credential whose channel declares no window at all is cut by UTC calendar
//! month under the window id [`MONTH_WINDOW`].
//!
//! # Which windows
//!
//! Every channel-declared dimension, Reported or Counted, is a billing window
//! once it is at least [`MIN_CYCLE_MS`] long: a request inside it spends it,
//! so its cost is worth knowing, and a `Total` window (a prepaid balance)
//! simply never rolls over, which makes its cycle the credential's lifetime
//! spend. Shorter windows are rate limits, and a cycle per minute would be a
//! table of noise. Operator limits (`limit:{quota_id}`) are this deployment's
//! own windows, already metered in `counted_windows`, and are left out.
//!
//! # Accrual
//!
//! A settled exchange adds its USD cost to every open cycle of its credential
//! whose scope covers the upstream model — the declared scope, or the
//! observation's where the declaration leaves it unknown; an unknown scope
//! never matches. Overlapping windows each carry the full cost, so cycles of
//! different windows never add up to anything.
//!
//! The settlement path touches Store once in the common case: one batch of
//! `cost_usd = cost_usd + ?` updates, one per matching cycle, against a cached
//! list of the credential's open cycles. Opening and rolling over are rare and
//! cost one more batch. The cache is only a hint: every write is guarded in the
//! database, and a cycle that closed under a stale cache is detected by its
//! update touching no row, which reloads and places the charge again.
//!
//! # Observations
//!
//! [`decide`] is the state machine, one reading against the window's open
//! cycle. Its rules, in order:
//!
//! 1. A reading of an unused rolling window ("not started": nothing used and
//!    a reset one window length from now, within [`TOLERANCE_MS`]) opens
//!    nothing. An expired cycle is closed at its end; a local cycle is kept,
//!    because a first use here is ahead of the upstream's report; an observed
//!    cycle that has not ended was reset by the upstream with nothing used
//!    since, and is closed now. A reading whose reset is still the open
//!    cycle's own end is not a new period (tiny usage reads as 0%) and falls
//!    through to the rules below.
//! 2. No open cycle: open an `observed` one, and backfill what was already
//!    spent in it from the usage log.
//! 3. Reset within [`TOLERANCE_MS`] of the cycle's end: the same cycle. Adopt
//!    the upstream's boundaries and the reading — unless a `Periodic` window's
//!    usage fell by [`DROP_POINTS`] or more, which is a server reset.
//! 4. Reset further away, cycle ended: rollover.
//! 5. Reset further away, cycle not ended, cycle still `local`: the first
//!    observation corrects a guessed boundary; it does not split. This covers
//!    the cycle a manual reset opens, whose next observation always differs
//!    from the old window.
//! 6. Otherwise a server reset: close at the split point, open a new cycle,
//!    and move what the usage log shows was spent since the split.
//!
//! An observation whose dimension is not declared never touches a cycle.

use std::{collections::HashSet, sync::Arc, time::Duration};

use gproxy_cache::Cache;
use gproxy_channel::channel::{
    QuotaAllowance, QuotaDimension, QuotaResetBehavior, QuotaScope, QuotaTracking, QuotaWindow,
};
use gproxy_protocol::Operation;
use gproxy_seaorm::{BatchConnectionTrait, FixedDecimal};
use gproxy_store::{
    Store,
    entity::limits::credential_cycle::{self, CycleBoundary, CycleOpening},
    operations::cycles::{CycleChange, CycleSample, NewCycle},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::window_bounds;
use crate::{CoreError, CoreResult, CredentialData, ids, keys, usage_scan};

/// Boundary drift below this is the same period, as in the observation dedupe.
pub(crate) const TOLERANCE_MS: i64 = super::dedupe::PERIOD_TOLERANCE_MS;
/// The window id of the calendar-month cycle of a credential that declares
/// no window.
pub(crate) const MONTH_WINDOW: &str = "month";
/// Windows shorter than this are rate limits, not billing periods.
pub(crate) const MIN_CYCLE_MS: i64 = 60 * 60 * 1000;
/// A `Periodic` window whose usage falls this many percentage points without
/// its reset moving was reset by the upstream.
pub(crate) const DROP_POINTS: Decimal = Decimal::TEN;
/// How long a credential's open cycles stay cached. Short, because the cache
/// is shared by instances that each change cycles; a stale entry costs one
/// wasted update, never a wrong total.
const CACHE_TTL: Duration = Duration::from_secs(60);

/// The cached projection of one open cycle: what the settlement path and the
/// state machine read. Cost is not here; it changes on every request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct OpenCycle {
    pub id: String,
    pub window_id: String,
    pub dimension_id: Option<String>,
    pub scope: QuotaScope,
    pub starts_at_ms: i64,
    pub ends_at_ms: Option<i64>,
    pub boundary: CycleBoundary,
    pub opened_by: CycleOpening,
    pub sample_used_percent: Option<Decimal>,
    pub sample_at_ms: Option<i64>,
}

impl OpenCycle {
    fn from_row(row: &credential_cycle::Model) -> Self {
        Self {
            id: row.id.clone(),
            window_id: row.window_id.clone(),
            dimension_id: row.dimension_id.clone(),
            scope: serde_json::from_value(row.scope.clone()).unwrap_or_default(),
            starts_at_ms: row.starts_at_ms,
            ends_at_ms: row.ends_at_ms,
            boundary: row.boundary,
            opened_by: row.opened_by,
            sample_used_percent: row.sample_used_percent.map(FixedDecimal::decimal),
            sample_at_ms: row.sample_at_ms,
        }
    }

    fn expired(&self, now_ms: i64) -> bool {
        self.ends_at_ms.is_some_and(|end| end <= now_ms)
    }
}

/// The windows of one credential that keep cycles.
pub(crate) struct CycleSubject<'a> {
    pub credential_id: &'a str,
    /// Channel-declared dimensions long enough to be billing periods. Empty
    /// puts the credential on calendar months.
    pub dimensions: Vec<&'a QuotaDimension>,
}

impl<'a> CycleSubject<'a> {
    pub fn of(credential: &'a CredentialData) -> Self {
        Self {
            credential_id: &credential.id,
            dimensions: credential
                .quota
                .iter()
                .filter(|d| !credential.limits.iter().any(|l| l.dimension_id == d.id))
                .filter(|d| billing_window(&d.window))
                .collect(),
        }
    }

    fn dimension(&self, id: Option<&str>) -> Option<&'a QuotaDimension> {
        let id = id?;
        self.dimensions.iter().copied().find(|d| d.id == id)
    }

    /// Whether a charge for this operation and model lands in `cycle`.
    fn covers(&self, cycle: &OpenCycle, operation: Operation, model: Option<&str>) -> bool {
        scope_covers(&cycle.scope, model)
            && self
                .dimension(cycle.dimension_id.as_deref())
                .and_then(|d| d.operations.as_ref())
                .is_none_or(|ops| ops.contains(&operation))
    }
}

fn billing_window(window: &QuotaWindow) -> bool {
    match window {
        QuotaWindow::Rolling { seconds } | QuotaWindow::Fixed { seconds, .. } => {
            seconds.saturating_mul(1000) >= MIN_CYCLE_MS
        }
        _ => true,
    }
}

/// An unknown scope never matches: an observe-only model list is not proof
/// that a request spent the window.
fn scope_covers(scope: &QuotaScope, model: Option<&str>) -> bool {
    match scope {
        QuotaScope::All => true,
        QuotaScope::Unknown => false,
        scope => model.is_some_and(|model| scope.matches(model)),
    }
}

/// The length of a window, where it has a fixed one.
fn window_ms(window: &QuotaWindow) -> Option<i64> {
    match window {
        QuotaWindow::Rolling { seconds } | QuotaWindow::Fixed { seconds, .. } => {
            Some(seconds.saturating_mul(1000))
        }
        QuotaWindow::CalendarDay => Some(86_400_000),
        QuotaWindow::CalendarWeek => Some(7 * 86_400_000),
        QuotaWindow::CalendarMonth | QuotaWindow::Total => None,
    }
}

/// Boundaries of a cycle opened here at `now_ms`. A Reported rolling window
/// starts at first use, which is now; a Counted one is aligned exactly like
/// the meter's own `counted_windows`, so the two agree on what a period is.
fn local_bounds(dimension: Option<&QuotaDimension>, now_ms: i64) -> (i64, Option<i64>) {
    let Some(dimension) = dimension else {
        let (start, end) = window_bounds(&QuotaWindow::CalendarMonth, now_ms);
        return (start, Some(end));
    };
    match dimension.window {
        QuotaWindow::Total => (now_ms, None),
        QuotaWindow::Rolling { seconds } if dimension.tracking == QuotaTracking::Reported => (
            now_ms,
            Some(now_ms.saturating_add(seconds.saturating_mul(1000))),
        ),
        ref window => {
            let (start, end) = window_bounds(window, now_ms);
            (start, Some(end))
        }
    }
}

fn near(a: i64, b: i64) -> bool {
    a.abs_diff(b) <= TOLERANCE_MS as u64
}

fn fixed(value: Option<Decimal>) -> Option<FixedDecimal> {
    value.and_then(|value| FixedDecimal::rounded(value).ok())
}

fn scope_json(scope: &QuotaScope) -> serde_json::Value {
    serde_json::to_value(scope).unwrap_or(serde_json::Value::Null)
}

/// What one observed reading says about its window, reduced to what the
/// state machine reads.
#[derive(Clone, Debug, Default)]
pub(crate) struct Facts {
    pub window_id: String,
    pub dimension_id: String,
    pub scope: QuotaScope,
    /// Reported, or derived from used and limit.
    pub used_percent: Option<Decimal>,
    pub used: Option<Decimal>,
    pub limit: Option<Decimal>,
    pub starts_at_ms: Option<i64>,
    pub resets_at_ms: Option<i64>,
    /// The reported period, or the declared window's length.
    pub window_ms: Option<i64>,
    /// An unused rolling window whose reset floats with the observation.
    pub idle: bool,
    pub periodic: bool,
    /// Balances only link to the cycle: they carry no period and no share.
    pub link_only: bool,
    /// The observation log persists this reading, so the cycle takes it as
    /// its sample.
    pub written: bool,
}

impl Facts {
    pub fn new(
        window_id: &str,
        dimension: &QuotaDimension,
        scope: QuotaScope,
        allowance: Option<&QuotaAllowance>,
        written: bool,
        now_ms: i64,
    ) -> Self {
        let Some(a) = allowance else {
            return Self {
                window_id: window_id.to_owned(),
                dimension_id: dimension.id.clone(),
                scope,
                link_only: true,
                written,
                ..Self::default()
            };
        };
        let unlimited = a.unlimited == Some(true);
        let window_ms = match (a.period_start_ms, a.period_end_ms) {
            (Some(start), Some(end)) if end > start => Some(end - start),
            _ => window_ms(&dimension.window),
        };
        let used_percent = a.used_percent.or_else(|| match (a.used, a.limit) {
            (Some(used), Some(limit)) if limit > Decimal::ZERO => used
                .checked_div(limit)
                .and_then(|ratio| ratio.checked_mul(Decimal::ONE_HUNDRED)),
            _ => None,
        });
        Self {
            window_id: window_id.to_owned(),
            dimension_id: dimension.id.clone(),
            scope,
            used_percent,
            used: a.used,
            limit: a.limit,
            // An unlimited window has no period worth placing; its reading
            // is kept like one without a reset.
            starts_at_ms: a.period_start_ms.filter(|_| !unlimited),
            resets_at_ms: a.period_end_ms.filter(|_| !unlimited),
            window_ms,
            idle: super::dedupe::idle_window(a, window_ms, now_ms),
            periodic: a.reset_behavior == QuotaResetBehavior::Periodic,
            link_only: false,
            written,
        }
    }

    fn sample(&self, now_ms: i64) -> CycleSample {
        CycleSample {
            used_percent: fixed(self.used_percent),
            used: fixed(self.used),
            limit: fixed(self.limit),
            at_ms: now_ms,
        }
    }

    /// Where the reported period starts: as reported, else one window before
    /// its reset.
    fn period_start(&self, reset: i64) -> Option<i64> {
        self.starts_at_ms
            .or_else(|| self.window_ms.map(|window| reset.saturating_sub(window)))
    }
}

/// What to do with one reading.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Decision {
    /// Change nothing; link the reading to `link`.
    Keep { link: Option<String> },
    /// Adopt boundaries and the reading on the open cycle.
    Observe {
        id: String,
        starts_at_ms: i64,
        ends_at_ms: Option<i64>,
    },
    /// Close the open cycle and open nothing.
    Close { id: String, at_ms: i64 },
    /// Open a cycle for the window, closing `close` first.
    Open {
        close: Option<(String, i64)>,
        opened_by: CycleOpening,
        starts_at_ms: i64,
        ends_at_ms: i64,
        /// Move (or, with nothing to close, fetch) the cost the usage log
        /// shows from here until now into the new cycle.
        backfill_from: Option<i64>,
    },
}

/// The state machine: one reading against its window's open cycle. See the
/// module documentation for the rules.
pub(crate) fn decide(open: Option<&OpenCycle>, facts: &Facts, now_ms: i64) -> Decision {
    let keep = Decision::Keep {
        link: open.map(|c| c.id.clone()),
    };
    let reset = match facts.resets_at_ms {
        Some(reset) if !facts.link_only => reset,
        // Nothing to place a period by: the reading only refreshes the
        // sample, and an upstream that stopped reporting the boundary no
        // longer vouches for it (`observe` marks the cycle local again).
        _ => {
            return match open {
                Some(c) if facts.written && !facts.link_only => Decision::Observe {
                    id: c.id.clone(),
                    starts_at_ms: c.starts_at_ms,
                    ends_at_ms: c.ends_at_ms,
                },
                _ => keep,
            };
        }
    };
    if facts.idle {
        match open {
            None => return keep,
            Some(c) if c.expired(now_ms) => {
                return Decision::Close {
                    id: c.id.clone(),
                    at_ms: c.ends_at_ms.unwrap_or(now_ms),
                };
            }
            Some(c) if c.ends_at_ms.is_some_and(|end| near(end, reset)) => {}
            Some(c) if c.boundary == CycleBoundary::Local => return keep,
            Some(c) => {
                return Decision::Close {
                    id: c.id.clone(),
                    at_ms: now_ms,
                };
            }
        }
    }
    let Some(c) = open else {
        let starts = facts.period_start(reset).unwrap_or(now_ms).min(now_ms);
        return Decision::Open {
            close: None,
            opened_by: CycleOpening::FirstUse,
            starts_at_ms: starts,
            ends_at_ms: reset,
            backfill_from: Some(starts),
        };
    };
    let starts = facts
        .period_start(reset)
        .unwrap_or(c.starts_at_ms)
        .min(reset);
    let Some(end) = c.ends_at_ms else {
        // A `Total` cycle never ends; a reading only refreshes it.
        return if facts.written {
            Decision::Observe {
                id: c.id.clone(),
                starts_at_ms: c.starts_at_ms,
                ends_at_ms: None,
            }
        } else {
            keep
        };
    };
    let first_look = c.sample_at_ms.is_none();
    if near(end, reset) {
        let dropped = facts.periodic
            && !first_look
            && matches!((c.sample_used_percent, facts.used_percent),
                (Some(before), Some(after)) if before - after >= DROP_POINTS);
        if dropped {
            let split = split_point(c, facts, reset, now_ms);
            return server_reset(c, split, reset);
        }
        return if facts.written || c.boundary == CycleBoundary::Local {
            Decision::Observe {
                id: c.id.clone(),
                starts_at_ms: starts,
                ends_at_ms: Some(reset),
            }
        } else {
            keep
        };
    }
    if end <= now_ms {
        return Decision::Open {
            close: Some((c.id.clone(), end)),
            opened_by: CycleOpening::Rollover,
            starts_at_ms: facts.period_start(reset).unwrap_or(now_ms).min(now_ms),
            ends_at_ms: reset,
            backfill_from: None,
        };
    }
    if c.boundary == CycleBoundary::Local {
        return Decision::Observe {
            id: c.id.clone(),
            starts_at_ms: starts,
            ends_at_ms: Some(reset),
        };
    }
    server_reset(c, split_point(c, facts, reset, now_ms), reset)
}

fn server_reset(cycle: &OpenCycle, split: i64, reset: i64) -> Decision {
    Decision::Open {
        close: Some((cycle.id.clone(), split)),
        opened_by: CycleOpening::ServerReset,
        starts_at_ms: split,
        ends_at_ms: reset,
        backfill_from: Some(split),
    }
}

/// Where a server reset splits the old cycle from the new, in order: the
/// upstream's own period start; one window before the new reset, if that
/// falls between the previous observation and this one; this observation.
/// Always inside `[previous observation, now]`, since the reset happened after
/// a reading that still saw the old period.
pub(crate) fn split_point(cycle: &OpenCycle, facts: &Facts, reset: i64, now_ms: i64) -> i64 {
    let lower = cycle.sample_at_ms.unwrap_or(cycle.starts_at_ms).min(now_ms);
    if let Some(start) = facts.starts_at_ms {
        return start.clamp(lower, now_ms);
    }
    if let Some(window) = facts.window_ms {
        let start = reset.saturating_sub(window);
        if (lower..=now_ms).contains(&start) {
            return start;
        }
    }
    now_ms
}

/// Where one reading's observation row points.
pub(crate) struct Link {
    pub cycle_id: String,
    pub cost_usd: FixedDecimal,
}

/// The cycle store behind one engine: Store for the rows, the shared cache for
/// the open-cycle projection.
pub(crate) struct Cycles<'a, C> {
    pub store: &'a Store<C>,
    pub cache: &'a Arc<dyn Cache>,
}

impl<C: BatchConnectionTrait> Cycles<'_, C> {
    /// The credential's open cycles, from the cache or else Store.
    pub async fn open(&self, credential_id: &str) -> CoreResult<Vec<OpenCycle>> {
        let key = keys::credential_cycles(credential_id);
        if let Some(entry) = self.cache.get(&key).await?
            && let Ok(cycles) = serde_json::from_slice(&entry.value)
        {
            return Ok(cycles);
        }
        self.reload(credential_id).await
    }

    /// The credential's open cycles from Store, refreshing the cache.
    pub async fn reload(&self, credential_id: &str) -> CoreResult<Vec<OpenCycle>> {
        let rows = self
            .store
            .credential_cycles()
            .open_of(&[credential_id.to_owned()])
            .await?;
        self.remember(credential_id, &rows).await
    }

    async fn remember(
        &self,
        credential_id: &str,
        rows: &[credential_cycle::Model],
    ) -> CoreResult<Vec<OpenCycle>> {
        let cycles: Vec<OpenCycle> = rows.iter().map(OpenCycle::from_row).collect();
        let value = serde_json::to_vec(&cycles).map_err(|e| CoreError::Rewrite(e.to_string()))?;
        // Last writer wins: every writer stores what Store held after its own
        // change, and Store, not the cache, arbitrates.
        self.cache
            .put(&keys::credential_cycles(credential_id), value, CACHE_TTL)
            .await?;
        Ok(cycles)
    }

    async fn apply(
        &self,
        credential_id: &str,
        changes: Vec<CycleChange>,
    ) -> CoreResult<Vec<credential_cycle::Model>> {
        let (_, rows) = self
            .store
            .credential_cycles()
            .apply(credential_id, changes)
            .await?;
        self.remember(credential_id, &rows).await?;
        Ok(rows)
    }

    /// Settle one exchange: open whatever windows this charge starts, then
    /// add `usd` to every open cycle that covers the model.
    pub async fn accrue(
        &self,
        subject: &CycleSubject<'_>,
        operation: Operation,
        model: Option<&str>,
        usd: Option<Decimal>,
        now_ms: i64,
    ) -> CoreResult<()> {
        let credential_id = subject.credential_id;
        let mut open = self.open(credential_id).await?;
        let changes = charge_changes(subject, &open, operation, model, now_ms);
        if !changes.is_empty() {
            open = self
                .apply(credential_id, changes)
                .await?
                .iter()
                .map(OpenCycle::from_row)
                .collect();
        }
        let Some(amount) = fixed(usd).filter(|amount| amount.atoms() > 0) else {
            return Ok(());
        };
        let targets: Vec<&OpenCycle> = open
            .iter()
            .filter(|c| !c.expired(now_ms) && subject.covers(c, operation, model))
            .collect();
        let ids: Vec<String> = targets.iter().map(|c| c.id.clone()).collect();
        let applied = self.store.credential_cycles().accrue(&ids, amount).await?;
        if applied.iter().all(|applied| *applied) {
            return Ok(());
        }
        // A cycle closed under a stale cache: its window has a successor, or
        // needs one. Place the charge on the windows it has not reached yet.
        let charged: HashSet<&str> = targets
            .iter()
            .zip(&applied)
            .filter(|(_, applied)| **applied)
            .map(|(c, _)| c.window_id.as_str())
            .collect();
        let mut open = self.reload(credential_id).await?;
        let changes = charge_changes(subject, &open, operation, model, now_ms);
        if !changes.is_empty() {
            open = self
                .apply(credential_id, changes)
                .await?
                .iter()
                .map(OpenCycle::from_row)
                .collect();
        }
        let ids: Vec<String> = open
            .iter()
            .filter(|c| {
                !c.expired(now_ms)
                    && subject.covers(c, operation, model)
                    && !charged.contains(c.window_id.as_str())
            })
            .map(|c| c.id.clone())
            .collect();
        let applied = self.store.credential_cycles().accrue(&ids, amount).await?;
        if !applied.iter().all(|applied| *applied) {
            tracing::debug!(
                credential_id,
                "a cycle closed twice under one charge; its share is lost"
            );
        }
        Ok(())
    }

    /// Run the state machine over one observation's readings and return,
    /// per reading, the cycle its row should name.
    pub async fn observe(
        &self,
        credential_id: &str,
        readings: &[Facts],
        now_ms: i64,
    ) -> CoreResult<Vec<Option<Link>>> {
        if readings.is_empty() {
            return Ok(Vec::new());
        }
        let open = self.open(credential_id).await?;
        let mut changes = Vec::new();
        // Per reading: the cycle id to link, and whether it is new here.
        let mut links: Vec<Option<(String, bool)>> = Vec::with_capacity(readings.len());
        let mut seen = HashSet::new();
        for facts in readings {
            // One answer reports a window once; a repeat is ignored rather
            // than planned twice against the same cycle.
            if !seen.insert(facts.window_id.as_str()) {
                links.push(None);
                continue;
            }
            let cycle = open.iter().find(|c| c.window_id == facts.window_id);
            match decide(cycle, facts, now_ms) {
                Decision::Keep { link } => links.push(link.map(|id| (id, false))),
                Decision::Observe {
                    id,
                    starts_at_ms,
                    ends_at_ms,
                } => {
                    changes.push(CycleChange::Observe {
                        id: id.clone(),
                        starts_at_ms,
                        ends_at_ms,
                        boundary: if facts.resets_at_ms.is_some() {
                            CycleBoundary::Observed
                        } else {
                            CycleBoundary::Local
                        },
                        sample: Some(facts.sample(now_ms)),
                    });
                    links.push(Some((id, false)));
                }
                Decision::Close { id, at_ms } => {
                    changes.push(CycleChange::Close {
                        id,
                        at_ms,
                        moved: FixedDecimal::ZERO,
                    });
                    links.push(None);
                }
                Decision::Open {
                    close,
                    opened_by,
                    starts_at_ms,
                    ends_at_ms,
                    backfill_from,
                } => {
                    let moved = match backfill_from {
                        Some(from) => {
                            self.backfill(credential_id, &facts.scope, from, now_ms)
                                .await
                        }
                        None => FixedDecimal::ZERO,
                    };
                    if let Some((id, at_ms)) = close {
                        changes.push(CycleChange::Close {
                            id,
                            at_ms,
                            moved: if opened_by == CycleOpening::ServerReset {
                                moved
                            } else {
                                FixedDecimal::ZERO
                            },
                        });
                    }
                    let id = ids::random_id();
                    changes.push(CycleChange::Open(NewCycle {
                        id: id.clone(),
                        credential_id: credential_id.to_owned(),
                        window_id: facts.window_id.clone(),
                        dimension_id: Some(facts.dimension_id.clone()),
                        scope: scope_json(&facts.scope),
                        starts_at_ms,
                        ends_at_ms: Some(ends_at_ms),
                        boundary: CycleBoundary::Observed,
                        opened_by,
                        cost_usd: moved,
                        sample: Some(facts.sample(now_ms)),
                    }));
                    links.push(Some((id, true)));
                }
            }
        }
        let wants_cost = readings
            .iter()
            .zip(&links)
            .any(|(facts, link)| facts.written && link.is_some());
        let rows = if !changes.is_empty() {
            self.apply(credential_id, changes).await?
        } else if wants_cost {
            self.store
                .credential_cycles()
                .open_of(&[credential_id.to_owned()])
                .await?
        } else {
            return Ok(readings.iter().map(|_| None).collect());
        };
        Ok(readings
            .iter()
            .zip(links)
            .map(|(facts, link)| {
                let (id, new) = link?;
                // An open that lost to a concurrent opener links the winner.
                let row = rows.iter().find(|row| row.id == id).or_else(|| {
                    new.then(|| rows.iter().find(|row| row.window_id == facts.window_id))
                        .flatten()
                })?;
                Some(Link {
                    cycle_id: row.id.clone(),
                    cost_usd: row.cost_usd,
                })
            })
            .collect())
    }

    /// USD the usage log shows this credential spent under `scope` since
    /// `from_ms`. Bounded; a truncated sum is logged and used as the lower
    /// bound it is, and a failed read moves nothing.
    async fn backfill(
        &self,
        credential_id: &str,
        scope: &QuotaScope,
        from_ms: i64,
        now_ms: i64,
    ) -> FixedDecimal {
        if from_ms >= now_ms || matches!(scope, QuotaScope::Unknown) {
            return FixedDecimal::ZERO;
        }
        match usage_scan::credential_usd_cost(
            self.store,
            credential_id,
            from_ms,
            now_ms,
            |model| scope_covers(scope, model),
            usage_scan::MAX_SCAN_ROWS,
        )
        .await
        {
            Ok((amount, truncated)) => {
                if truncated {
                    tracing::warn!(
                        credential_id,
                        from_ms,
                        "cycle backfill hit the scan cap; the moved cost is a lower bound"
                    );
                }
                fixed(Some(amount)).unwrap_or(FixedDecimal::ZERO)
            }
            Err(error) => {
                tracing::warn!(credential_id, %error, "cycle backfill failed; nothing moved");
                FixedDecimal::ZERO
            }
        }
    }

    /// A redeemed reset credit reopened the windows in `clears` (every
    /// window when empty): close their cycles now and open `manual_reset`
    /// ones in their place. The calendar-month cycle of an undeclared
    /// credential is not an upstream window and is left alone.
    pub async fn manual_reset(
        &self,
        subject: &CycleSubject<'_>,
        clears: &[String],
        now_ms: i64,
    ) -> CoreResult<()> {
        let credential_id = subject.credential_id;
        let open = self.reload(credential_id).await?;
        let mut changes = Vec::new();
        for cycle in &open {
            let Some(dimension_id) = cycle.dimension_id.as_deref() else {
                continue;
            };
            if !clears.is_empty()
                && !clears
                    .iter()
                    .any(|id| id == dimension_id || *id == cycle.window_id)
            {
                continue;
            }
            changes.push(CycleChange::Close {
                id: cycle.id.clone(),
                at_ms: now_ms,
                moved: FixedDecimal::ZERO,
            });
            let Some(dimension) = subject.dimension(Some(dimension_id)) else {
                continue;
            };
            let (starts_at_ms, ends_at_ms) = local_bounds(Some(dimension), now_ms);
            changes.push(CycleChange::Open(NewCycle {
                id: ids::random_id(),
                credential_id: credential_id.to_owned(),
                window_id: cycle.window_id.clone(),
                dimension_id: Some(dimension_id.to_owned()),
                scope: scope_json(&cycle.scope),
                starts_at_ms: starts_at_ms.max(now_ms),
                ends_at_ms,
                boundary: CycleBoundary::Local,
                opened_by: CycleOpening::ManualReset,
                cost_usd: FixedDecimal::ZERO,
                sample: None,
            }));
        }
        if !changes.is_empty() {
            self.apply(credential_id, changes).await?;
        }
        Ok(())
    }
}

/// The opens and rollovers a charge for this operation and model needs:
/// every expired cycle it would land in rolls over, and every covering
/// window without a live cycle gets one.
fn charge_changes(
    subject: &CycleSubject<'_>,
    open: &[OpenCycle],
    operation: Operation,
    model: Option<&str>,
    now_ms: i64,
) -> Vec<CycleChange> {
    let mut changes = Vec::new();
    let mut live: HashSet<Option<&str>> = HashSet::new();
    let credential_id = subject.credential_id;
    let fresh = |cycle_window: &str,
                 dimension: Option<&QuotaDimension>,
                 scope: &QuotaScope,
                 opened_by: CycleOpening| {
        let (starts_at_ms, ends_at_ms) = local_bounds(dimension, now_ms);
        CycleChange::Open(NewCycle {
            id: ids::random_id(),
            credential_id: credential_id.to_owned(),
            window_id: cycle_window.to_owned(),
            dimension_id: dimension.map(|d| d.id.clone()),
            scope: scope_json(scope),
            starts_at_ms,
            ends_at_ms,
            boundary: CycleBoundary::Local,
            opened_by,
            cost_usd: FixedDecimal::ZERO,
            sample: None,
        })
    };
    for cycle in open {
        if !cycle.expired(now_ms) {
            live.insert(cycle.dimension_id.as_deref());
            continue;
        }
        if !subject.covers(cycle, operation, model) {
            continue;
        }
        changes.push(CycleChange::Close {
            id: cycle.id.clone(),
            at_ms: cycle.ends_at_ms.unwrap_or(now_ms),
            moved: FixedDecimal::ZERO,
        });
        let dimension = subject.dimension(cycle.dimension_id.as_deref());
        // A window whose declaration went away just closes, and so does the
        // month cycle of a credential that has since declared windows.
        let orphaned = match cycle.dimension_id {
            Some(_) => dimension.is_none(),
            None => !subject.dimensions.is_empty(),
        };
        if orphaned {
            continue;
        }
        changes.push(fresh(
            &cycle.window_id,
            dimension,
            &cycle.scope,
            CycleOpening::Rollover,
        ));
        live.insert(cycle.dimension_id.as_deref());
    }
    if subject.dimensions.is_empty() {
        if !live.contains(&None) {
            changes.push(fresh(
                MONTH_WINDOW,
                None,
                &QuotaScope::All,
                CycleOpening::FirstUse,
            ));
        }
        return changes;
    }
    for dimension in &subject.dimensions {
        let applies = scope_covers(&dimension.scope, model)
            && dimension
                .operations
                .as_ref()
                .is_none_or(|ops| ops.contains(&operation));
        if applies && !live.contains(&Some(dimension.id.as_str())) {
            changes.push(fresh(
                &dimension.id,
                Some(dimension),
                &dimension.scope,
                CycleOpening::FirstUse,
            ));
        }
    }
    changes
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use gproxy_channel::channel::QuotaMetric;
    use gproxy_store::entity::usage::usage_record;
    use sea_orm::{ConnectOptions, Database, DatabaseConnection, Set};
    use serde_json::json;

    const HOUR: i64 = 60 * 60 * 1000;
    const MINUTE: i64 = 60 * 1000;
    const NOW: i64 = 1_789_812_000_000;
    const WINDOW: i64 = 5 * HOUR;

    fn cycle(boundary: CycleBoundary, starts: i64, ends: i64) -> OpenCycle {
        OpenCycle {
            id: "c1".into(),
            window_id: "five_hour".into(),
            dimension_id: Some("five_hour".into()),
            scope: QuotaScope::All,
            starts_at_ms: starts,
            ends_at_ms: Some(ends),
            boundary,
            opened_by: CycleOpening::FirstUse,
            sample_used_percent: Some(40.into()),
            sample_at_ms: Some(starts + MINUTE),
        }
    }

    fn facts(percent: i64, reset: i64) -> Facts {
        Facts {
            window_id: "five_hour".into(),
            dimension_id: "five_hour".into(),
            scope: QuotaScope::All,
            used_percent: Some(percent.into()),
            resets_at_ms: Some(reset),
            window_ms: Some(WINDOW),
            idle: percent == 0 && near(reset, NOW + WINDOW),
            periodic: true,
            written: true,
            ..Facts::default()
        }
    }

    fn opened(decision: &Decision) -> Option<(CycleOpening, i64)> {
        match decision {
            Decision::Open {
                opened_by,
                starts_at_ms,
                ..
            } => Some((*opened_by, *starts_at_ms)),
            _ => None,
        }
    }

    /// One row per rule of the state machine.
    #[test]
    fn the_state_machine_table() {
        let started = NOW - HOUR;
        let observed = cycle(CycleBoundary::Observed, started, started + WINDOW);
        // No open cycle, a started window: open observed, backfilled.
        let decision = decide(None, &facts(10, started + WINDOW), NOW);
        assert_eq!(
            decision,
            Decision::Open {
                close: None,
                opened_by: CycleOpening::FirstUse,
                starts_at_ms: started,
                ends_at_ms: started + WINDOW,
                backfill_from: Some(started),
            }
        );
        // Same reset: adopt it and the reading.
        assert_eq!(
            decide(Some(&observed), &facts(45, started + WINDOW + MINUTE), NOW),
            Decision::Observe {
                id: "c1".into(),
                starts_at_ms: started + MINUTE,
                ends_at_ms: Some(started + WINDOW + MINUTE),
            }
        );
        // Same reset, unchanged and not persisted: nothing to write.
        let mut quiet = facts(40, started + WINDOW);
        quiet.written = false;
        assert_eq!(
            decide(Some(&observed), &quiet, NOW),
            Decision::Keep {
                link: Some("c1".into())
            }
        );
        // Far reset, old end passed: rollover at the old end.
        let ended = cycle(CycleBoundary::Observed, NOW - 6 * HOUR, NOW - HOUR);
        let decision = decide(Some(&ended), &facts(3, NOW + 4 * HOUR), NOW);
        assert!(
            matches!(&decision, Decision::Open { close: Some((id, at)), opened_by: CycleOpening::Rollover, backfill_from: None, .. } if id == "c1" && *at == NOW - HOUR),
            "{decision:?}"
        );
        // Far reset, old end ahead: a server reset.
        let decision = decide(Some(&observed), &facts(2, NOW + WINDOW - MINUTE), NOW);
        assert_eq!(opened(&decision).unwrap().0, CycleOpening::ServerReset);
        // Same reset but a periodic window lost ten points: a server reset.
        let decision = decide(Some(&observed), &facts(30, started + WINDOW), NOW);
        assert_eq!(opened(&decision).unwrap().0, CycleOpening::ServerReset);
        // Nine points is noise, and a recovering window may fall on its own.
        assert!(matches!(
            decide(Some(&observed), &facts(31, started + WINDOW), NOW),
            Decision::Observe { .. }
        ));
        let mut recovering = facts(0, started + WINDOW);
        recovering.used_percent = Some(5.into());
        recovering.periodic = false;
        assert!(matches!(
            decide(Some(&observed), &recovering, NOW),
            Decision::Observe { .. }
        ));
    }

    /// The first observation after a manual reset corrects the guessed
    /// boundary; it must not be read as another reset.
    #[test]
    fn a_manual_reset_is_not_split_again_by_its_first_observation() {
        let mut manual = cycle(CycleBoundary::Local, NOW - MINUTE, NOW - MINUTE + WINDOW);
        manual.opened_by = CycleOpening::ManualReset;
        manual.sample_at_ms = None;
        manual.sample_used_percent = None;
        let decision = decide(Some(&manual), &facts(1, NOW + 2 * HOUR), NOW);
        assert_eq!(
            decision,
            Decision::Observe {
                id: "c1".into(),
                starts_at_ms: NOW + 2 * HOUR - WINDOW,
                ends_at_ms: Some(NOW + 2 * HOUR),
            }
        );
    }

    #[test]
    fn a_window_not_started_opens_nothing() {
        let idle = facts(0, NOW + WINDOW + 3 * MINUTE);
        assert!(idle.idle);
        // No cycle: nothing to open.
        assert_eq!(decide(None, &idle, NOW), Decision::Keep { link: None });
        // An expired cycle closes at its end, and nothing replaces it.
        let ended = cycle(CycleBoundary::Observed, NOW - 6 * HOUR, NOW - HOUR);
        assert_eq!(
            decide(Some(&ended), &idle, NOW),
            Decision::Close {
                id: "c1".into(),
                at_ms: NOW - HOUR
            }
        );
        // A cycle a local first use just opened is ahead of the upstream.
        let fresh = cycle(
            CycleBoundary::Local,
            NOW - 10 * MINUTE,
            NOW - 10 * MINUTE + WINDOW,
        );
        assert_eq!(
            decide(Some(&fresh), &idle, NOW),
            Decision::Keep {
                link: Some("c1".into())
            }
        );
    }

    #[test]
    fn five_minutes_is_the_same_period_and_a_millisecond_more_is_not() {
        let started = NOW - HOUR;
        let observed = cycle(CycleBoundary::Observed, started, started + WINDOW);
        let end = started + WINDOW;
        assert!(matches!(
            decide(Some(&observed), &facts(41, end + TOLERANCE_MS), NOW),
            Decision::Observe { .. }
        ));
        assert!(matches!(
            decide(Some(&observed), &facts(41, end - TOLERANCE_MS), NOW),
            Decision::Observe { .. }
        ));
        assert_eq!(
            opened(&decide(
                Some(&observed),
                &facts(41, end + TOLERANCE_MS + 1),
                NOW
            ))
            .unwrap()
            .0,
            CycleOpening::ServerReset
        );
    }

    #[test]
    fn a_server_reset_splits_where_the_new_period_began() {
        let observed = cycle(CycleBoundary::Observed, NOW - 3 * HOUR, NOW + 2 * HOUR);
        let previous = observed.sample_at_ms.unwrap();
        // The upstream's own start wins, kept inside the observation gap.
        let mut reported = facts(1, NOW + 4 * HOUR);
        reported.starts_at_ms = Some(NOW - HOUR);
        assert_eq!(
            split_point(&observed, &reported, NOW + 4 * HOUR, NOW),
            NOW - HOUR
        );
        reported.starts_at_ms = Some(previous - HOUR);
        assert_eq!(
            split_point(&observed, &reported, NOW + 4 * HOUR, NOW),
            previous
        );
        // Else one window before the new reset, when that falls in the gap.
        let derived = facts(1, NOW + 4 * HOUR);
        assert_eq!(
            split_point(&observed, &derived, NOW + 4 * HOUR, NOW),
            NOW - HOUR
        );
        // Else now.
        assert_eq!(split_point(&observed, &derived, NOW + 6 * HOUR, NOW), NOW);
    }

    async fn fixture() -> (Store<DatabaseConnection>, Arc<dyn Cache>) {
        let mut options = ConnectOptions::new("sqlite::memory:");
        options.max_connections(1).sqlx_logging(false);
        let store = Store::new(Database::connect(options).await.unwrap());
        store.sync().await.unwrap();
        (store, Arc::new(gproxy_cache::MemoryCache::default()))
    }

    fn dimension(id: &str, hours: i64, scope: QuotaScope) -> QuotaDimension {
        QuotaDimension {
            id: id.into(),
            label: None,
            scope,
            operations: None,
            metric: QuotaMetric::Unit("percent".into()),
            window: QuotaWindow::Rolling {
                seconds: hours * 3600,
            },
            limit: None,
            tracking: QuotaTracking::Reported,
            blocking: true,
        }
    }

    fn claude() -> Vec<QuotaDimension> {
        vec![
            dimension("five_hour", 5, QuotaScope::All),
            dimension("seven_day", 168, QuotaScope::All),
            dimension(
                "seven_day_fable",
                168,
                QuotaScope::ModelPrefixes(vec!["claude-fable".into()]),
            ),
        ]
    }

    fn subject(dimensions: &[QuotaDimension]) -> CycleSubject<'_> {
        CycleSubject {
            credential_id: "c",
            dimensions: dimensions.iter().collect(),
        }
    }

    async fn costs(store: &Store<DatabaseConnection>) -> Vec<(String, String)> {
        let mut rows: Vec<_> = store
            .credential_cycles()
            .open_of(&["c".into()])
            .await
            .unwrap()
            .into_iter()
            .map(|row| (row.window_id, row.cost_usd.to_string()))
            .collect();
        rows.sort();
        rows
    }

    fn usd(amount: &str) -> Option<Decimal> {
        Some(amount.parse().unwrap())
    }

    #[tokio::test]
    async fn a_request_accrues_to_every_window_whose_declared_scope_covers_it() {
        let (store, cache) = fixture().await;
        let cycles = Cycles {
            store: &store,
            cache: &cache,
        };
        let dimensions = claude();
        let op = Operation::GenerateContent;
        cycles
            .accrue(
                &subject(&dimensions),
                op,
                Some("claude-fable-5"),
                usd("1.5"),
                NOW,
            )
            .await
            .unwrap();
        cycles
            .accrue(
                &subject(&dimensions),
                op,
                Some("claude-haiku-4"),
                usd("0.25"),
                NOW + 1,
            )
            .await
            .unwrap();
        assert_eq!(
            costs(&store).await,
            [
                ("five_hour".into(), "1.75".into()),
                ("seven_day".into(), "1.75".into()),
                ("seven_day_fable".into(), "1.5".into()),
            ]
        );
        // A local rolling cycle starts at first use.
        let rows = store
            .credential_cycles()
            .open_of(&["c".into()])
            .await
            .unwrap();
        let five = rows.iter().find(|r| r.window_id == "five_hour").unwrap();
        assert_eq!(
            (five.starts_at_ms, five.ends_at_ms),
            (NOW, Some(NOW + WINDOW))
        );
        assert_eq!(five.boundary, CycleBoundary::Local);
        // Past its end, the next charge rolls it over.
        cycles
            .accrue(
                &subject(&dimensions),
                op,
                Some("claude-haiku-4"),
                usd("2"),
                NOW + WINDOW,
            )
            .await
            .unwrap();
        let rows = store
            .credential_cycles()
            .open_of(&["c".into()])
            .await
            .unwrap();
        let five = rows.iter().find(|r| r.window_id == "five_hour").unwrap();
        assert_eq!(five.opened_by, CycleOpening::Rollover);
        assert_eq!(five.cost_usd.to_string(), "2");
    }

    #[tokio::test]
    async fn a_credential_without_windows_is_cut_by_calendar_month() {
        let (store, cache) = fixture().await;
        let cycles = Cycles {
            store: &store,
            cache: &cache,
        };
        cycles
            .accrue(
                &subject(&[]),
                Operation::GenerateContent,
                None,
                usd("3"),
                NOW,
            )
            .await
            .unwrap();
        let rows = store
            .credential_cycles()
            .open_of(&["c".into()])
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].window_id, MONTH_WINDOW);
        assert_eq!(rows[0].dimension_id, None);
        // 2026-09-01 .. 2026-10-01 UTC.
        assert_eq!(rows[0].starts_at_ms, 1_788_220_800_000);
        assert_eq!(rows[0].ends_at_ms, Some(1_790_812_800_000));
        assert_eq!(rows[0].cost_usd.to_string(), "3");
    }

    /// Two instances with their own caches race to open the same windows.
    #[tokio::test]
    async fn concurrent_first_charges_open_one_cycle_per_window() {
        let (store, _) = fixture().await;
        let (one, two): (Arc<dyn Cache>, Arc<dyn Cache>) = (
            Arc::new(gproxy_cache::MemoryCache::default()),
            Arc::new(gproxy_cache::MemoryCache::default()),
        );
        let dimensions = claude();
        let charge = |cache| {
            let store = &store;
            let dimensions = &dimensions;
            async move {
                Cycles { store, cache }
                    .accrue(
                        &subject(dimensions),
                        Operation::GenerateContent,
                        Some("claude-haiku-4"),
                        usd("1"),
                        NOW,
                    )
                    .await
                    .unwrap()
            }
        };
        tokio::join!(charge(&one), charge(&two));
        assert_eq!(
            costs(&store).await,
            [
                ("five_hour".into(), "2".into()),
                ("seven_day".into(), "2".into()),
            ]
        );
        let all = store.credential_cycles().recent("c", 10).await.unwrap();
        assert_eq!(all.len(), 2, "no losing opener left a row behind");
    }

    /// A charge that lands on a cycle another instance closed is placed on
    /// the window's successor.
    #[tokio::test]
    async fn a_stale_cache_does_not_lose_a_charge() {
        let (store, cache) = fixture().await;
        let cycles = Cycles {
            store: &store,
            cache: &cache,
        };
        let dimensions = vec![dimension("five_hour", 5, QuotaScope::All)];
        let op = Operation::GenerateContent;
        cycles
            .accrue(&subject(&dimensions), op, None, usd("1"), NOW)
            .await
            .unwrap();
        let id = store
            .credential_cycles()
            .open_of(&["c".into()])
            .await
            .unwrap()[0]
            .id
            .clone();
        store
            .credential_cycles()
            .apply(
                "c",
                vec![CycleChange::Close {
                    id,
                    at_ms: NOW + 1,
                    moved: FixedDecimal::ZERO,
                }],
            )
            .await
            .unwrap();
        cycles
            .accrue(&subject(&dimensions), op, None, usd("2"), NOW + 2)
            .await
            .unwrap();
        assert_eq!(costs(&store).await, [("five_hour".into(), "2".into())]);
    }

    async fn spent(store: &Store<DatabaseConnection>, id: &str, at: i64, model: &str, cost: &str) {
        store
            .usage_records()
            .create_many(vec![usage_record::ActiveModel {
                request_id: Set(id.into()),
                model: Set(model.into()),
                operation: Set("generate_content".into()),
                side: Set(gproxy_store::entity::usage::capture_record::CaptureSide::Upstream),
                credential_id: Set(Some("c".into())),
                cost: Set(Some(cost.parse().unwrap())),
                metrics: Set(json!({})),
                started_at_ms: Set(at),
                ..Default::default()
            }])
            .await
            .unwrap();
    }

    /// A request that was both logged and settled against the cycles.
    async fn charge(
        cycles: &Cycles<'_, DatabaseConnection>,
        dimensions: &[QuotaDimension],
        id: &str,
        at: i64,
        cost: &str,
    ) {
        spent(cycles.store, id, at, "m", cost).await;
        cycles
            .accrue(
                &subject(dimensions),
                Operation::GenerateContent,
                Some("m"),
                usd(cost),
                at,
            )
            .await
            .unwrap();
    }

    fn observation(dimension: &QuotaDimension, percent: i64, reset: i64, now: i64) -> Facts {
        Facts::new(
            &dimension.id,
            dimension,
            dimension.scope.clone(),
            Some(&QuotaAllowance {
                used_percent: Some(percent.into()),
                period_end_ms: Some(reset),
                reset_behavior: QuotaResetBehavior::Periodic,
                ..Default::default()
            }),
            true,
            now,
        )
    }

    #[tokio::test]
    async fn a_server_reset_moves_what_was_spent_after_the_split() {
        let (store, cache) = fixture().await;
        let cycles = Cycles {
            store: &store,
            cache: &cache,
        };
        let dimensions = vec![dimension("five_hour", 5, QuotaScope::All)];
        let five = &dimensions[0];
        let start = NOW - 3 * HOUR;
        // Spent before this deployment saw the window: backfilled on open.
        spent(&store, "r0", start + MINUTE, "m", "0.5").await;
        let links = cycles
            .observe(
                "c",
                &[observation(five, 20, start + WINDOW, NOW - 2 * HOUR)],
                NOW - 2 * HOUR,
            )
            .await
            .unwrap();
        let link = links[0].as_ref().unwrap();
        assert_eq!(link.cost_usd.to_string(), "0.5");
        let old = link.cycle_id.clone();
        charge(&cycles, &dimensions, "r1", NOW - 90 * MINUTE, "1").await;
        let links = cycles
            .observe(
                "c",
                &[observation(five, 30, start + WINDOW, NOW - 80 * MINUTE)],
                NOW - 80 * MINUTE,
            )
            .await
            .unwrap();
        assert_eq!(links[0].as_ref().unwrap().cost_usd.to_string(), "1.5");
        charge(&cycles, &dimensions, "r2", NOW - 30 * MINUTE, "2").await;
        // The upstream reset an hour ago: its new window ends five hours
        // after that, which falls between the two observations.
        let links = cycles
            .observe("c", &[observation(five, 5, NOW - HOUR + WINDOW, NOW)], NOW)
            .await
            .unwrap();
        let new = links[0].as_ref().unwrap();
        assert_ne!(new.cycle_id, old);
        assert_eq!(new.cost_usd.to_string(), "2", "r2 moved into the new cycle");
        let history = store.credential_cycles().recent("c", 5).await.unwrap();
        let closed = history.iter().find(|row| row.id == old).unwrap();
        assert_eq!(closed.closed_at_ms, Some(NOW - HOUR));
        assert_eq!(closed.cost_usd.to_string(), "1.5");
        let open = history.iter().find(|row| row.id == new.cycle_id).unwrap();
        assert_eq!(open.opened_by, CycleOpening::ServerReset);
        assert_eq!(open.starts_at_ms, NOW - HOUR);
        assert_eq!(
            open.sample_cost_usd.map(|c| c.to_string()),
            Some("2".into())
        );
    }

    #[tokio::test]
    async fn a_manual_reset_reopens_only_the_cleared_windows() {
        let (store, cache) = fixture().await;
        let cycles = Cycles {
            store: &store,
            cache: &cache,
        };
        let dimensions = claude();
        cycles
            .accrue(
                &subject(&dimensions),
                Operation::GenerateContent,
                Some("claude-fable-5"),
                usd("1"),
                NOW,
            )
            .await
            .unwrap();
        cycles
            .manual_reset(&subject(&dimensions), &["five_hour".into()], NOW + HOUR)
            .await
            .unwrap();
        let open = store
            .credential_cycles()
            .open_of(&["c".into()])
            .await
            .unwrap();
        let five = open.iter().find(|r| r.window_id == "five_hour").unwrap();
        assert_eq!(five.opened_by, CycleOpening::ManualReset);
        assert_eq!(
            (five.starts_at_ms, five.cost_usd),
            (NOW + HOUR, FixedDecimal::ZERO)
        );
        assert!(
            open.iter()
                .filter(|r| r.window_id != "five_hour")
                .all(|r| r.opened_by == CycleOpening::FirstUse)
        );
        // Its first observation corrects the boundary instead of splitting.
        let links = cycles
            .observe(
                "c",
                &[observation(
                    &dimensions[0],
                    1,
                    NOW + 3 * HOUR,
                    NOW + 2 * HOUR,
                )],
                NOW + 2 * HOUR,
            )
            .await
            .unwrap();
        assert_eq!(links[0].as_ref().unwrap().cycle_id, five.id);
        // An empty `clears` is every window.
        cycles
            .manual_reset(&subject(&dimensions), &[], NOW + 3 * HOUR)
            .await
            .unwrap();
        let open = store
            .credential_cycles()
            .open_of(&["c".into()])
            .await
            .unwrap();
        assert_eq!(open.len(), 3);
        assert!(
            open.iter()
                .all(|r| r.opened_by == CycleOpening::ManualReset
                    && r.starts_at_ms == NOW + 3 * HOUR)
        );
    }
}
