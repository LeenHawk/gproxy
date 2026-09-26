//! Account quota observation and self-counted consumption.
//!
//! Reported dimensions get their values from `QuotaHeaders` after every
//! upstream answer and from `QuotaQuery` on demand; every reading moves the
//! credential's cycles (see `cycles`), each entry that changed (see `dedupe`)
//! is persisted as a `credential_quota_cycles` observation row and an
//! exhausted entry becomes a `QuotaExhausted` block until the upstream's
//! period end (or one window derived from the dimension). Counted dimensions are metered in Store's
//! `counted_windows` rows: requests before the exchange, tokens and priced
//! USD cost after usage settles. Operator limits (`credential_limit`) are
//! Counted dimensions too, with a model filter of their own.

use crate::{
    BlockSource, Core, CoreError, CoreResult, CredentialBlock, CredentialData, RequestContext,
    UsageReport, api::lifecycle::now_ms, credential_limit::counted_units, ids,
};
use gproxy_cache::Cache;
use gproxy_channel::{
    ChannelError,
    channel::{
        CredentialContext, QuotaAllowance, QuotaDimension, QuotaEntry, QuotaHeaderContext,
        QuotaMetric, QuotaModel, QuotaScope, QuotaSnapshot, QuotaTracking, QuotaValue, QuotaWindow,
        classify_by_id,
    },
};
use gproxy_protocol::{Operation, OperationKey, capability::CapabilityFuture};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{
    Store,
    entity::limits::{credential_block, credential_quota_cycle},
    operations::counted::{CountedCharge, CountedOutcome},
};
use rust_decimal::Decimal;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Set};
use std::{borrow::Cow, sync::Arc};
use time::{Duration as TimeDuration, OffsetDateTime};

pub(crate) mod cycles;
mod dedupe;

/// `Total` windows never reset; the counter still needs a cache TTL.
const TOTAL_WINDOW_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// Cache TTL of a blocks entry left without blocks after a reset.
const EMPTY_BLOCKS_CACHE_TTL: std::time::Duration =
    std::time::Duration::from_secs(7 * 24 * 60 * 60);

/// Whether `dimension` covers this operation and model. An operator limit
/// behind the dimension may narrow it further by its model glob.
fn dimension_applies(
    credential: &CredentialData,
    dimension: &QuotaDimension,
    operation: Operation,
    model: Option<&str>,
) -> bool {
    let operation_ok = dimension
        .operations
        .as_ref()
        .is_none_or(|ops| ops.contains(&operation));
    let scope_ok = match &dimension.scope {
        QuotaScope::All => true,
        QuotaScope::Unknown => false,
        scoped => model.is_some_and(|m| scoped.matches(m)),
    };
    let limit_ok = credential
        .limits
        .iter()
        .find(|l| l.dimension_id == dimension.id)
        .is_none_or(|l| l.applies_to(model));
    operation_ok && scope_ok && limit_ok
}

/// The channel's quota model for this credential's provider, if it has one.
pub(crate) fn quota_model<'a>(
    data: &'a crate::CoreData,
    credential: &CredentialData,
) -> Option<&'a dyn QuotaModel> {
    data.providers
        .get(&credential.provider_id)
        .and_then(|provider| provider.channel.quota_model())
}

/// The credential's dimension an observed entry reports on, placed by the
/// channel's own rule; without a quota model entries match by id.
pub(crate) fn classify<'d>(
    model: Option<&dyn QuotaModel>,
    credential: &'d CredentialData,
    entry: &QuotaEntry,
) -> Option<Cow<'d, QuotaDimension>> {
    match model {
        Some(model) => model.classify(&credential.quota, entry),
        None => classify_by_id(&credential.quota, entry),
    }
}

fn allowance(value: &QuotaValue) -> Option<&QuotaAllowance> {
    match value {
        QuotaValue::Window(a) | QuotaValue::RateLimit(a) | QuotaValue::Budget(a) => Some(a),
        QuotaValue::Balance(_) | QuotaValue::Breakdown(_) => None,
    }
}

