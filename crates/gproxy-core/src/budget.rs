//! Caller budgets: USD limits per owner over fixed, calendar-month or
//! permanent windows, checked before the first attempt and settled with the
//! priced cost after the exchange. Windows are `quota_windows` rows opened
//! lazily and never deleted; a manual reset closes the open window and opens
//! a fresh one anchored at the reset time.
//!
//! Every applicable budget must pass (AND). Cost is known only after the
//! upstream answered, so a budget can be overrun by at most one request.

use crate::{Core, CoreError, CoreResult, RequestContext, quota};
use gproxy_channel::channel::QuotaWindow;
use gproxy_seaorm::{BatchConnectionTrait, FixedDecimal};
use gproxy_store::{
    Store,
    entity::limits::{quota as quota_entity, quota_window},
    operations::quota::{OpenWindow, Settlement},
};
use regex::Regex;
use rust_decimal::Decimal;
use sea_orm::Set;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Who a request is charged to: one owner of a `quotas` row, `(kind, id)`.
/// Kinds are host-defined strings (`user`, `api_key`, `team`, `org`,
/// ... whatever levels the host's organisation has); core matches them
/// verbatim against `owner_kind` and knows no hierarchy between them. The
/// host lists every owner a request spends for, and every enabled budget of
/// any owner in that chain applies.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BudgetOwner {
    pub kind: String,
    pub id: String,
}

impl BudgetOwner {
    pub fn new(kind: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
        }
    }

    /// Whether `row` belongs to this owner.
    pub fn matches(&self, row: &quota_entity::Model) -> bool {
        row.owner_kind == self.kind && row.owner_id == self.id
    }
}

impl std::fmt::Display for BudgetOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.kind, self.id)
    }
}

/// How a budget's windows are laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetPeriod {
    /// Back-to-back windows of `seconds`, aligned to the quota's anchor
    /// (`anchor_at_ms`, epoch 0 when unset): `5h`, `1d`, `7d`, or a custom
    /// `period_seconds`.
    Fixed { seconds: i64 },
    /// UTC calendar months (`1m`).
    CalendarMonth,
    /// One permanent window (`total`).
    Total,
}

/// One enabled `quotas` row compiled for lookup.
pub struct BudgetData {
    pub quota: Arc<quota_entity::Model>,
    pub owner: BudgetOwner,
    pub period: BudgetPeriod,
    pub limit: Decimal,
    /// `model_pattern` as a `*`/`?` glob over the whole upstream model name.
    model: Option<Regex>,
}

/// One budget's current window, for hosts and consoles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BudgetStatus {
    pub quota_id: String,
    pub owner: BudgetOwner,
    pub window_key: String,
    pub period: String,
    pub model_pattern: Option<String>,
    pub unit: String,
    pub limit: Decimal,
    pub used: Decimal,
    pub window_id: String,
    pub starts_at_ms: i64,
    /// None for a permanent window.
    pub ends_at_ms: Option<i64>,
    /// When the window rolls over; None when it never does.
    pub resets_at_ms: Option<i64>,
}

/// The period named by a `quotas` row, shared with credential limits.
pub(crate) fn period_of(row: &quota_entity::Model) -> Result<BudgetPeriod, String> {
    Ok(match row.period.trim().to_ascii_lowercase().as_str() {
        "5h" => BudgetPeriod::Fixed { seconds: 5 * 3600 },
        "1d" => BudgetPeriod::Fixed { seconds: 86_400 },
        "7d" => BudgetPeriod::Fixed {
            seconds: 7 * 86_400,
        },
        "1m" => BudgetPeriod::CalendarMonth,
        "total" => BudgetPeriod::Total,
        other => match row.period_seconds {
            Some(seconds) if seconds > 0 => BudgetPeriod::Fixed { seconds },
            _ => return Err(format!("unknown period `{other}` without period_seconds")),
        },
    })
}

impl BudgetData {
    /// Compile one row. Errors name the reason the row is unusable; the
    /// assembly logs and skips such rows.
    pub fn compile(row: &quota_entity::Model) -> Result<Self, String> {
        if !row.metric.eq_ignore_ascii_case("cost") {
            return Err(format!("metric `{}` is not `cost`", row.metric));
        }
        if row.owner_kind.trim().is_empty() || row.owner_id.trim().is_empty() {
            return Err("owner_kind and owner_id are required".into());
        }
        let owner = BudgetOwner::new(row.owner_kind.clone(), row.owner_id.clone());
        let period = period_of(row)?;
        let limit = row.limit_value.decimal();
        if limit < Decimal::ZERO {
            return Err("limit_value must be nonnegative".into());
        }
        let model = row
            .model_pattern
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(|p| {
                crate::rewrite::glob_to_regex(p).map_err(|_| format!("invalid model_pattern `{p}`"))
            })
            .transpose()?;
        Ok(Self {
            quota: Arc::new(row.clone()),
            owner,
            period,
            limit,
            model,
        })
    }

