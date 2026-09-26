//! Operator limits on upstream credentials: `quotas` rows owned by a
//! `credential` or a `provider` become synthetic Counted quota dimensions of
//! the credentials they cover, next to the channel's own. Requests are
//! charged before the exchange and USD cost after settlement through the
//! same `counted_windows` machinery, so an exhausted limit blocks the
//! credential until its window ends. These are upstream-side limits, not
//! caller budgets: they never see `RequestContext::budgets` and are never
//! settled into `quota_windows`.

use crate::{
    Core, CoreError, CoreResult,
    budget::{BudgetOwner, BudgetPeriod, period_of},
    quota::window_bounds,
};
use gproxy_channel::channel::{
    QuotaDimension, QuotaMetric, QuotaScope, QuotaTracking, QuotaWindow,
};
use gproxy_seaorm::{BatchConnectionTrait, FixedDecimal};
use gproxy_store::entity::limits::{counted_window, credential_block, quota as quota_entity};
use regex::Regex;
use rust_decimal::Decimal;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Set};
use std::sync::Arc;

/// Owner kind of a limit on one credential (`owner_id` = credential id).
pub const OWNER_CREDENTIAL: &str = "credential";
/// Owner kind of a limit on every credential of a provider (`owner_id` =
/// provider id); a credential-level row with the same `window_key` replaces
/// it for that credential.
pub const OWNER_PROVIDER: &str = "provider";
/// Synthetic dimension ids are `limit:{quota_id}`.
pub const DIMENSION_PREFIX: &str = "limit:";

/// How a limit's `model_pattern` selects models.
enum ModelFilter {
    /// Blank pattern: every model.
    All,
    /// A pattern without `*`/`?`: one exact model, expressed as the
    /// dimension's scope so blocks carry it too.
    Exact(String),
    /// A glob, checked at charge time; a block then names the charged model.
    Glob(Regex),
}

/// One enabled credential/provider `quotas` row compiled for assembly.
pub struct CredentialLimit {
    pub quota: Arc<quota_entity::Model>,
    pub owner: BudgetOwner,
    /// `Requests` (unit `count`) or `Cost` (unit `USD`).
    pub metric: QuotaMetric,
    pub period: BudgetPeriod,
    pub limit: Decimal,
    /// `limit:{quota_id}`, the id of the synthetic dimension.
    pub dimension_id: String,
    model: ModelFilter,
}

/// One limit's current window on one credential, for hosts and consoles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialLimitStatus {
    pub quota_id: String,
    pub owner: BudgetOwner,
    pub window_key: String,
    /// `requests` or `cost`.
    pub metric: String,
    pub period: String,
    pub model_pattern: Option<String>,
    /// `count` or `USD`.
    pub unit: String,
    pub limit: Decimal,
    pub used: Decimal,
    pub window_start_ms: i64,
    /// None for a `total` window.
    pub window_end_ms: Option<i64>,
}

impl CredentialLimit {
    /// Whether `row` is an operator limit rather than a caller budget.
    pub fn is_limit_row(row: &quota_entity::Model) -> bool {
        matches!(row.owner_kind.as_str(), OWNER_CREDENTIAL | OWNER_PROVIDER)
    }

    /// Compile one row. Errors name the reason the row is unusable; the
    /// assembly logs and skips such rows.
    pub fn compile(row: &quota_entity::Model) -> Result<Self, String> {
        if !Self::is_limit_row(row) {
            return Err(format!(
                "owner_kind `{}` is neither `{OWNER_CREDENTIAL}` nor `{OWNER_PROVIDER}`",
                row.owner_kind
            ));
        }
        if row.owner_id.trim().is_empty() {
            return Err("owner_id is required".into());
        }
        let metric = row.metric.trim().to_ascii_lowercase();
        let (metric, unit) = match metric.as_str() {
            "requests" => (QuotaMetric::Requests, "count"),
            "cost" => (QuotaMetric::Cost, "USD"),
            other => return Err(format!("metric `{other}` is neither `requests` nor `cost`")),
        };
        if !row.unit.trim().eq_ignore_ascii_case(unit) {
            return Err(format!(
                "unit `{}` does not fit metric `{}` (expected `{unit}`)",
                row.unit, row.metric
            ));
        }
        let period = period_of(row)?;
        let limit = row.limit_value.decimal();
        if limit < Decimal::ZERO {
            return Err("limit_value must be nonnegative".into());
        }
        let model = match row.model_pattern.as_deref().map(str::trim) {
            None | Some("") => ModelFilter::All,
            Some(pattern) if pattern.contains(['*', '?']) => ModelFilter::Glob(
                crate::rewrite::glob_to_regex(pattern)
                    .map_err(|_| format!("invalid model_pattern `{pattern}`"))?,
            ),
            Some(exact) => ModelFilter::Exact(exact.to_owned()),
        };
        Ok(Self {
            quota: Arc::new(row.clone()),
            owner: BudgetOwner::new(row.owner_kind.clone(), row.owner_id.clone()),
            metric,
            period,
            limit,
            dimension_id: format!("{DIMENSION_PREFIX}{}", row.id),
            model,
        })
    }

