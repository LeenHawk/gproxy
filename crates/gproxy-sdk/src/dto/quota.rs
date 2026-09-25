//! Budgets and limits: the `quotas` rows a host configures, the windows they
//! are currently in, and what an upstream says about the account behind a
//! credential.
//!
//! Decimal amounts are strings. A budget is money and a limit is a count of
//! requests; neither survives a JavaScript `Number` in every magnitude.
//! They are normalized first: a fixed-scale `100` reads as `100`, not as
//! `100.000000000`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::double_option;
use gproxy_channel::channel::{
    QuotaAllowance, QuotaBalance, QuotaBreakdownRow, QuotaEntry, QuotaResetBehavior,
    QuotaResetCredits, QuotaResetOption, QuotaResetOutcome, QuotaResetResult, QuotaSnapshot,
    QuotaSubject, QuotaValue,
};
use gproxy_store::entity::limits::{
    counted_window, credential_block, credential_quota_cycle, quota, quota_settlement,
};

/// One configured budget or operator limit. `owner_kind` is a free string:
/// `user`, `api_key`, `team`, `org` and `pool` are caller budgets the host
/// names per request, while `credential` and `provider` are operator limits on
/// upstream accounts. Both live in one table because both are windows over a
/// metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct QuotaDto {
    pub id: String,
    pub owner_kind: String,
    pub owner_id: String,
    /// Stable key within the owner, e.g. `primary`.
    pub window_key: String,
    /// `cost` for a budget; `requests` or `cost` for a limit.
    pub metric: String,
    /// `USD` for cost, `count` for requests.
    pub unit: String,
    pub limit_value: String,
    /// `5h`, `1d`, `7d`, `1m`, `total`, or anything with `period_seconds`.
    pub period: String,
    pub period_seconds: Option<i64>,
    pub anchor_at_ms: Option<i64>,
    /// A `*`/`?` glob over the upstream model name; absent covers everything.
    pub model_pattern: Option<String>,
    pub enabled: bool,
}