    /// Whether this budget covers a request for `model`. A scoped budget
    /// never applies to a request without a model.
    pub fn applies_to(&self, model: Option<&str>) -> bool {
        match &self.model {
            None => true,
            Some(pattern) => model.is_some_and(|m| pattern.is_match(m)),
        }
    }

    /// `[start, end)` of the window containing `now`; `end` is None for a
    /// permanent window.
    pub fn window_bounds(&self, now_ms: i64) -> (i64, Option<i64>) {
        let anchor = self.quota.anchor_at_ms.unwrap_or(0);
        match self.period {
            BudgetPeriod::Fixed { seconds } => {
                let (start, end) = quota::window_bounds(
                    &QuotaWindow::Fixed {
                        seconds,
                        anchor_at_ms: Some(anchor),
                    },
                    now_ms,
                );
                (start, Some(end))
            }
            BudgetPeriod::CalendarMonth => {
                let (start, end) = quota::window_bounds(&QuotaWindow::CalendarMonth, now_ms);
                (start, Some(end))
            }
            BudgetPeriod::Total => (anchor.min(now_ms), None),
        }
    }

    /// The window a manual reset at `now` opens: it starts now and runs to
    /// the next natural boundary (a whole period for fixed windows, the end
    /// of the month for `1m`, forever for `total`).
    fn reset_bounds(&self, now_ms: i64) -> (i64, Option<i64>) {
        match self.period {
            BudgetPeriod::Fixed { seconds } => {
                (now_ms, Some(now_ms.saturating_add(seconds.max(1) * 1000)))
            }
            BudgetPeriod::CalendarMonth => {
                let (_, end) = quota::window_bounds(&QuotaWindow::CalendarMonth, now_ms);
                (now_ms, Some(end))
            }
            BudgetPeriod::Total => (now_ms, None),
        }
    }

    fn snapshot_json(&self) -> serde_json::Value {
        let q = &self.quota;
        serde_json::json!({
            "id": q.id,
            "owner_kind": q.owner_kind,
            "owner_id": q.owner_id,
            "window_key": q.window_key,
            "metric": q.metric,
            "unit": q.unit,
            "limit_value": q.limit_value.to_string(),
            "period": q.period,
            "period_seconds": q.period_seconds,
            "anchor_at_ms": q.anchor_at_ms,
            "model_pattern": q.model_pattern,
            "enabled": q.enabled,
        })
    }
}

/// The enabled budgets of `owners` in `snapshot`, limited to those covering
/// `model`.
pub(crate) fn applicable<'a>(
    budgets: &'a [Arc<BudgetData>],
    owners: &[BudgetOwner],
    model: Option<&str>,
) -> Vec<&'a Arc<BudgetData>> {
    if owners.is_empty() {
        return Vec::new();
    }
    budgets
        .iter()
        .filter(|b| owners.contains(&b.owner) && b.applies_to(model))
        .collect()
}

/// The current window of every budget, opened when missing. The open row
/// is looked up first so a window opened by a reset (which may not match
/// the anchor of a snapshot assembled before it) is honoured.
pub(crate) async fn current_windows<C: BatchConnectionTrait>(
    store: &Store<C>,
    budgets: &[&Arc<BudgetData>],
    now_ms: i64,
) -> CoreResult<Vec<quota_window::Model>> {
    if budgets.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = budgets.iter().map(|b| b.quota.id.clone()).collect();
    let open = store.quota_windows().open_at(&ids, now_ms).await?;
    let mut out: Vec<Option<quota_window::Model>> = budgets
        .iter()
        .map(|b| {
            open.iter()
                .filter(|w| w.quota_id == b.quota.id)
                .max_by_key(|w| w.starts_at_ms)
                .cloned()
        })
        .collect();
    let missing: Vec<usize> = (0..budgets.len()).filter(|i| out[*i].is_none()).collect();
    if !missing.is_empty() {
        let opened = store
            .quota_windows()
            .open_many(
                missing
                    .iter()
                    .map(|&i| {
                        let budget = budgets[i];
                        let (starts_at_ms, ends_at_ms) = budget.window_bounds(now_ms);
                        OpenWindow {
                            quota_id: budget.quota.id.clone(),
                            starts_at_ms,
                            ends_at_ms,
                            quota_snapshot: budget.snapshot_json(),
                        }
                    })
                    .collect(),
            )
            .await?;
        for (i, window) in missing.into_iter().zip(opened) {
            out[i] = Some(window);
        }
    }
    Ok(out.into_iter().map(|w| w.expect("window opened")).collect())
}

