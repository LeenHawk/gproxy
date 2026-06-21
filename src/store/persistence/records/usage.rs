//! Usage records (§8-D): per-request usage and rollups. All tables are
//! append/accumulate only; retention is deferred.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// A per-request usage row (one logical proxied request, §8-D).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    pub id: i64,
    pub request_id: String,
    /// Unix seconds.
    pub at: i64,
    pub route_name: Option<String>,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub org_id: Option<i64>,
    pub team_id: Option<i64>,
    pub user_id: Option<i64>,
    pub user_key_id: Option<i64>,
    pub operation: String,
    pub kind: String,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_5m_tokens: i64,
    pub cache_creation_1h_tokens: i64,
    #[serde(with = "rust_decimal::serde::str")]
    pub cost: Decimal,
    /// §15.3: upstream latency (ms, time-to-first-response) of the settled
    /// attempt; 0 when unmeasured (wasm has no monotonic clock).
    #[serde(default)]
    pub latency_ms: i64,
    /// §17: `upstream` | `counted` | `estimated` (empty on pre-M6 rows).
    #[serde(default)]
    pub usage_source: String,
    /// §17: `complete` | `interrupted` (empty on pre-M6 rows).
    #[serde(default)]
    pub ended: String,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds.
    pub updated_at: i64,
}

/// Append input for a usage row (append-only; no id).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageInput {
    pub request_id: String,
    pub at: i64,
    pub route_name: Option<String>,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub org_id: Option<i64>,
    pub team_id: Option<i64>,
    pub user_id: Option<i64>,
    pub user_key_id: Option<i64>,
    pub operation: String,
    pub kind: String,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_5m_tokens: i64,
    pub cache_creation_1h_tokens: i64,
    #[serde(with = "rust_decimal::serde::str")]
    pub cost: Decimal,
    /// §15.3: upstream latency (ms) of the settled attempt; 0 when unmeasured.
    #[serde(default)]
    pub latency_ms: i64,
    /// §17: `upstream` | `counted` | `estimated`.
    #[serde(default)]
    pub usage_source: String,
    /// §17: `complete` | `interrupted`.
    #[serde(default)]
    pub ended: String,
}

/// An aggregated usage bucket for one `(granularity, bucket_start, dimensions)`
/// tuple (§8-D). Metrics are accumulated across requests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageRollup {
    pub id: i64,
    /// `hour` | `day` | ... (free-form granularity label).
    pub granularity: String,
    /// Unix seconds at the start of the bucket.
    pub bucket_start: i64,
    pub provider_id: Option<i64>,
    pub org_id: Option<i64>,
    pub team_id: Option<i64>,
    pub user_id: Option<i64>,
    pub route_name: Option<String>,
    pub model: Option<String>,
    pub requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    #[serde(with = "rust_decimal::serde::str")]
    pub cost: Decimal,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds.
    pub updated_at: i64,
}

/// Accumulate input for a rollup bucket: dimension fields locate (or create)
/// the bucket; the metric fields are added as deltas. No id (accumulate-only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageRollupInput {
    pub granularity: String,
    pub bucket_start: i64,
    pub provider_id: Option<i64>,
    pub org_id: Option<i64>,
    pub team_id: Option<i64>,
    pub user_id: Option<i64>,
    pub route_name: Option<String>,
    pub model: Option<String>,
    pub requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    #[serde(with = "rust_decimal::serde::str")]
    pub cost: Decimal,
}
