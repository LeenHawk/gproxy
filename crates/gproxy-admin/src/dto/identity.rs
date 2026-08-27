use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OrganizationDto {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OrganizationWriteRequest {
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TeamDto {
    pub id: i64,
    pub organization_id: i64,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TeamWriteRequest {
    pub organization_id: i64,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UserDto {
    pub id: i64,
    pub name: String,
    pub organization_id: Option<i64>,
    pub team_id: Option<i64>,
    pub enabled: bool,
    pub is_admin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UserWriteRequest {
    pub name: String,
    pub organization_id: Option<i64>,
    pub team_id: Option<i64>,
    pub enabled: bool,
    pub is_admin: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename_all = "lowercase")]
pub enum UserKeyPrefix {
    Sk,
    At,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UserKeyDto {
    pub id: i64,
    pub user_id: i64,
    pub prefix: Option<String>,
    pub label: Option<String>,
    pub revealable: bool,
    pub expires_at: Option<i64>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UserKeyCreateRequest {
    pub user_id: i64,
    pub prefix: UserKeyPrefix,
    pub label: Option<String>,
    pub expires_at: Option<i64>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UserKeyUpdateRequest {
    pub label: Option<String>,
    pub expires_at: Option<i64>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UserKeyCreateResponse {
    pub id: i64,
    pub api_key: String,
    pub prefix: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UserKeyRevealResponse {
    pub id: i64,
    pub api_key: String,
    pub revealed_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PermissionDto {
    pub id: i64,
    pub subject_kind: String,
    pub subject_id: i64,
    pub provider_id: Option<i64>,
    pub operation_group: Option<String>,
    pub allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PermissionWriteRequest {
    pub subject_kind: String,
    pub subject_id: i64,
    pub provider_id: Option<i64>,
    pub operation_group: Option<String>,
    pub allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RateLimitDto {
    pub id: i64,
    pub subject_kind: String,
    pub subject_id: i64,
    pub requests: u64,
    pub window_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RateLimitWriteRequest {
    pub subject_kind: String,
    pub subject_id: i64,
    pub requests: u64,
    pub window_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct QuotaDto {
    pub id: i64,
    pub subject_kind: String,
    pub subject_id: i64,
    pub quota_total: String,
    pub quota_daily: Option<String>,
    pub quota_weekly: Option<String>,
    pub quota_monthly: Option<String>,
    pub quota_5h: Option<String>,
    pub quota_7d: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct QuotaWriteRequest {
    pub subject_kind: String,
    pub subject_id: i64,
    pub quota_total: String,
    pub quota_daily: Option<String>,
    pub quota_weekly: Option<String>,
    pub quota_monthly: Option<String>,
    pub quota_5h: Option<String>,
    pub quota_7d: Option<String>,
    pub enabled: bool,
}
