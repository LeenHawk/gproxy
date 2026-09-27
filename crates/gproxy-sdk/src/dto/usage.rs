//! Usage records and aggregates read from fixed columns.
//!
//! A token count on a single record is `Option<u64>`: an upstream that did not
//! report a field did not measure zero. A total is a plain `u64`, because a
//! sum of "nothing reported" really is zero.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use gproxy_store::entity::usage::usage_record;

/// Token counts exactly as the upstream reported them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct UsageTokensDto {
    /// Ordinary input, excluding cache reads and cache writes.
    pub input_tokens: Option<u64>,
    /// Total output, reasoning included.
    pub output_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub cache_creation_5m_tokens: Option<u64>,
    pub cache_creation_30m_tokens: Option<u64>,
    pub cache_creation_1h_tokens: Option<u64>,
    /// A subset of `output_tokens`, never additive with it.
    pub reasoning_tokens: Option<u64>,
}

impl UsageTokensDto {
    pub(crate) fn from_row(row: &usage_record::Model) -> Self {
        Self {
            input_tokens: row.input_tokens(),
            output_tokens: row.output_tokens(),
            cached_input_tokens: row.cached_input_tokens(),
            cache_creation_5m_tokens: row.cache_creation_5m_tokens(),
            cache_creation_30m_tokens: row.cache_creation_30m_tokens(),
            cache_creation_1h_tokens: row.cache_creation_1h_tokens(),
            reasoning_tokens: row.reasoning_tokens(),
        }
    }
}

/// One persisted request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct UsageRecordDto {
    /// The downstream exchange or websocket turn this usage belongs to, which
    /// is also the id of its downstream `capture_records` row.
    pub request_id: String,
    pub user_id: Option<String>,
    pub api_key_id: Option<String>,
    /// The name the caller asked for, falling back to the upstream model.
    pub model: String,
    pub operation: String,
    /// The settlement state core recorded: `settled`, `failed`, … .
    pub state: Option<String>,
    /// `complete`, `partial` or `unknown`.
    pub completeness: Option<String>,
    pub actual_service_tier: Option<String>,
    pub tokens: UsageTokensDto,
    /// Reported media/tool quantities, as exact decimal strings. Missing is unknown.
    pub quantities: std::collections::BTreeMap<String, String>,
    /// The settled USD charge, read from its dedicated column.
    pub cost: Option<String>,
    pub currency: Option<String>,
    /// One entry per upstream attempt that produced usage, which is where a
    /// provider and a credential are named.
    pub exchanges: Vec<UsageExchangeDto>,
    /// Dynamic dimensions and nested protocol-specific detail; fixed fields are above.
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub metrics: Value,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
}

impl From<gproxy_core::usage_scan::UsageRecord> for UsageRecordDto {
    fn from(record: gproxy_core::usage_scan::UsageRecord) -> Self {
        let row = record.row;
        Self {
            tokens: UsageTokensDto::from_row(&row),
            quantities: quantities(&row),
            exchanges: record
                .exchanges
                .iter()
                .map(UsageExchangeDto::from_row)
                .collect(),
            currency: row.cost.map(|_| "USD".into()),
            cost: row.cost.map(|cost| cost.to_string()),
            request_id: row.request_id,
            user_id: row.user_id,
            api_key_id: row.api_key_id,
            model: row.model,
            operation: row.operation,
            state: row.state,
            completeness: row.completeness,
            actual_service_tier: row.actual_service_tier,
            metrics: row.metrics,
            started_at_ms: row.started_at_ms,
            ended_at_ms: row.ended_at_ms,
        }
    }
}

/// One upstream attempt inside a request: which provider and credential
/// served it, and what it cost on its own.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct UsageExchangeDto {
    pub capture_id: Option<String>,
    pub completeness: Option<String>,
    pub actual_service_tier: Option<String>,
    /// Dynamic dimensions and nested protocol-specific usage detail.
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub metrics: Value,
    pub attempt_id: Option<String>,
    pub attempt_ordinal: Option<i64>,
    pub provider_id: Option<String>,
    pub credential_id: Option<String>,
    /// The upstream model name, which need not be the requested one.
    pub model: Option<String>,
    pub tokens: UsageTokensDto,
    /// Reported media/tool quantities, as exact decimal strings. Missing is unknown.
    pub quantities: std::collections::BTreeMap<String, String>,
    pub cost: Option<String>,
    pub currency: Option<String>,
}

impl UsageExchangeDto {
    pub(crate) fn from_row(row: &usage_record::Model) -> Self {
        Self {
            completeness: row.completeness.clone(),
            actual_service_tier: row.actual_service_tier.clone(),
            metrics: row.metrics.clone(),
            capture_id: Some(row.request_id.clone()),
            attempt_id: row.attempt_id.clone(),
            attempt_ordinal: row.attempt_ordinal,
            provider_id: row.provider_id.clone(),
            credential_id: row.credential_id.clone(),
            model: (!row.model.is_empty()).then(|| row.model.clone()),
            tokens: UsageTokensDto::from_row(row),
            quantities: quantities(row),
            cost: row.cost.map(|cost| cost.to_string()),
            currency: row.cost.map(|_| "USD".into()),
        }
    }
}