/// Settle `amount` of `currency` into the current window of every budget of
/// the request, idempotently by request id. Budgets in another unit than the
/// cost's currency are left alone.
pub(crate) async fn settle<C: BatchConnectionTrait>(
    store: &Store<C>,
    request: &RequestContext,
    amount: Decimal,
    currency: &str,
    now_ms: i64,
) -> CoreResult<()> {
    let budgets: Vec<&Arc<BudgetData>> = applicable(
        &request.snapshot.budgets,
        &request.budgets,
        request.target.upstream_model.as_deref(),
    )
    .into_iter()
    .filter(|b| b.quota.unit.eq_ignore_ascii_case(currency))
    .collect();
    if budgets.is_empty() {
        return Ok(());
    }
    let amount = FixedDecimal::rounded(amount.max(Decimal::ZERO))
        .map_err(|e| CoreError::Store(gproxy_store::StoreError::Amount(e)))?;
    let windows = current_windows(store, &budgets, now_ms).await?;
    store
        .quota_settlements()
        .settle_many(
            windows
                .into_iter()
                .map(|w| Settlement {
                    window_id: w.id,
                    request_id: request.request_id.clone(),
                    amount,
                    settled_at_ms: now_ms,
                })
                .collect(),
        )
        .await?;
    Ok(())
}

fn status_of(budget: &BudgetData, window: quota_window::Model) -> BudgetStatus {
    BudgetStatus {
        quota_id: budget.quota.id.clone(),
        owner: budget.owner.clone(),
        window_key: budget.quota.window_key.clone(),
        period: budget.quota.period.clone(),
        model_pattern: budget.quota.model_pattern.clone(),
        unit: budget.quota.unit.clone(),
        limit: budget.limit,
        used: window.used.decimal(),
        window_id: window.id,
        starts_at_ms: window.starts_at_ms,
        ends_at_ms: window.ends_at_ms,
        resets_at_ms: window.ends_at_ms,
    }
}

impl<C: BatchConnectionTrait> Core<C> {
    /// Every applicable budget must have room before the first attempt. The
    /// first exhausted one is the error; nothing was sent or reserved.
    pub(crate) async fn check_budgets(
        &self,
        request: &RequestContext,
        now_ms: i64,
    ) -> CoreResult<()> {
        let budgets = applicable(
            &request.snapshot.budgets,
            &request.budgets,
            request.target.upstream_model.as_deref(),
        );
        if budgets.is_empty() {
            return Ok(());
        }
        let windows = current_windows(self.store(), &budgets, now_ms).await?;
        for (budget, window) in budgets.iter().zip(windows) {
            if window.used.decimal() >= budget.limit {
                return Err(CoreError::BudgetExhausted {
                    quota_id: budget.quota.id.clone(),
                    window_key: budget.quota.window_key.clone(),
                    resets_at_ms: window.ends_at_ms,
                });
            }
        }
        Ok(())
    }

    /// The current window of every enabled budget of `owners`, whatever
    /// model it is scoped to, opened when missing. Ordered by quota id.
    pub async fn budget_status(
        &self,
        owners: &[BudgetOwner],
        now_ms: i64,
    ) -> CoreResult<Vec<BudgetStatus>> {
        let snapshot = self.snapshot();
        let mut budgets: Vec<&Arc<BudgetData>> = snapshot
            .budgets
            .iter()
            .filter(|b| owners.contains(&b.owner))
            .collect();
        budgets.sort_by(|a, b| a.quota.id.cmp(&b.quota.id));
        let windows = current_windows(self.store(), &budgets, now_ms).await?;
        Ok(budgets
            .into_iter()
            .zip(windows)
            .map(|(budget, window)| status_of(budget, window))
            .collect())
    }

