use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::Scope;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct UsageQuery {
    pub provider_id: Scope<i64>,
    pub credential_id: Scope<i64>,
    pub channel: Scope<String>,
    pub model: Scope<String>,
    pub user_id: Scope<i64>,
    pub user_key_id: Scope<i64>,
    pub from_unix_ms: Option<i64>,
    pub to_unix_ms: Option<i64>,
    pub cursor_at_unix_ms: Option<i64>,
    pub cursor_trace_id: Option<i64>,
    pub offset: Option<u64>,
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageQueryRow {
    pub trace_id: i64,
    pub downstream_trace_id: Option<i64>,
    pub at: OffsetDateTime,
    pub provider_id: Option<i64>,
    pub provider_channel: Option<String>,
    pub credential_id: Option<i64>,
    pub user_id: Option<i64>,
    pub user_key_id: Option<i64>,
    pub operation: String,
    pub protocol: String,
    pub model: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_input_tokens: Option<i64>,
    pub cache_creation_input_tokens: Option<i64>,
    pub cache_creation_input_tokens_5min: Option<i64>,
    pub cache_creation_input_tokens_1h: Option<i64>,
    /// Per-request quota cost charged when this row was recorded. Same
    /// unit as `user_quotas.cost_used`. Zero on rows written before this
    /// column was added.
    pub cost: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct UsageSummary {
    pub count: u64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_creation_input_tokens_5min: i64,
    pub cache_creation_input_tokens_1h: i64,
    /// Sum of `usages.cost` across the matched rows. Used by the metric
    /// cards on the admin and per-user dashboards.
    pub total_cost: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UsageQueryCount {
    pub count: u64,
}