impl From<quota::Model> for QuotaDto {
    fn from(row: quota::Model) -> Self {
        Self {
            id: row.id,
            owner_kind: row.owner_kind,
            owner_id: row.owner_id,
            window_key: row.window_key,
            metric: row.metric,
            unit: row.unit,
            limit_value: row.limit_value.to_string(),
            period: row.period,
            period_seconds: row.period_seconds,
            anchor_at_ms: row.anchor_at_ms,
            model_pattern: row.model_pattern,
            enabled: row.enabled,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct QuotaWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub owner_kind: String,
    pub owner_id: String,
    #[serde(default)]
    pub window_key: Option<String>,
    pub metric: String,
    pub unit: String,
    pub limit_value: String,
    pub period: String,
    #[serde(default)]
    pub period_seconds: Option<i64>,
    #[serde(default)]
    pub anchor_at_ms: Option<i64>,
    #[serde(default)]
    pub model_pattern: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct QuotaPatch {
    #[serde(default)]
    pub owner_kind: Option<String>,
    #[serde(default)]
    pub owner_id: Option<String>,
    #[serde(default)]
    pub window_key: Option<String>,
    #[serde(default)]
    pub metric: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub limit_value: Option<String>,
    #[serde(default)]
    pub period: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub period_seconds: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub anchor_at_ms: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub model_pattern: Option<Option<String>>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// A caller budget's current window.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct BudgetStatusDto {
    pub quota_id: String,
    pub owner_kind: String,
    pub owner_id: String,
    pub window_key: String,
    pub period: String,
    pub model_pattern: Option<String>,
    pub unit: String,
    pub limit: String,
    pub used: String,
    pub window_id: String,
    pub starts_at_ms: i64,
    pub ends_at_ms: Option<i64>,
    pub resets_at_ms: Option<i64>,
}

impl From<gproxy_core::BudgetStatus> for BudgetStatusDto {
    fn from(status: gproxy_core::BudgetStatus) -> Self {
        Self {
            quota_id: status.quota_id,
            owner_kind: status.owner.kind,
            owner_id: status.owner.id,
            window_key: status.window_key,
            period: status.period,
            model_pattern: status.model_pattern,
            unit: status.unit,
            limit: status.limit.normalize().to_string(),
            used: status.used.normalize().to_string(),
            window_id: status.window_id,
            starts_at_ms: status.starts_at_ms,
            ends_at_ms: status.ends_at_ms,
            resets_at_ms: status.resets_at_ms,
        }
    }
}

/// An operator limit's current window on one credential.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct CredentialLimitStatusDto {
    pub quota_id: String,
    pub owner_kind: String,
    pub owner_id: String,
    pub window_key: String,
    pub metric: String,
    pub period: String,
    pub model_pattern: Option<String>,
    pub unit: String,
    pub limit: String,
    pub used: String,
    pub window_start_ms: i64,
    pub window_end_ms: Option<i64>,
}

impl From<gproxy_core::CredentialLimitStatus> for CredentialLimitStatusDto {
    fn from(status: gproxy_core::CredentialLimitStatus) -> Self {
        Self {
            quota_id: status.quota_id,
            owner_kind: status.owner.kind,
            owner_id: status.owner.id,
            window_key: status.window_key,
            metric: status.metric,
            period: status.period,
            model_pattern: status.model_pattern,
            unit: status.unit,
            limit: status.limit.normalize().to_string(),
            used: status.used.normalize().to_string(),
            window_start_ms: status.window_start_ms,
            window_end_ms: status.window_end_ms,
        }
    }
}

/// What this instance knows about one credential's upstream quota: the cycles
/// observed so far and the blocks currently keeping it out of selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct CredentialQuotaDto {
    pub cycles: Vec<CredentialCycleDto>,
    /// Only blocks that have not expired yet.
    pub blocks: Vec<CredentialBlockDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct CredentialCycleDto {
    pub id: String,
    pub credential_id: String,
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub scope: Value,
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub snapshot: Value,
    pub observed_at_ms: i64,
    pub starts_at_ms: Option<i64>,
    pub resets_at_ms: Option<i64>,
}

impl From<credential_quota_cycle::Model> for CredentialCycleDto {
    fn from(row: credential_quota_cycle::Model) -> Self {
        Self {
            id: row.id,
            credential_id: row.credential_id,
            scope: row.scope,
            snapshot: row.snapshot,
            observed_at_ms: row.observed_at_ms,
            starts_at_ms: row.starts_at_ms,
            resets_at_ms: row.resets_at_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct CredentialBlockDto {
    pub id: String,
    pub credential_id: String,
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub scope: Value,
    pub operation: Option<String>,
    pub until_ms: i64,
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub source: Value,
    pub observed_at_ms: i64,
}

impl From<credential_block::Model> for CredentialBlockDto {
    fn from(row: credential_block::Model) -> Self {
        Self {
            id: row.id,
            credential_id: row.credential_id,
            scope: row.scope,
            operation: row.operation,
            until_ms: row.until_ms,
            source: row.source,
            observed_at_ms: row.observed_at_ms,
        }
    }
}

/// One live answer from the upstream, already persisted as cycles and blocks
/// by the time it is returned.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct QuotaSnapshotDto {
    pub observed_at_ms: i64,
    pub entries: Vec<QuotaEntryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct QuotaResetCreditsDto {
    pub credit_expirations_ms: Vec<Option<i64>>,
    pub options: Vec<QuotaResetOptionDto>,
    pub available_count: Option<u64>,
    pub expires_at_ms: Option<i64>,
}

impl From<QuotaResetCredits> for QuotaResetCreditsDto {
    fn from(credits: QuotaResetCredits) -> Self {
        Self {
            credit_expirations_ms: credits.credit_expirations_ms,
            available_count: credits.available_count,
            expires_at_ms: credits.expires_at_ms,
            options: credits.options.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct QuotaResetOptionDto {
    pub program: String,
    pub grant_id: Option<String>,
    pub label: Option<String>,
    pub available_count: Option<u64>,
    pub expires_at_ms: Option<i64>,
    pub next_available_at_ms: Option<i64>,
    pub usable: bool,
    pub ineligible_reason: Option<String>,
    pub clears: Vec<String>,
}

impl From<QuotaResetOption> for QuotaResetOptionDto {
    fn from(option: QuotaResetOption) -> Self {
        Self {
            program: option.program,
            grant_id: option.grant_id,
            label: option.label,
            available_count: option.available_count,
            expires_at_ms: option.expires_at_ms,
            next_available_at_ms: option.next_available_at_ms,
            usable: option.usable,
            ineligible_reason: option.ineligible_reason,
            clears: option.clears,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct QuotaResetWrite {
    pub program: Option<String>,
    pub grant_id: Option<String>,
    /// Reuse after an uncertain reply to avoid consuming another grant.
    pub request_id: Option<String>,
}

impl From<QuotaSnapshot> for QuotaSnapshotDto {
    fn from(snapshot: QuotaSnapshot) -> Self {
        Self {
            observed_at_ms: snapshot.observed_at_ms,
            entries: snapshot.entries.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct QuotaEntryDto {
    pub id: String,
    pub source_id: String,
    pub label: Option<String>,
    /// `account`, `organization`, `project`, `key` or `unknown`.
    pub subject: String,
    /// The channel's `QuotaScope` JSON: `"all"`, `{"models":[…]}`, … .
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub model_scope: Value,
    /// `window`, `rate_limit`, `budget`, `balance` or `breakdown`.
    pub kind: String,
    /// Present for `window`, `rate_limit` and `budget`.
    pub allowance: Option<QuotaAllowanceDto>,
    /// Present only for `balance`.
    pub balance: Option<QuotaBalanceDto>,
    pub breakdown: Option<Vec<QuotaBreakdownRowDto>>,
}

impl From<QuotaEntry> for QuotaEntryDto {
    fn from(entry: QuotaEntry) -> Self {
        let (kind, allowance, balance, breakdown) = match entry.value {
            QuotaValue::Window(allowance) => ("window", Some(allowance.into()), None, None),
            QuotaValue::RateLimit(allowance) => ("rate_limit", Some(allowance.into()), None, None),
            QuotaValue::Budget(allowance) => ("budget", Some(allowance.into()), None, None),
            QuotaValue::Breakdown(rows) => (
                "breakdown",
                None,
                None,
                Some(rows.into_iter().map(Into::into).collect()),
            ),
            QuotaValue::Balance(balance) => ("balance", None, Some(balance.into()), None),
        };
        Self {
            id: entry.id,
            source_id: entry.source_id,
            label: entry.label,
            subject: subject_name(entry.subject).to_owned(),
            // Infallible in practice: QuotaScope is a plain tagged enum.
            model_scope: serde_json::to_value(&entry.model_scope).unwrap_or(Value::Null),
            kind: kind.to_owned(),
            allowance,
            balance,
            breakdown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct QuotaBreakdownRowDto {
    pub key: String,
    pub label: Option<String>,
    pub percent: String,
}

impl From<QuotaBreakdownRow> for QuotaBreakdownRowDto {
    fn from(row: QuotaBreakdownRow) -> Self {
        Self {
            key: row.key,
            label: row.label,
            percent: row.percent.to_string(),
        }
    }
}

fn subject_name(subject: QuotaSubject) -> &'static str {
    match subject {
        QuotaSubject::Account => "account",
        QuotaSubject::Organization => "organization",
        QuotaSubject::Project => "project",
        QuotaSubject::Key => "key",
        QuotaSubject::Unknown => "unknown",
    }
}

/// None means the upstream did not report the field, never zero.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct QuotaAllowanceDto {
    pub used: Option<String>,
    pub limit: Option<String>,
    pub remaining: Option<String>,
    /// On the 0..100 scale.
    pub used_percent: Option<String>,
    pub unlimited: Option<bool>,
    pub unit: Option<String>,
    pub period_start_ms: Option<i64>,
    pub period_end_ms: Option<i64>,
    /// `periodic`, `recovering` or `unknown`.
    pub reset_behavior: String,
}

impl From<QuotaAllowance> for QuotaAllowanceDto {
    fn from(allowance: QuotaAllowance) -> Self {
        Self {
            used: allowance.used.map(|value| value.normalize().to_string()),
            limit: allowance.limit.map(|value| value.normalize().to_string()),
            remaining: allowance
                .remaining
                .map(|value| value.normalize().to_string()),
            used_percent: allowance
                .used_percent
                .map(|value| value.normalize().to_string()),
            unlimited: allowance.unlimited,
            unit: allowance.unit,
            period_start_ms: allowance.period_start_ms,
            period_end_ms: allowance.period_end_ms,
            reset_behavior: reset_behavior_name(allowance.reset_behavior).to_owned(),
        }
    }
}

fn reset_behavior_name(behavior: QuotaResetBehavior) -> &'static str {
    match behavior {
        QuotaResetBehavior::Periodic => "periodic",
        QuotaResetBehavior::Recovering => "recovering",
        QuotaResetBehavior::Unknown => "unknown",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct QuotaBalanceDto {
    pub remaining: Option<String>,
    pub unit: Option<String>,
}

impl From<QuotaBalance> for QuotaBalanceDto {
    fn from(balance: QuotaBalance) -> Self {
        Self {
            remaining: balance.remaining.map(|value| value.normalize().to_string()),
            unit: balance.unit,
        }
    }
}

/// The outcome of redeeming an upstream reset credit.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct QuotaResetDto {
    pub reason: Option<String>,
    /// `reset`, `nothing_to_reset`, `no_credit`, `already_redeemed`,
    /// `ineligible`, `unavailable` or `cooldown`.
    pub outcome: String,
    pub windows_reset: Option<u64>,
}

impl From<QuotaResetResult> for QuotaResetDto {
    fn from(result: QuotaResetResult) -> Self {
        Self {
            reason: result.reason,
            outcome: match result.outcome {
                QuotaResetOutcome::Reset => "reset",
                QuotaResetOutcome::NothingToReset => "nothing_to_reset",
                QuotaResetOutcome::NoCredit => "no_credit",
                QuotaResetOutcome::AlreadyRedeemed => "already_redeemed",
                QuotaResetOutcome::Ineligible => "ineligible",
                QuotaResetOutcome::Unavailable => "unavailable",
                QuotaResetOutcome::Cooldown => "cooldown",
            }
            .to_owned(),
            windows_reset: result.windows_reset,
        }
    }
}

/// One historical window of a configured budget or limit, joined to the
/// `quotas` row it belongs to.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct QuotaWindowDto {
    pub id: String,
    pub quota_id: String,
    /// The window's own consumption, as its settlements left it.
    pub used: String,
    pub starts_at_ms: i64,
    /// None for a permanent (`total`) window.
    pub ends_at_ms: Option<i64>,
    /// Whether the window covers the instant the query was asked about.
    pub active: bool,
    /// The quota row as it is *now*. All None when the quota has been
    /// deleted: a window keeps a historical reference, not a foreign key.
    pub owner_kind: Option<String>,
    pub owner_id: Option<String>,
    pub window_key: Option<String>,
    pub metric: Option<String>,
    pub unit: Option<String>,
    pub limit_value: Option<String>,
    pub period: Option<String>,
    /// The quota as it was when this window opened, which is what the window
    /// was actually metered against if the row has changed since.
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub quota_snapshot: Value,
}

/// One request's contribution to a window.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct QuotaSettlementDto {
    pub window_id: String,
    /// A downstream request id for a caller budget, an upstream capture id
    /// for a pool budget.
    pub request_id: String,
    pub amount: String,
    pub settled_at_ms: i64,
}

impl From<quota_settlement::Model> for QuotaSettlementDto {
    fn from(row: quota_settlement::Model) -> Self {
        Self {
            window_id: row.window_id,
            request_id: row.request_id,
            amount: row.amount.to_string(),
            settled_at_ms: row.settled_at_ms,
        }
    }
}

/// One fixed window of a counted dimension on a credential: what the meter
/// itself recorded, in the meter's own units.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct CountedWindowDto {
    pub credential_id: String,
    /// A channel-declared dimension, or `limit:{quota_id}` for the synthetic
    /// dimension an operator limit meters through.
    pub dimension: String,
    pub window_start_ms: i64,
    pub window_end_ms: i64,
    /// Raw meter units: requests for a request dimension, fixed-point atoms
    /// for a cost one. [`CredentialLimitStatusDto`] carries the same window
    /// as a normalized decimal.
    pub used: i64,
    pub limit: i64,
}

impl From<counted_window::Model> for CountedWindowDto {
    fn from(row: counted_window::Model) -> Self {
        Self {
            credential_id: row.credential_id,
            dimension: row.dimension,
            window_start_ms: row.window_start_ms,
            window_end_ms: row.window_end_ms,
            used: row.used,
            limit: row.limit,
        }
    }
}

/// What a window listing filters on.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct QuotaWindowQuery {
    pub quota_id: Option<String>,
    /// Both owner fields select the quotas first; a kind without an id is a
    /// valid filter on its own.
    pub owner_kind: Option<String>,
    pub owner_id: Option<String>,
    /// Keep only the window covering `now`, dropping the closed history.
    #[serde(default)]
    pub active_only: bool,
    /// 1-based. Zero and absent both mean the first page.
    pub page: Option<u64>,
    /// Clamped to 1..=500; absent means 50.
    pub page_size: Option<u64>,
}