/// Exhaustion is a positive statement from the upstream: zero remaining, used
/// at or over a known limit, or 100%. Unreported values never count as zero.
fn exhausted(value: &QuotaValue) -> bool {
    match value {
        QuotaValue::Balance(balance) => balance.remaining.is_some_and(|r| r <= Decimal::ZERO),
        other => {
            let Some(a) = allowance(other) else {
                return false;
            };
            if a.unlimited == Some(true) {
                return false;
            }
            a.remaining.is_some_and(|r| r <= Decimal::ZERO)
                || matches!((a.used, a.limit), (Some(used), Some(limit)) if used >= limit)
                || a.used_percent.is_some_and(|p| p >= Decimal::ONE_HUNDRED)
        }
    }
}

fn datetime(ms: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

fn millis(at: OffsetDateTime) -> i64 {
    i64::try_from(at.unix_timestamp_nanos() / 1_000_000).unwrap_or(i64::MAX)
}

/// `[start, end)` of the window containing `now`. Rolling windows without an
/// upstream reset time are aligned like fixed epoch windows; the design leaves
/// their true start open and this keeps counting deterministic across peers.
pub(crate) fn window_bounds(window: &QuotaWindow, now_ms: i64) -> (i64, i64) {
    match window {
        QuotaWindow::Rolling { seconds } => aligned(*seconds, 0, now_ms),
        QuotaWindow::Fixed {
            seconds,
            anchor_at_ms,
        } => aligned(*seconds, anchor_at_ms.unwrap_or(0), now_ms),
        QuotaWindow::CalendarDay => {
            let start = datetime(now_ms).replace_time(time::Time::MIDNIGHT);
            (millis(start), millis(start + TimeDuration::days(1)))
        }
        QuotaWindow::CalendarWeek => {
            let today = datetime(now_ms).replace_time(time::Time::MIDNIGHT);
            let back = today.weekday().number_days_from_monday();
            let start = today - TimeDuration::days(i64::from(back));
            (millis(start), millis(start + TimeDuration::weeks(1)))
        }
        QuotaWindow::CalendarMonth => {
            let now = datetime(now_ms);
            let start = now
                .replace_day(1)
                .unwrap_or(now)
                .replace_time(time::Time::MIDNIGHT);
            let next = if start.month() == time::Month::December {
                start
                    .replace_year(start.year() + 1)
                    .and_then(|d| d.replace_month(time::Month::January))
            } else {
                start.replace_month(start.month().next())
            }
            .unwrap_or(start + TimeDuration::days(31));
            (millis(start), millis(next))
        }
        QuotaWindow::Total => (0, now_ms.saturating_add(TOTAL_WINDOW_TTL_MS)),
    }
}

fn aligned(seconds: i64, anchor_ms: i64, now_ms: i64) -> (i64, i64) {
    let length = seconds.max(1).saturating_mul(1000);
    let offset = (now_ms - anchor_ms).rem_euclid(length);
    let start = now_ms - offset;
    (start, start.saturating_add(length))
}

fn decimal_json(value: Option<Decimal>) -> serde_json::Value {
    value.map_or(serde_json::Value::Null, |d| {
        serde_json::Value::String(d.to_string())
    })
}

/// A stable JSON rendering of an observation for the cycle row.
fn snapshot_json(entry: &QuotaEntry) -> serde_json::Value {
    let (kind, a) = match &entry.value {
        QuotaValue::Window(a) => ("window", Some(a)),
        QuotaValue::RateLimit(a) => ("rate_limit", Some(a)),
        QuotaValue::Budget(a) => ("budget", Some(a)),
        QuotaValue::Breakdown(rows) => {
            return serde_json::json!({
                "id": entry.id, "source_id": entry.source_id, "label": entry.label,
                "kind": "breakdown", "breakdown": rows,
            });
        }
        QuotaValue::Balance(b) => {
            return serde_json::json!({
                "id": entry.id,
                "source_id": entry.source_id,
                "label": entry.label,
                "subject": format!("{:?}", entry.subject).to_lowercase(),
                "kind": "balance",
                "remaining": decimal_json(b.remaining),
                "unit": b.unit,
            });
        }
    };
    let a = a.expect("allowance");
    serde_json::json!({
        "id": entry.id,
        "source_id": entry.source_id,
        "label": entry.label,
        "subject": format!("{:?}", entry.subject).to_lowercase(),
        "kind": kind,
        "used": decimal_json(a.used),
        "limit": decimal_json(a.limit),
        "remaining": decimal_json(a.remaining),
        "used_percent": decimal_json(a.used_percent),
        "unlimited": a.unlimited,
        "unit": a.unit,
        "period_start_ms": a.period_start_ms,
        "period_end_ms": a.period_end_ms,
        "reset_behavior": format!("{:?}", a.reset_behavior).to_lowercase(),
    })
}

/// Blocks derived from one exhausted entry: one per declared operation, or a
/// single operation-wide block. The scope is the dimension's declaration; an
/// entry's own scope stands in when the dimension does not know its models.
fn exhaustion_blocks(
    dimension: &QuotaDimension,
    entry: &QuotaEntry,
    cycle_id: &str,
    now_ms: i64,
) -> Vec<CredentialBlock> {
    let until = allowance(&entry.value)
        .and_then(|a| a.period_end_ms)
        .filter(|end| *end > now_ms)
        .unwrap_or_else(|| window_bounds(&dimension.window, now_ms).1);
    let scope = match &dimension.scope {
        QuotaScope::Unknown => entry.model_scope.clone(),
        scope => scope.clone(),
    };
    let source = BlockSource::QuotaExhausted {
        dimension: dimension.id.clone(),
        cycle_id: Some(cycle_id.to_owned()),
    };
    let block = |operation| CredentialBlock {
        scope: scope.clone(),
        operation,
        until_ms: until,
        source: source.clone(),
        observed_at_ms: now_ms,
    };
    match &dimension.operations {
        Some(ops) if !ops.is_empty() => ops.iter().map(|op| block(Some(*op))).collect(),
        _ => vec![block(None)],
    }
}

impl<C: BatchConnectionTrait> Core<C> {
    /// Persist the entries that changed as observation rows, move the
    /// credential's cycles by every reading (persisted or not), and block on
    /// the exhausted ones that match a Reported dimension. Returns the
    /// blocks written.
    pub(crate) async fn observe_quota(
        &self,
        credential: &CredentialData,
        entries: &[QuotaEntry],
        now_ms: i64,
    ) -> CoreResult<Vec<CredentialBlock>> {
        if entries.is_empty() {
            return Ok(Vec::new());
        }
        let latest = self.persisted_quota_observations(&credential.id).await?;
        let data = self.snapshot();
        let model = quota_model(&data, credential);
        let subject = cycles::CycleSubject::of(credential);
        let mut readings = Vec::with_capacity(entries.len());
        let mut facts = Vec::new();
        for entry in entries {
            let reading = dedupe::Persisted::new(&credential.quota, entry, now_ms);
            // An exhausted entry always gets its row: its blocks name it.
            let exhausted = exhausted(&entry.value);
            let write = exhausted
                || !latest
                    .iter()
                    .find(|p| p.id() == entry.id)
                    .is_some_and(|p| !p.needs_write(&reading));
            let dimension = classify(model, credential, entry);
            // Only declared billing windows keep cycles; rate limits and
            // breakdowns are not periods anything is spent in.
            let fact = dimension
                .as_deref()
                .filter(|d| subject.dimensions.iter().any(|s| s.id == d.id))
                .filter(|_| {
                    !matches!(
                        entry.value,
                        QuotaValue::RateLimit(_) | QuotaValue::Breakdown(_)
                    )
                })
                .map(|d| {
                    let scope = match &d.scope {
                        QuotaScope::Unknown => entry.model_scope.clone(),
                        scope => scope.clone(),
                    };
                    facts.push(cycles::Facts::new(
                        &entry.id,
                        d,
                        scope,
                        allowance(&entry.value),
                        write,
                        now_ms,
                    ));
                    facts.len() - 1
                });
            readings.push((entry, reading, exhausted, write, dimension, fact));
        }
        // Cycle bookkeeping never stands in the way of the observation log or
        // of a block: a failure here costs the link, not the reading.
        let links = match (cycles::Cycles {
            store: self.store(),
            cache: self.cache(),
        })
        .observe(&credential.id, &facts, now_ms)
        .await
        {
            Ok(links) => links,
            Err(error) => {
                tracing::warn!(credential_id = %credential.id, %error, "quota cycles were not updated");
                Vec::new()
            }
        };
        let mut rows = Vec::with_capacity(entries.len());
        let mut written = Vec::with_capacity(entries.len());
        let mut blocks = Vec::new();
        for (entry, reading, exhausted, write, dimension, fact) in readings {
            if !write {
                continue;
            }
            let cycle_id = ids::random_id();
            let period = allowance(&entry.value);
            let link = fact
                .and_then(|index| links.get(index))
                .and_then(Option::as_ref);
            rows.push(credential_quota_cycle::ActiveModel {
                id: Set(cycle_id.clone()),
                credential_id: Set(credential.id.clone()),
                scope: Set(serde_json::to_value(&entry.model_scope)
                    .map_err(|e| CoreError::Rewrite(e.to_string()))?),
                snapshot: Set(snapshot_json(entry)),
                observed_at_ms: Set(now_ms),
                starts_at_ms: Set(period.and_then(|a| a.period_start_ms)),
                resets_at_ms: Set(period.and_then(|a| a.period_end_ms)),
                credential_cycle_id: Set(link.map(|l| l.cycle_id.clone())),
                cycle_cost_usd: Set(link.map(|l| l.cost_usd)),
            });
            written.push(reading);
            if let Some(dimension) = dimension.filter(|d| d.tracking == QuotaTracking::Reported)
                && exhausted
            {
                blocks.extend(exhaustion_blocks(&dimension, entry, &cycle_id, now_ms));
            }
        }
        if !rows.is_empty() {
            self.store()
                .credential_quota_cycles()
                .create_many(rows)
                .await?;
            self.record_quota_observations(&credential.id, written, now_ms)
                .await?;
        }
        // Skipped readings still refresh the reset projection: it tracks the
        // latest reading, not the latest row.
        self.update_reset_observations(&credential.id, entries, now_ms)
            .await?;
        for block in &blocks {
            self.record_block(
                &credential.provider_id,
                &credential.id,
                block.clone(),
                now_ms,
            )
            .await?;
        }
        Ok(blocks)
    }

    /// Hand one upstream answer's headers to the channel; returns the blocks
    /// written when the observation reported exhaustion.
    pub(crate) async fn observe_answer_headers(
        &self,
        credential: &CredentialData,
        operation: OperationKey,
        upstream_model: Option<&str>,
        status: http::StatusCode,
        headers: &http::HeaderMap,
        now_ms: i64,
    ) -> CoreResult<Vec<CredentialBlock>> {
        let snapshot = self.snapshot();
        let Some(provider) = snapshot.providers.get(&credential.provider_id) else {
            return Ok(Vec::new());
        };
        let Some(observer) = provider.channel.quota_headers() else {
            return Ok(Vec::new());
        };
        let entries = observer
            .observe(QuotaHeaderContext {
                operation,
                upstream_model: upstream_model.unwrap_or_default(),
                status,
                headers,
            })
            .map_err(CoreError::Channel)?;
        self.observe_quota(credential, &entries, now_ms).await
    }

    /// Charge one request against every Counted request dimension covering
    /// this operation and model, before anything goes out. A window already
    /// at its limit blocks the credential until the window ends; that block
    /// is returned so the attempt moves on without sending.
    pub(crate) async fn charge_request(
        &self,
        credential: &CredentialData,
        operation: Operation,
        upstream_model: Option<&str>,
        now_ms: i64,
    ) -> CoreResult<Option<CredentialBlock>> {
        meter(
            self.store(),
            self.cache(),
            credential,
            operation,
            upstream_model,
            QuotaMetric::Requests,
            Decimal::ONE,
            now_ms,
        )
        .await
    }

    /// Query upstream account observations through the assigned channel and
    /// client, persist them and block on exhaustion. Does not aggregate
    /// subscription pools, change quotas or redeem reset credits. The upper
    /// layer must authorize this account operation.
    pub async fn query_credential_quota(
        &self,
        provider_id: &str,
        credential_id: &str,
    ) -> CoreResult<QuotaSnapshot>
    where
        C: Send,
    {
        let queried_at_ms = now_ms();
        let (credential, mut observed) = self
            .quota_operation(provider_id, credential_id, |provider, context| {
                Box::pin(async move {
                    provider
                        .channel
                        .quota_query()
                        .ok_or(ChannelError::UnsupportedService)?
                        .query(context)
                        .await
                })
            })
            .await?;
        observed.observed_at_ms = now_ms();
        self.observe_quota(&credential, &observed.entries, observed.observed_at_ms)
            .await?;
        self.clear_recovered_quota_blocks(&credential, &observed.entries, queried_at_ms)
            .await?;
        Ok(observed)
    }

    /// Only positive upstream readings can remove an older exhaustion block.
    /// Other dimensions, local limits and blocks observed during the query stay.
    async fn clear_recovered_quota_blocks(
        &self,
        credential: &CredentialData,
        entries: &[QuotaEntry],
        queried_at_ms: i64,
    ) -> CoreResult<()> {
        let data = self.snapshot();
        let model = quota_model(&data, credential);
        let recovered: Vec<_> = entries.iter().filter(|entry| {
            !exhausted(&entry.value) && match &entry.value {
                QuotaValue::Balance(balance) => balance.remaining.is_some_and(|r| r > Decimal::ZERO),
                value => allowance(value).is_some_and(|a| a.unlimited == Some(true)
                    || a.remaining.is_some_and(|r| r > Decimal::ZERO)
                    || a.used_percent.is_some_and(|p| p < Decimal::ONE_HUNDRED)
                    || matches!((a.used, a.limit), (Some(used), Some(limit)) if used < limit)),
            }
        }).filter_map(|entry| {
            let dimension = classify(model, credential, entry)?;
            let scope = match &dimension.scope { QuotaScope::Unknown => &entry.model_scope, scope => scope };
            Some((dimension.id.clone(), scope.clone()))
        }).collect();
        if recovered.is_empty() {
            return Ok(());
        }
        let rows = self
            .store()
            .credential_blocks()
            .query(
                credential_block::Entity::find()
                    .filter(credential_block::Column::CredentialId.eq(&credential.id))
                    .filter(credential_block::Column::ObservedAtMs.lte(queried_at_ms)),
            )
            .await?;
        let ids: Vec<_> = rows
            .into_iter()
            .filter(|row| {
                row.source["kind"] == "quota_exhausted"
                    && recovered.iter().any(|(dimension, scope)| {
                        row.source["dimension"] == *dimension
                            && serde_json::from_value::<QuotaScope>(row.scope.clone())
                                .ok()
                                .as_ref()
                                == Some(scope)
                    })
            })
            .map(|row| row.id)
            .collect();
        if !ids.is_empty() {
            self.store().credential_blocks().delete_many(&ids).await?;
        }
        let key = crate::keys::credential_blocks(&credential.provider_id, &credential.id);
        loop {
            let Some(cached) = self.cache().get(&key).await? else {
                return Ok(());
            };
            let mut blocks: crate::CredentialBlocks = serde_json::from_slice(&cached.value)
                .map_err(|e| CoreError::Rewrite(e.to_string()))?;
            let before = blocks.blocks.len();
            blocks.blocks.retain(|block| !(block.observed_at_ms <= queried_at_ms
                && matches!(&block.source, BlockSource::QuotaExhausted { dimension, .. }
                    if recovered.iter().any(|(id, scope)| id == dimension && *scope == block.scope))));
            if blocks.blocks.len() == before {
                return Ok(());
            }
            let replacement = gproxy_cache::Replacement {
                value: serde_json::to_vec(&blocks)
                    .map_err(|e| CoreError::Rewrite(e.to_string()))?,
                ttl: EMPTY_BLOCKS_CACHE_TTL,
            };
            if self
                .cache()
                .compare_exchange(&key, Some(cached.version), Some(replacement))
                .await?
                != gproxy_cache::CasOutcome::Conflict
            {
                return Ok(());
            }
        }
    }

    /// Read reset cards separately from usage. Never consumes a card.
    pub async fn query_credential_reset_credits(
        &self,
        provider_id: &str,
        credential_id: &str,
    ) -> CoreResult<gproxy_channel::channel::QuotaResetCredits>
    where
        C: Send,
    {
        let (_, credits) = self
            .quota_operation(provider_id, credential_id, |provider, context| {
                Box::pin(async move {
                    provider
                        .channel
                        .quota_reset()
                        .ok_or(ChannelError::UnsupportedService)?
                        .credits(context)
                        .await
                })
            })
            .await?;
        Ok(credits)
    }

    /// Explicitly redeem a reset card. An authentication retry uses the same
    /// redemption id and the credential's assigned client, just like querying.
    pub async fn reset_credential_quota(
        &self,
        provider_id: &str,
        credential_id: &str,
        request: gproxy_channel::channel::QuotaResetRequest<'_>,
    ) -> CoreResult<gproxy_channel::channel::QuotaResetResult>
    where
        C: Send,
    {
        let (credential, result) = self
            .quota_operation(provider_id, credential_id, |provider, context| {
                let redeem_request_id = request.redeem_request_id.to_owned();
                let program = request.program.map(str::to_owned);
                let grant_id = request.grant_id.map(str::to_owned);
                Box::pin(async move {
                    provider
                        .channel
                        .quota_reset()
                        .ok_or(ChannelError::UnsupportedService)?
                        .reset(
                            context,
                            gproxy_channel::channel::QuotaResetRequest {
                                redeem_request_id: &redeem_request_id,
                                program: program.as_deref(),
                                grant_id: grant_id.as_deref(),
                            },
                        )
                        .await
                })
            })
            .await?;
        if result.outcome == gproxy_channel::channel::QuotaResetOutcome::Reset {
            // The upstream reopened the windows: cut their cycles here, at
            // the moment it said so, then ask for the new boundaries. Neither
            // step can undo a redemption that already happened, so neither
            // fails it.
            let cut = cycles::Cycles {
                store: self.store(),
                cache: self.cache(),
            }
            .manual_reset(
                &cycles::CycleSubject::of(&credential),
                &result.clears,
                now_ms(),
            )
            .await;
            if let Err(error) = cut {
                tracing::warn!(credential_id, %error, "quota cycles were not cut after a reset");
            }
            if let Err(error) = self
                .query_credential_quota(provider_id, credential_id)
                .await
            {
                tracing::debug!(credential_id, %error, "no quota reading after a reset");
            }
        }
        Ok(result)
    }

    async fn quota_operation<T>(
        &self,
        provider_id: &str,
        credential_id: &str,
        invoke: impl for<'a> Fn(
            &'a crate::ProviderData,
            CredentialContext<'a>,
        ) -> gproxy_channel::channel::OperationFuture<'a, T>,
    ) -> CoreResult<(Arc<CredentialData>, T)>
    where
        C: Send,
    {
        let snapshot = self.snapshot();
        let credential = snapshot
            .credentials
            .get(credential_id)
            .filter(|c| c.provider_id == provider_id)
            .cloned()
            .ok_or_else(|| {
                CoreError::InvalidTarget(format!(
                    "credential `{credential_id}` is not part of provider `{provider_id}`"
                ))
            })?;
        let provider = snapshot
            .providers
            .get(provider_id)
            .cloned()
            .ok_or_else(|| {
                CoreError::InvalidTarget(format!("provider `{provider_id}` is not loaded"))
            })?;
        let can_refresh = provider.channel.credential_refresh().is_some();
        if can_refresh && crate::refresh::needs_refresh(&credential.state.load(), now_ms()) {
            self.refresh_credential(provider_id, credential_id, crate::RefreshMode::IfNeeded)
                .await?;
        }
        let mut retried = false;
        loop {
            let version = credential.state.load();
            let result = invoke(
                &provider,
                CredentialContext {
                    provider: crate::assemble::provider_view(&provider.entity),
                    credential: crate::execute::prepare::credential_view(&credential, &version),
                    client: credential.client.as_ref(),
                },
            )
            .await;
            // Imported credentials can have no expiry; allow one auth retry.
            if can_refresh
                && !retried
                && matches!(&result, Err(ChannelError::UpstreamResponse { status, .. }) if *status == http::StatusCode::UNAUTHORIZED)
            {
                self.refresh_credential(provider_id, credential_id, crate::RefreshMode::Force)
                    .await?;
                retried = true;
                continue;
            }
            return result
                .map(|value| (credential, value))
                .map_err(CoreError::Channel);
        }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Core<C> {
    /// The post-usage meter for the funnel.
    pub(crate) fn usage_meter(&self) -> Arc<dyn UsageMeter> {
        Arc::new(CountedMeter {
            store: self.store().clone(),
            cache: self.cache().clone(),
        })
    }
}

/// Charges settled usage against Counted token and cost dimensions and
/// settles the priced cost into the request's caller budgets. Runs inside
/// the funnel, so it owns its handles rather than borrowing the engine.
pub(crate) trait UsageMeter: Send + Sync {
    fn charge<'a>(
        &'a self,
        request: &'a RequestContext,
        report: &'a UsageReport,
    ) -> CapabilityFuture<'a, ()>;
}

struct CountedMeter<C> {
    store: Arc<Store<C>>,
    cache: Arc<dyn Cache>,
}

impl<C: BatchConnectionTrait + Send + Sync> UsageMeter for CountedMeter<C> {
    fn charge<'a>(
        &'a self,
        request: &'a RequestContext,
        report: &'a UsageReport,
    ) -> CapabilityFuture<'a, ()> {
        Box::pin(async move {
            let now = now_ms();
            // Budgets settle whatever the request cost, zero included: an
            // unpriced request still records that it happened. Settlement
            // failures only lose accounting; the request is done.
            if !request.budgets.is_empty() {
                let (amount, currency) = match &report.cost {
                    Some(cost) => (cost.amount, cost.currency.as_str()),
                    None => (Decimal::ZERO, "USD"),
                };
                let _ = crate::budget::settle(&self.store, request, amount, currency, now).await;
            }
            for exchange in &report.exchanges {
                let Some(credential) = request
                    .target
                    .credentials
                    .iter()
                    .find(|c| c.id == exchange.credential_id)
                else {
                    continue;
                };
                let tokens = &exchange.usage.tokens;
                let total: u64 = [
                    tokens.input_tokens,
                    tokens.output_tokens,
                    tokens.cached_input_tokens,
                    tokens.cache_creation_5m_tokens,
                    tokens.cache_creation_30m_tokens,
                    tokens.cache_creation_1h_tokens,
                ]
                .into_iter()
                .flatten()
                .fold(0u64, u64::saturating_add);
                // Metering failures only lose counting; the request is done.
                if total > 0 {
                    let _ = meter(
                        &self.store,
                        &self.cache,
                        credential,
                        request.operation.operation,
                        exchange.upstream_model.as_deref(),
                        QuotaMetric::Tokens,
                        Decimal::from(total),
                        now,
                    )
                    .await;
                }
                // Cycles accrue USD only; another currency is not converted.
                let usd = match &exchange.cost {
                    Some(cost) if cost.currency.eq_ignore_ascii_case("USD") => Some(cost.amount),
                    Some(cost) => {
                        tracing::debug!(
                            credential_id = %credential.id,
                            currency = %cost.currency,
                            "a cost in another currency does not accrue to quota cycles"
                        );
                        None
                    }
                    None => None,
                };
                if let Err(error) = (cycles::Cycles {
                    store: &self.store,
                    cache: &self.cache,
                })
                .accrue(
                    &cycles::CycleSubject::of(credential),
                    request.operation.operation,
                    exchange.upstream_model.as_deref(),
                    usd,
                    now,
                )
                .await
                {
                    tracing::warn!(credential_id = %credential.id, %error, "quota cycles missed a charge");
                }
                // Cost dimensions count USD; an unpriced exchange (or one
                // priced in another currency) charges nothing.
                if let Some(cost) = &exchange.cost
                    && cost.currency.eq_ignore_ascii_case("USD")
                {
                    let _ = meter(
                        &self.store,
                        &self.cache,
                        credential,
                        request.operation.operation,
                        exchange.upstream_model.as_deref(),
                        QuotaMetric::Cost,
                        cost.amount,
                        now,
                    )
                    .await;
                }
            }
        })
    }
}