    /// Whether this limit covers a request for `model`. A scoped limit never
    /// applies to a request without a model.
    pub fn applies_to(&self, model: Option<&str>) -> bool {
        match &self.model {
            ModelFilter::All => true,
            ModelFilter::Exact(exact) => model == Some(exact.as_str()),
            ModelFilter::Glob(pattern) => model.is_some_and(|m| pattern.is_match(m)),
        }
    }

    /// Whether the limit is narrower than its dimension's scope, so a block
    /// derived from it must name the charged model instead.
    pub fn narrows_scope(&self) -> bool {
        matches!(self.model, ModelFilter::Glob(_))
    }

    /// The window layout: fixed periods align to `anchor_at_ms` (epoch 0
    /// when unset), `1m` to UTC months, `total` never resets.
    pub fn window(&self) -> QuotaWindow {
        match self.period {
            BudgetPeriod::Fixed { seconds } => QuotaWindow::Fixed {
                seconds,
                anchor_at_ms: Some(self.quota.anchor_at_ms.unwrap_or(0)),
            },
            BudgetPeriod::CalendarMonth => QuotaWindow::CalendarMonth,
            BudgetPeriod::Total => QuotaWindow::Total,
        }
    }

    /// The synthetic Counted dimension appended to a covered credential.
    pub fn dimension(&self) -> QuotaDimension {
        QuotaDimension {
            id: self.dimension_id.clone(),
            label: Some(self.quota.window_key.clone()),
            scope: match &self.model {
                ModelFilter::Exact(exact) => QuotaScope::Models(vec![exact.clone()]),
                ModelFilter::All | ModelFilter::Glob(_) => QuotaScope::All,
            },
            operations: None,
            metric: self.metric.clone(),
            window: self.window(),
            limit: Some(self.limit),
            tracking: QuotaTracking::Counted,
            blocking: true,
        }
    }

    /// `[start, end)` of the window containing `now`; `end` is None for a
    /// permanent window, whose start is the anchor (the last reset).
    pub fn window_bounds(&self, now_ms: i64) -> (i64, Option<i64>) {
        match self.period {
            BudgetPeriod::Total => (self.quota.anchor_at_ms.unwrap_or(0).min(now_ms), None),
            _ => {
                let (start, end) = window_bounds(&self.window(), now_ms);
                (start, Some(end))
            }
        }
    }

    fn unit(&self) -> &'static str {
        match self.metric {
            QuotaMetric::Cost => "USD",
            _ => "count",
        }
    }

    /// A counted `used` value back in the limit's unit.
    fn unscale(&self, used: i64) -> Decimal {
        match self.metric {
            QuotaMetric::Cost => FixedDecimal::from_atoms(used).decimal(),
            _ => Decimal::from(used),
        }
    }
}

/// Convert a `metric` value to the integer the window rows count in:
/// requests and tokens as they are, cost in `FixedDecimal` atoms. None when
/// the value does not fit.
pub(crate) fn counted_units(metric: &QuotaMetric, value: Decimal) -> Option<i64> {
    use rust_decimal::prelude::ToPrimitive;
    match metric {
        QuotaMetric::Cost => FixedDecimal::rounded(value).ok().map(FixedDecimal::atoms),
        _ => value.to_i64(),
    }
}

/// The limits covering one credential: its own rows, plus the provider's
/// rows whose `window_key` no credential row overrides. Ordered by quota id.
pub(crate) fn limits_for(
    limits: &[Arc<CredentialLimit>],
    provider_id: &str,
    credential_id: &str,
) -> Vec<Arc<CredentialLimit>> {
    let own: Vec<&Arc<CredentialLimit>> = limits
        .iter()
        .filter(|l| l.owner.kind == OWNER_CREDENTIAL && l.owner.id == credential_id)
        .collect();
    let mut out: Vec<Arc<CredentialLimit>> = limits
        .iter()
        .filter(|l| l.owner.kind == OWNER_PROVIDER && l.owner.id == provider_id)
        .filter(|l| !own.iter().any(|o| o.quota.window_key == l.quota.window_key))
        .chain(own.iter().copied())
        .cloned()
        .collect();
    out.sort_by(|a, b| a.quota.id.cmp(&b.quota.id));
    out
}

