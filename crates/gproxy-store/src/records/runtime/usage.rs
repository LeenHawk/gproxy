use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageInput {
    #[serde(default)]
    pub upstream_started_at_ms: Option<i64>,
    pub request_id: String,
    pub at: i64,
    pub provider_id: i64,
    pub credential_id: i64,
    pub organization_id: Option<i64>,
    pub team_id: Option<i64>,
    pub user_id: Option<i64>,
    pub user_key_id: Option<i64>,
    pub operation: Option<String>,
    pub upstream_model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub metrics: Value,
    pub dimensions: Value,
    pub cost: Decimal,
    pub usage_source: String,
    pub ended: String,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: i64,
    pub usage: UsageInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentUsageRecord {
    pub request_id: String,
    pub at: i64,
    pub provider_id: i64,
    pub operation: Option<String>,
    pub upstream_model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cost: Decimal,
    pub usage_source: String,
    pub ended: String,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageWindow {
    pub cost: Decimal,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageGroupBy {
    UserKey,
    User,
    Provider,
    Model,
    Dimensions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageAggregateQuery {
    pub from: i64,
    pub to: i64,
    pub group_by: UsageGroupBy,
    pub user_key_id: Option<i64>,
    pub user_id: Option<i64>,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageAggregateRecord {
    pub group: String,
    pub user_key_id: Option<i64>,
    pub user_id: Option<i64>,
    pub provider_id: i64,
    pub model: String,
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_creation_5m_tokens: u64,
    pub cache_creation_30m_tokens: u64,
    pub cache_creation_1h_tokens: u64,
    pub cost: Decimal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageTrendPoint {
    pub bucket_start: i64,
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cost: Decimal,
}