/// Meter `amount` against every Counted dimension of `metric` covering this
/// operation and model. The window rows in Store are the record and the
/// arbiter: a charge lands only while it fits under the limit, atomically per
/// row, so every instance sees the same count and a restart loses nothing.
/// Requests and tokens count as integers, cost in `FixedDecimal` atoms.
/// Returns the block written when a window refused the charge, so the caller
/// can skip the credential; a charge that lands exactly on the limit blocks
/// the window for later calls but still counts as applied. A block for a
/// glob-scoped operator limit names the charged model, since the dimension's
/// scope is wider than the limit.
#[allow(clippy::too_many_arguments)]
async fn meter<C: BatchConnectionTrait>(
    store: &Store<C>,
    cache: &Arc<dyn Cache>,
    credential: &CredentialData,
    operation: Operation,
    upstream_model: Option<&str>,
    metric: QuotaMetric,
    amount: Decimal,
    now_ms: i64,
) -> CoreResult<Option<CredentialBlock>> {
    let Some(amount) = counted_units(&metric, amount).filter(|a| *a > 0) else {
        return Ok(None);
    };
    for dimension in &credential.quota {
        if dimension.tracking != QuotaTracking::Counted
            || dimension.metric != metric
            || !dimension_applies(credential, dimension, operation, upstream_model)
        {
            continue;
        }
        // A Counted dimension without a limit cannot be enforced.
        let Some(limit) = dimension.limit.and_then(|l| counted_units(&metric, l)) else {
            continue;
        };
        let (start, end) = window_bounds(&dimension.window, now_ms);
        let outcome = store
            .counted_windows()
            .charge_many(vec![CountedCharge {
                credential_id: credential.id.clone(),
                dimension: dimension.id.clone(),
                window_start_ms: start,
                window_end_ms: end,
                amount,
                limit,
            }])
            .await?
            .into_iter()
            .next()
            .unwrap_or(CountedOutcome {
                applied: false,
                used: 0,
            });
        if !outcome.applied || outcome.used >= limit {
            let narrowed = credential
                .limits
                .iter()
                .any(|l| l.dimension_id == dimension.id && l.narrows_scope());
            let scope = match upstream_model {
                Some(model) if narrowed => QuotaScope::Models(vec![model.to_owned()]),
                _ => dimension.scope.clone(),
            };
            let block = CredentialBlock {
                scope,
                operation: None,
                until_ms: end,
                source: BlockSource::Counted {
                    dimension: dimension.id.clone(),
                },
                observed_at_ms: now_ms,
            };
            crate::availability::persist_block(
                store,
                cache,
                &credential.provider_id,
                &credential.id,
                block.clone(),
                now_ms,
            )
            .await?;
            if !outcome.applied {
                return Ok(Some(block));
            }
        }
    }
    Ok(None)
}