impl<C: BatchConnectionTrait> Core<C> {
    /// Every limit covering `credential_id` with its current window's usage.
    /// Ordered by quota id. Windows are only read: a limit nothing charged
    /// yet reports zero without opening a row.
    pub async fn credential_limit_status(
        &self,
        credential_id: &str,
        now_ms: i64,
    ) -> CoreResult<Vec<CredentialLimitStatus>> {
        let snapshot = self.snapshot();
        let credential = snapshot.credentials.get(credential_id).ok_or_else(|| {
            CoreError::InvalidTarget(format!("credential `{credential_id}` is not loaded"))
        })?;
        if credential.limits.is_empty() {
            return Ok(Vec::new());
        }
        let rows = self
            .store()
            .counted_windows()
            .query(
                counted_window::Entity::find()
                    .filter(counted_window::Column::CredentialId.eq(credential_id)),
            )
            .await?;
        Ok(credential
            .limits
            .iter()
            .map(|limit| {
                // The row key is the window the meter computes, which for a
                // permanent window starts at 0 whatever the anchor.
                let (row_start, _) = window_bounds(&limit.window(), now_ms);
                let used = rows
                    .iter()
                    .find(|r| r.dimension == limit.dimension_id && r.window_start_ms == row_start)
                    .map_or(0, |r| r.used);
                let (window_start_ms, window_end_ms) = limit.window_bounds(now_ms);
                CredentialLimitStatus {
                    quota_id: limit.quota.id.clone(),
                    owner: limit.owner.clone(),
                    window_key: limit.quota.window_key.clone(),
                    metric: limit.quota.metric.trim().to_ascii_lowercase(),
                    period: limit.quota.period.clone(),
                    model_pattern: limit.quota.model_pattern.clone(),
                    unit: limit.unit().to_owned(),
                    limit: limit.limit,
                    used: limit.unscale(used),
                    window_start_ms,
                    window_end_ms,
                }
            })
            .collect())
    }

