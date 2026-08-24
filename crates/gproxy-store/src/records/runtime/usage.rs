use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageInput {
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageWindow {
    pub cost: Decimal,
    pub input_tokens: u64,
    pub output_tokens: u64,
}