    /// Close the budget's open window at `now` and open a new one starting
    /// there. The quota's `anchor_at_ms` becomes `now` so later windows align
    /// to it; publish a reloaded snapshot for the new anchor to take effect
    /// once the reset window itself has ended. History is kept.
    pub async fn reset_budget(&self, quota_id: &str, now_ms: i64) -> CoreResult<BudgetStatus> {
        let snapshot = self.snapshot();
        let budget = snapshot
            .budgets
            .iter()
            .find(|b| b.quota.id == quota_id)
            .ok_or_else(|| {
                CoreError::InvalidTarget(format!("budget `{quota_id}` is not an enabled budget"))
            })?;
        let store = self.store();
        let open: Vec<String> = store
            .quota_windows()
            .open_at(std::slice::from_ref(&budget.quota.id), now_ms)
            .await?
            .into_iter()
            .map(|w| w.id)
            .collect();
        store.quota_windows().close_many(&open, now_ms).await?;
        store
            .quotas()
            .update_many(vec![quota_entity::ActiveModel {
                id: Set(budget.quota.id.clone()),
                anchor_at_ms: Set(Some(now_ms)),
                ..Default::default()
            }])
            .await?;
        let (starts_at_ms, ends_at_ms) = budget.reset_bounds(now_ms);
        let mut snapshot_json = budget.snapshot_json();
        snapshot_json["anchor_at_ms"] = serde_json::json!(now_ms);
        let window = store
            .quota_windows()
            .open_many(vec![OpenWindow {
                quota_id: budget.quota.id.clone(),
                starts_at_ms,
                ends_at_ms,
                quota_snapshot: snapshot_json,
            }])
            .await?
            .pop()
            .ok_or(gproxy_store::StoreError::UnexpectedResult)?;
        Ok(status_of(budget, window))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(period: &str, anchor: Option<i64>) -> quota_entity::Model {
        quota_entity::Model {
            id: "q".into(),
            owner_kind: "user".into(),
            owner_id: "u".into(),
            window_key: "primary".into(),
            metric: "cost".into(),
            unit: "USD".into(),
            limit_value: "5".parse().unwrap(),
            period: period.into(),
            period_seconds: None,
            anchor_at_ms: anchor,
            model_pattern: Some("gpt-*".into()),
            enabled: true,
        }
    }

    #[test]
    fn periods_and_bounds() {
        // 2026-09-19T10:00:00Z
        let now = 1_789_812_000_000;
        let five_h = BudgetData::compile(&row("5h", Some(1_000))).unwrap();
        assert_eq!(five_h.period, BudgetPeriod::Fixed { seconds: 18_000 });
        let (start, end) = five_h.window_bounds(now);
        assert_eq!((now - start) % 18_000_000, (now - 1_000) % 18_000_000);
        assert_eq!(end, Some(start + 18_000_000));
        let month = BudgetData::compile(&row("1m", None)).unwrap();
        let (start, end) = month.window_bounds(now);
        assert_eq!(start, 1_788_220_800_000, "2026-09-01T00:00:00Z");
        assert_eq!(end, Some(1_790_812_800_000), "2026-10-01T00:00:00Z");
        let total = BudgetData::compile(&row("total", Some(7))).unwrap();
        assert_eq!(total.window_bounds(now), (7, None));
        assert_eq!(total.reset_bounds(now), (now, None));
        assert!(five_h.applies_to(Some("gpt-5")));
        assert!(!five_h.applies_to(Some("claude-3")));
        assert!(!five_h.applies_to(None));
        let custom = BudgetData::compile(&quota_entity::Model {
            period_seconds: Some(60),
            ..row("custom", None)
        })
        .unwrap();
        assert_eq!(custom.period, BudgetPeriod::Fixed { seconds: 60 });
    }

    #[test]
    fn invalid_rows_name_their_reason() {
        assert!(BudgetData::compile(&row("2w", None)).is_err());
        assert!(
            BudgetData::compile(&quota_entity::Model {
                metric: "tokens".into(),
                ..row("1d", None)
            })
            .is_err()
        );
        assert!(
            BudgetData::compile(&quota_entity::Model {
                owner_kind: " ".into(),
                ..row("1d", None)
            })
            .is_err()
        );
        let compiled = BudgetData::compile(&row("1d", None)).unwrap();
        assert_eq!(compiled.owner, BudgetOwner::new("user", "u"));
        assert_eq!(compiled.owner.to_string(), "user:u");
        assert!(compiled.owner.matches(&row("1d", None)));
        assert!(!BudgetOwner::new("team", "u").matches(&row("1d", None)));
        assert!(
            BudgetData::compile(&quota_entity::Model {
                model_pattern: Some("".into()),
                ..row("1d", None)
            })
            .unwrap()
            .applies_to(None),
            "blank pattern means every model"
        );
    }
}
