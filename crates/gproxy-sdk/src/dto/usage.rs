//! Usage as it is read back: one request's record, and the aggregates a
//! console asks for over many of them.
//!
//! `usage_records.metrics` is one JSON document core's observer writes per
//! request — normalized token counts, the per-exchange breakdown behind them,
//! the settlement state and the priced cost. The database cannot sum it, so
//! everything below the record itself is summed in Rust over a bounded scan.
//! The extracted fields never replace the document: `metrics` travels with the
//! record so a detail view can show what no aggregate models.
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
    /// The `tokens` object of one `usage_json` document, which is the shape
    /// core writes both at the top level of `metrics` and inside every
    /// exchange. A missing or malformed field reads as "not reported".
    pub(crate) fn read(usage: Option<&Value>) -> Self {
        let tokens = usage.and_then(|usage| usage.get("tokens"));
        let count = |name: &str| tokens.and_then(|t| t.get(name)).and_then(Value::as_u64);
        Self {
            input_tokens: count("input_tokens"),
            output_tokens: count("output_tokens"),
            cached_input_tokens: count("cached_input_tokens"),
            cache_creation_5m_tokens: count("cache_creation_5m_tokens"),
            cache_creation_30m_tokens: count("cache_creation_30m_tokens"),
            cache_creation_1h_tokens: count("cache_creation_1h_tokens"),
            reasoning_tokens: count("reasoning_tokens"),
        }
    }
}

/// `{"amount": "…", "currency": "…"}` as core writes a priced cost, or two
/// `None` when nothing was priced.
pub(crate) fn money(value: Option<&Value>) -> (Option<String>, Option<String>) {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return (None, None);
    };
    (
        value
            .get("amount")
            .and_then(Value::as_str)
            .map(str::to_owned),
        value
            .get("currency")
            .and_then(Value::as_str)
            .map(str::to_owned),
    )
}

fn text(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(str::to_owned)
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
    pub tokens: UsageTokensDto,
    /// The settled charge, from the indexed column rather than the document.
    pub cost: Option<String>,
    pub currency: Option<String>,
    /// One entry per upstream attempt that produced usage, which is where a
    /// provider and a credential are named.
    pub exchanges: Vec<UsageExchangeDto>,
    /// The whole document, so nothing above is a lossy summary of it.
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub metrics: Value,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
}

impl From<usage_record::Model> for UsageRecordDto {
    fn from(row: usage_record::Model) -> Self {
        let tokens = UsageTokensDto::read(Some(&row.metrics));
        let (_, currency) = money(row.metrics.get("cost"));
        let exchanges = row
            .metrics
            .get("exchanges")
            .and_then(Value::as_array)
            .map(|items| items.iter().map(UsageExchangeDto::read).collect())
            .unwrap_or_default();
        Self {
            request_id: row.request_id,
            user_id: row.user_id,
            api_key_id: row.api_key_id,
            model: row.model,
            operation: row.operation,
            state: text(row.metrics.get("state")),
            completeness: text(row.metrics.get("completeness")),
            tokens,
            cost: row.cost.map(|cost| cost.to_string()),
            currency,
            exchanges,
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
    pub attempt_id: Option<String>,
    pub attempt_ordinal: Option<i64>,
    pub provider_id: Option<String>,
    pub credential_id: Option<String>,
    /// The upstream model name, which need not be the requested one.
    pub model: Option<String>,
    pub tokens: UsageTokensDto,
    pub cost: Option<String>,
    pub currency: Option<String>,
}

impl UsageExchangeDto {
    pub(crate) fn read(value: &Value) -> Self {
        let (cost, currency) = money(value.get("cost"));
        Self {
            capture_id: text(value.get("capture_id")),
            attempt_id: text(value.get("attempt_id")),
            attempt_ordinal: value.get("attempt_ordinal").and_then(Value::as_i64),
            provider_id: text(value.get("provider_id")),
            credential_id: text(value.get("credential_id")),
            model: text(value.get("model")),
            tokens: UsageTokensDto::read(value.get("usage")),
            cost,
            currency,
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
    /// A normalized decimal string, `"0"` when nothing was priced.
    pub cost: String,
    /// None when no record in the scan carried a currency, or when two
    /// disagreed: summing dollars and euros into one number would be a lie.
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
    /// Records any of whose attempts reached this provider. Not a column, so
    /// the list is then a bounded scan — see [`UsageRecordPage::truncated`].
    pub provider_id: Option<String>,
    /// Records any of whose attempts were served by this credential. Same
    /// scan as `provider_id`; the records come back whole, every attempt
    /// included, because a list shows what happened rather than a share of it.
    pub credential_id: Option<String>,
    /// 1-based. Zero and absent both mean the first page.
    pub page: Option<u64>,
    /// Clamped to 1..=500; absent means 50.
    pub page_size: Option<u64>,
}

/// One page of records.
///
/// The fields of [`Page`](crate::dto::Page), plus what an attempt filter
/// costs: with `providerId` or `credentialId` set the page is cut from a scan
/// of at most `query::MAX_SCAN_ROWS` records, newest first, and `total` counts
/// the matches among those. `truncated` says the scan stopped with older
/// records unread, so `total` is a lower bound and a page past it is empty
/// rather than proof that nothing older matches; narrow the time range to
/// reach them. Without an attempt filter the page is a plain database page and
/// `truncated` is always false.
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

/// What an aggregate filters on, and how far it is allowed to read.
///
/// `provider_id` and `credential_id` are not columns: they name upstream
/// attempts inside a record's `exchanges[]`, and a record can hold several —
/// a request that failed over from one credential to another. With either set
/// the aggregate cuts at the attempt, the same way the per-provider grouping
/// always has:
///
/// - a record matches when at least one of its attempts matches every
///   attempt filter given, and it then counts as one request;
/// - its tokens and cost are the sum over the matching attempts only, each
///   priced on its own, never the record's settled total — so a failed-over
///   request contributes to each credential exactly what that credential
///   spent, and the two cuts add back up to the whole;
/// - the summary, every trend bucket and every column group apply that same
///   rule, so they stay consistent with one another; a group by provider or
///   credential additionally keys each matching attempt by its own id.
///
/// Without either filter nothing changes: a record contributes its settled
/// cost column and its top-level token totals. `scanned` and the row cap count
/// every record the column filters let through, matching or not, because the
/// attempt filters are applied after the rows are read.
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
    /// Read from the per-exchange breakdown inside `metrics`, not from a
    /// column: a request that failed over is counted under every provider it
    /// actually reached, with that provider's own tokens and cost.
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
