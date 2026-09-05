use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UsageRecordQueryDto {
    pub from: i64,
    pub to: i64,
    pub user_key_id: Option<i64>,
    pub user_id: Option<i64>,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub model: Option<String>,
    pub request_id: Option<String>,
    pub operation: Option<String>,
    pub usage_source: Option<String>,
    pub ended: Option<String>,
    pub page: Option<u64>,
    pub page_size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct UsageRecordDto {
    pub id: i64,
    pub request_id: String,
    pub at: i64,
    pub provider_id: i64,
    pub credential_id: i64,
    pub user_id: Option<i64>,
    pub user_key_id: Option<i64>,
    pub operation: Option<String>,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    #[ts(type = "Record<string, string>")]
    pub metrics: Value,
    #[ts(type = "Record<string, string>")]
    pub dimensions: Value,
    pub cost: String,
    pub usage_source: String,
    pub ended: String,
    pub latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct UsageRecordPageDto {
    pub items: Vec<UsageRecordDto>,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct UsageSummaryDto {
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub total_tokens: String,
    pub cost: String,
    pub metrics: std::collections::BTreeMap<String, String>,
}

impl From<gproxy_store::records::UsageTotals> for UsageSummaryDto {
    fn from(value: gproxy_store::records::UsageTotals) -> Self {
        Self {
            total_tokens: value.total_tokens().normalize().to_string(),
            requests: value.requests,
            input_tokens: value.input_tokens,
            output_tokens: value.output_tokens,
            cached_input_tokens: value.cached_input_tokens,
            cost: value.cost.normalize().to_string(),
            metrics: value
                .metrics
                .into_iter()
                .map(|(key, amount)| (key, amount.normalize().to_string()))
                .collect(),
        }
    }
}
