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
    pub id: i64,
    pub quota_id: i64,
    pub window_kind: QuotaWindowKind,
    pub window_start: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_at: Option<i64>,
    pub cost_used: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuotaWindowKind {
    Total,
    Daily,
    Weekly,
    Monthly,
    #[serde(rename = "5h")]
    FiveHour,
    #[serde(rename = "7d")]
    SevenDay,
}

impl QuotaWindowKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Total => "total",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::FiveHour => "5h",
            Self::SevenDay => "7d",
        }
    }

    pub fn from_name(value: &str) -> Option<Self> {
        match value {
            "total" => Some(Self::Total),
            "daily" => Some(Self::Daily),
            "weekly" => Some(Self::Weekly),
            "monthly" => Some(Self::Monthly),
            "5h" => Some(Self::FiveHour),
            "7d" => Some(Self::SevenDay),
            _ => None,
        }
    }
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
