use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    AliasRecord, CredentialMetaRecord, ExposedModelRecord, PermissionRecord, PriceRateRecord,
    PriceRuleRecord, ProviderRecord, QuotaRecord, RateLimitRecord, RouteMemberRecord, RouteRecord,
    UserKeyRecord, UserRecord,
};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ControlSnapshot {
    pub providers: Vec<ProviderRecord>,
    pub credentials: Vec<CredentialMetaRecord>,
    pub routes: Vec<RouteRecord>,
    pub route_members: Vec<RouteMemberRecord>,
    pub aliases: Vec<AliasRecord>,
    pub exposed_models: Vec<ExposedModelRecord>,
    pub users: Vec<UserRecord>,
    pub user_keys: Vec<UserKeyRecord>,
    pub permissions: Vec<PermissionRecord>,
    pub rate_limits: Vec<RateLimitRecord>,
    pub quotas: Vec<QuotaRecord>,
    pub price_rules: Vec<PriceRuleRecord>,
    pub price_rates: Vec<PriceRateRecord>,
    pub settings: Vec<SettingRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingInput {
    pub key: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingRecord {
    pub key: String,
    pub value: Value,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaWindowRecord {
    pub quota_id: i64,
    pub window_start: i64,
    pub used_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureInput {
    pub request_id: String,
    pub at: i64,
    pub provider_id: Option<i64>,
    pub credential_id: Option<i64>,
    pub upstream_url: Option<String>,
    pub response_status: Option<u16>,
    pub request_body: Vec<u8>,
    pub response_body: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestLogInput {
    pub request_id: String,
    pub at: i64,
    pub method: String,
    pub path: String,
    pub query: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BindingInput {
    pub provider_id: i64,
    pub owner_user_id: i64,
    pub kind: String,
    pub resource_id: String,
    pub credential_id: i64,
    pub summary: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BindingRecord {
    pub provider_id: i64,
    pub owner_user_id: i64,
    pub kind: String,
    pub resource_id: String,
    pub credential_id: i64,
    pub summary: Value,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BindingPage {
    pub items: Vec<BindingRecord>,
    pub next_cursor: Option<String>,
}
