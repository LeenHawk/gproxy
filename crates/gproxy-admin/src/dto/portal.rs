use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::UserKeyPrefix;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PortalContextDto {
    pub user_name: String,
    pub recent_requests_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PortalLoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PortalSessionStatusDto {
    pub user: Option<PortalContextDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PortalPasswordChangeRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PortalKeyCreateRequest {
    pub prefix: UserKeyPrefix,
    pub label: Option<String>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PortalSettingsDto {
    pub recent_requests_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PortalModelCapabilityDto {
    pub source: String,
    pub operation: String,
    pub group: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PortalModelDto {
    pub name: String,
    pub capabilities: Vec<PortalModelCapabilityDto>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PortalUsageQueryDto {
    pub from: i64,
    pub to: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PortalUsageDto {
    pub from: i64,
    pub to: i64,
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cost: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum PortalQuotaScopeDto {
    User,
    Organization,
    Team,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum PortalQuotaWindowKindDto {
    Total,
    Daily,
    Weekly,
    Monthly,
    #[serde(rename = "5h")]
    #[ts(rename = "5h")]
    FiveHour,
    #[serde(rename = "7d")]
    #[ts(rename = "7d")]
    SevenDay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PortalQuotaWindowDto {
    pub scope: PortalQuotaScopeDto,
    pub window_kind: PortalQuotaWindowKindDto,
    pub window_start: Option<i64>,
    pub reset_at: Option<i64>,
    pub started: bool,
    pub cost_used: String,
    pub cost_limit: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PortalRecentQueryDto {
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PortalRecentRequestDto {
    pub request_id: String,
    pub at: i64,
    pub provider_name: Option<String>,
    pub operation: Option<String>,
    pub upstream_model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cost: String,
    pub usage_source: String,
    pub ended: String,
    pub latency_ms: u64,
}