/// Totals over the records a scan covered.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct UsageSummaryDto {
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    /// Total cache writes across all three retention periods.
    pub cache_creation_tokens: u64,
    pub cache_creation_5m_tokens: u64,
    pub cache_creation_30m_tokens: u64,
    pub cache_creation_1h_tokens: u64,
    /// Part of `output_tokens`, listed rather than added.
    pub reasoning_tokens: u64,
    /// Totals of reported media/tool quantities; keys are billable metric names.
    pub quantities: std::collections::BTreeMap<String, String>,
    /// A normalized decimal string, `"0"` when nothing was priced.
    pub cost: String,
    /// USD when any record was priced; None when none was priced.
    pub currency: Option<String>,
    /// The scan stopped at its row cap. The numbers describe `scanned`
    /// records, not every record the filters match.
    pub truncated: bool,
    pub scanned: u64,
}

/// One bucket of a grouped aggregate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct UsageGroupDto {
    /// The grouping column's value. None is "the records carried none" — an
    /// anonymous request has no user, a failed attempt no provider.
    pub key: Option<String>,
    /// `truncated` and `scanned` describe the one scan every group came from,
    /// not this group alone.
    pub summary: UsageSummaryDto,
}

/// One fixed-width bucket of a trend. Present even when empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct UsageTrendPointDto {
    pub start_ms: i64,
    /// Exclusive, always `start_ms + bucketMs`: the last bucket keeps the
    /// width of the others even when the requested range ends inside it.
    pub end_ms: i64,
    pub summary: UsageSummaryDto,
}

/// What a record listing filters on. Timestamps bound `started_at_ms`:
/// `from_ms` is inclusive, `to_ms` exclusive.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct UsageRecordQuery {
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub user_id: Option<String>,
    pub api_key_id: Option<String>,
    pub model: Option<String>,
    pub operation: Option<String>,
    pub request_id: Option<String>,
    /// Records whose structured upstream usage matches this provider.
    pub provider_id: Option<String>,
    /// Records whose structured upstream usage matches this credential.
    pub credential_id: Option<String>,
    /// 1-based. Zero and absent both mean the first page.
    pub page: Option<u64>,
    /// Clamped to 1..=500; absent means 50.
    pub page_size: Option<u64>,
}

/// One exact database page; provider and credential filters are SQL predicates.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct UsageRecordPage {
    pub items: Vec<UsageRecordDto>,
    pub total: u64,
    pub offset: u64,
    pub limit: u64,
    pub truncated: bool,
}

/// Filters for bounded aggregates. Without provider/credential filters, totals
/// use downstream summaries. With either filter, totals use only matching
/// upstream quantities and costs, counting each downstream request once.
/// The scan budget counts downstream requests after SQL filtering.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct UsageQuery {
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
    pub user_id: Option<String>,
    pub api_key_id: Option<String>,
    pub model: Option<String>,
    pub operation: Option<String>,
    /// Only what this provider's attempts account for. See the type note.
    pub provider_id: Option<String>,
    /// Only what this credential's attempts account for. See the type note.
    pub credential_id: Option<String>,
    /// How many matching records the aggregation may read before it stops and
    /// says so. Absent means `query::MAX_SCAN_ROWS`; the value is clamped to
    /// it, so a caller cannot ask the process to read a year of traffic into
    /// memory.
    pub max_scan_rows: Option<u64>,
}

/// Which column the aggregate is cut by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub enum UsageGroupBy {
    User,
    ApiKey,
    /// The name the caller asked for, as stored on the record.
    Model,
    Operation,
    /// Group structured upstream usage by provider, with each provider's own quantities and cost.
    Provider,
    /// The same per-attempt cut as `Provider`, keyed by the credential that
    /// served each attempt: what each upstream account actually spent.
    Credential,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct UsageGroupQuery {
    pub filter: UsageQuery,
    pub group_by: UsageGroupBy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct UsageTrendQuery {
    /// `from_ms` and `to_ms` are required here: buckets are aligned to
    /// `from_ms` and counted up to `to_ms`, and neither has a sane default.
    pub filter: UsageQuery,
    /// Bucket width in milliseconds. Zero is refused, and so is a range that
    /// would produce more than `query::MAX_TREND_BUCKETS` buckets.
    pub bucket_ms: i64,
}

fn quantities(row: &usage_record::Model) -> std::collections::BTreeMap<String, String> {
    row.quantities()
        .into_iter()
        .map(|(key, value)| (key, value.normalize().to_string()))
        .collect()
}