/// Drop every `Counted` block of `dimension` from the credential's cached
/// blocks with a CAS loop, keeping failure streaks and other blocks. Store
/// rows are the caller's business. A missing entry needs nothing.
pub(crate) async fn clear_counted_blocks(
    cache: &Arc<dyn Cache>,
    provider_id: &str,
    credential_id: &str,
    dimension: &str,
    now_ms: i64,
) -> CoreResult<()> {
    use gproxy_cache::{CasOutcome, Replacement};
    let key = crate::keys::credential_blocks(provider_id, credential_id);
    loop {
        let Some(entry) = cache.get(&key).await? else {
            return Ok(());
        };
        let mut current =
            serde_json::from_slice::<crate::CredentialBlocks>(&entry.value).unwrap_or_default();
        let before = current.blocks.len();
        current.blocks.retain(|block| {
            block.until_ms > now_ms
                && !matches!(&block.source, BlockSource::Counted { dimension: d } if d == dimension)
        });
        if current.blocks.len() == before {
            return Ok(());
        }
        let ttl = current.blocks.iter().map(|b| b.until_ms).max().map_or(
            EMPTY_BLOCKS_CACHE_TTL,
            |until| {
                std::time::Duration::from_millis(u64::try_from(until - now_ms).unwrap_or(1))
                    .min(EMPTY_BLOCKS_CACHE_TTL)
            },
        );
        let value = serde_json::to_vec(&current).map_err(|e| CoreError::Rewrite(e.to_string()))?;
        match cache
            .compare_exchange(&key, Some(entry.version), Some(Replacement { value, ttl }))
            .await?
        {
            CasOutcome::Applied(_) => return Ok(()),
            CasOutcome::Conflict => continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_windows_are_utc_aligned() {
        // 2026-09-19T10:00:00Z
        let now = 1_789_812_000_000;
        let (start, end) = window_bounds(&QuotaWindow::CalendarDay, now);
        assert_eq!(
            datetime(start).to_string(),
            "2026-09-19 0:00:00.0 +00:00:00"
        );
        assert_eq!(end - start, 86_400_000);
        let (start, end) = window_bounds(&QuotaWindow::CalendarWeek, now);
        assert_eq!(datetime(start).weekday(), time::Weekday::Monday);
        assert_eq!(end - start, 7 * 86_400_000);
        let (start, end) = window_bounds(&QuotaWindow::CalendarMonth, now);
        assert_eq!(
            datetime(start).to_string(),
            "2026-09-01 0:00:00.0 +00:00:00"
        );
        assert_eq!(datetime(end).to_string(), "2026-10-01 0:00:00.0 +00:00:00");
        let (start, end) = window_bounds(&QuotaWindow::Rolling { seconds: 3600 }, now);
        assert_eq!(now - start, 0);
        assert_eq!(end - start, 3_600_000);
    }
}