    /// Start the limit over at `now` on every credential it covers: the
    /// quota's `anchor_at_ms` becomes `now`, the current `counted_windows`
    /// rows of the dimension are deleted and its Counted blocks are cleared
    /// from Store and cache, so the credentials are selectable again at
    /// once. The new anchor aligns later fixed windows once a reloaded
    /// snapshot is published; until then the count restarts inside the
    /// current window. `total` gets a fresh permanent window the same way.
    /// Returns the ids of the credentials that were reset.
    pub async fn reset_credential_limit(
        &self,
        quota_id: &str,
        now_ms: i64,
    ) -> CoreResult<Vec<String>> {
        let snapshot = self.snapshot();
        let limit = snapshot
            .credential_limits
            .iter()
            .find(|l| l.quota.id == quota_id)
            .ok_or_else(|| {
                CoreError::InvalidTarget(format!(
                    "quota `{quota_id}` is not an enabled credential limit"
                ))
            })?;
        let mut affected: Vec<&Arc<crate::CredentialData>> = snapshot
            .credentials
            .values()
            .filter(|c| c.limits.iter().any(|l| l.quota.id == quota_id))
            .collect();
        affected.sort_by(|a, b| a.id.cmp(&b.id));
        let store = self.store();
        store
            .quotas()
            .update_many(vec![quota_entity::ActiveModel {
                id: Set(limit.quota.id.clone()),
                anchor_at_ms: Set(Some(now_ms)),
                ..Default::default()
            }])
            .await?;
        store
            .counted_windows()
            .delete_where_many(vec![
                counted_window::Entity::delete_many()
                    .filter(counted_window::Column::Dimension.eq(limit.dimension_id.clone())),
            ])
            .await?;
        let ids: Vec<String> = affected.iter().map(|c| c.id.clone()).collect();
        if ids.is_empty() {
            return Ok(ids);
        }
        let doomed: Vec<String> = store
            .credential_blocks()
            .query(
                credential_block::Entity::find()
                    .filter(credential_block::Column::CredentialId.is_in(ids.clone())),
            )
            .await?
            .into_iter()
            .filter(|row| {
                row.source["kind"] == "counted" && row.source["dimension"] == limit.dimension_id
            })
            .map(|row| row.id)
            .collect();
        if !doomed.is_empty() {
            store.credential_blocks().delete_many(&doomed).await?;
        }
        for credential in &affected {
            crate::quota::clear_counted_blocks(
                self.cache(),
                &credential.provider_id,
                &credential.id,
                &limit.dimension_id,
                now_ms,
            )
            .await?;
        }
        self.reload_credentials(&ids).await?;
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(owner_kind: &str, metric: &str, unit: &str, period: &str) -> quota_entity::Model {
        quota_entity::Model {
            id: "q".into(),
            owner_kind: owner_kind.into(),
            owner_id: "x".into(),
            window_key: "w".into(),
            metric: metric.into(),
            unit: unit.into(),
            limit_value: "5".parse().unwrap(),
            period: period.into(),
            period_seconds: None,
            anchor_at_ms: Some(1_000),
            model_pattern: None,
            enabled: true,
        }
    }

    #[test]
    fn rows_compile_into_counted_dimensions() {
        let requests =
            CredentialLimit::compile(&row("provider", "requests", "count", "5h")).unwrap();
        let dimension = requests.dimension();
        assert_eq!(dimension.id, "limit:q");
        assert_eq!(dimension.tracking, QuotaTracking::Counted);
        assert_eq!(dimension.metric, QuotaMetric::Requests);
        assert_eq!(
            dimension.window,
            QuotaWindow::Fixed {
                seconds: 18_000,
                anchor_at_ms: Some(1_000)
            }
        );
        assert_eq!(dimension.limit, Some(Decimal::from(5)));
        let cost = CredentialLimit::compile(&row("credential", "Cost", "usd", "total")).unwrap();
        assert_eq!(cost.dimension().window, QuotaWindow::Total);
        assert_eq!(cost.window_bounds(5_000), (1_000, None));
        assert_eq!(
            counted_units(&cost.metric, "0.0001".parse().unwrap()),
            Some(100_000)
        );
        assert_eq!(cost.unscale(100_000), "0.0001".parse().unwrap());
        assert_eq!(
            CredentialLimit::compile(&row("credential", "requests", "count", "1m"))
                .unwrap()
                .dimension()
                .window,
            QuotaWindow::CalendarMonth
        );
    }

    #[test]
    fn invalid_rows_name_their_reason() {
        assert!(CredentialLimit::compile(&row("user", "requests", "count", "5h")).is_err());
        assert!(CredentialLimit::compile(&row("credential", "tokens", "count", "5h")).is_err());
        assert!(CredentialLimit::compile(&row("credential", "requests", "USD", "5h")).is_err());
        assert!(CredentialLimit::compile(&row("credential", "cost", "count", "5h")).is_err());
        assert!(CredentialLimit::compile(&row("credential", "cost", "USD", "2w")).is_err());
        assert!(
            CredentialLimit::compile(&quota_entity::Model {
                owner_id: " ".into(),
                ..row("credential", "cost", "USD", "5h")
            })
            .is_err()
        );
        assert!(!CredentialLimit::is_limit_row(&row(
            "user", "cost", "USD", "5h"
        )));
    }

    #[test]
    fn model_patterns_scope_or_filter() {
        let glob = CredentialLimit::compile(&quota_entity::Model {
            model_pattern: Some("claude-*".into()),
            ..row("credential", "requests", "count", "1d")
        })
        .unwrap();
        assert!(glob.narrows_scope());
        assert_eq!(glob.dimension().scope, QuotaScope::All);
        assert!(glob.applies_to(Some("claude-3")));
        assert!(!glob.applies_to(Some("gpt-x")));
        assert!(!glob.applies_to(None));
        let exact = CredentialLimit::compile(&quota_entity::Model {
            model_pattern: Some("gpt-x".into()),
            ..row("credential", "requests", "count", "1d")
        })
        .unwrap();
        assert!(!exact.narrows_scope());
        assert_eq!(
            exact.dimension().scope,
            QuotaScope::Models(vec!["gpt-x".into()])
        );
        assert!(exact.applies_to(Some("gpt-x")) && !exact.applies_to(Some("gpt-x-mini")));
        let all = CredentialLimit::compile(&quota_entity::Model {
            model_pattern: Some(" ".into()),
            ..row("credential", "requests", "count", "1d")
        })
        .unwrap();
        assert!(all.applies_to(None));
    }

    #[test]
    fn credential_rows_override_provider_rows_by_window_key() {
        let mk = |id: &str, kind: &str, owner: &str, key: &str| {
            Arc::new(
                CredentialLimit::compile(&quota_entity::Model {
                    id: id.into(),
                    owner_id: owner.into(),
                    window_key: key.into(),
                    ..row(kind, "requests", "count", "1d")
                })
                .unwrap(),
            )
        };
        let all = vec![
            mk("p1", "provider", "p", "w"),
            mk("p2", "provider", "p", "other"),
            mk("c1", "credential", "a", "w"),
            mk("x", "provider", "q", "w"),
        ];
        let ids = |cred: &str| {
            limits_for(&all, "p", cred)
                .iter()
                .map(|l| l.quota.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("a"), ["c1", "p2"]);
        assert_eq!(ids("b"), ["p1", "p2"]);
    }
}
