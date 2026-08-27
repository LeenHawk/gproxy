use serde::{Deserialize, Serialize};

use super::CredentialEnvelope;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationInput {
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationRecord {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamInput {
    pub organization_id: i64,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamRecord {
    pub id: i64,
    pub organization_id: i64,
    pub name: String,
    pub enabled: bool,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInput {
    pub name: String,
    pub organization_id: Option<i64>,
    pub team_id: Option<i64>,
    pub password_hash: Option<String>,
    pub enabled: bool,
    pub is_admin: bool,
}

impl std::fmt::Debug for UserInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UserInput")
            .field("name", &self.name)
            .field("organization_id", &self.organization_id)
            .field("team_id", &self.team_id)
            .field(
                "password_hash",
                &self.password_hash.as_ref().map(|_| "<redacted>"),
            )
            .field("enabled", &self.enabled)
            .field("is_admin", &self.is_admin)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserRecord {
    pub id: i64,
    pub name: String,
    pub organization_id: Option<i64>,
    pub team_id: Option<i64>,
    pub enabled: bool,
    pub is_admin: bool,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserKeyInput {
    pub user_id: i64,
    pub digest: Vec<u8>,
    pub digest_version: u32,
    pub prefix: String,
    pub envelope: CredentialEnvelope,
    pub label: Option<String>,
    pub expires_at: Option<i64>,
    pub enabled: bool,
}

impl std::fmt::Debug for UserKeyInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UserKeyInput")
            .field("user_id", &self.user_id)
            .field("digest", &"<redacted>")
            .field("digest_version", &self.digest_version)
            .field("prefix", &self.prefix)
            .field("envelope", &"<redacted>")
            .field("label", &self.label)
            .field("expires_at", &self.expires_at)
            .field("enabled", &self.enabled)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserKeyRecord {
    pub id: i64,
    pub user_id: i64,
    pub digest: Vec<u8>,
    pub digest_version: u32,
    pub prefix: Option<String>,
    pub label: Option<String>,
    pub revealable: bool,
    pub expires_at: Option<i64>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserKeyUpdateInput {
    pub label: Option<String>,
    pub expires_at: Option<i64>,
    pub enabled: bool,
}

impl std::fmt::Debug for UserKeyRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UserKeyRecord")
            .field("id", &self.id)
            .field("user_id", &self.user_id)
            .field("digest", &"<redacted>")
            .field("digest_version", &self.digest_version)
            .field("prefix", &self.prefix)
            .field("label", &self.label)
            .field("revealable", &self.revealable)
            .field("expires_at", &self.expires_at)
            .field("enabled", &self.enabled)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionInput {
    pub subject_kind: String,
    pub subject_id: i64,
    pub provider_id: Option<i64>,
    pub operation_group: Option<String>,
    pub allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionRecord {
    pub id: i64,
    pub subject_kind: String,
    pub subject_id: i64,
    pub provider_id: Option<i64>,
    pub operation_group: Option<String>,
    pub allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitInput {
    pub subject_kind: String,
    pub subject_id: i64,
    pub requests: u64,
    pub window_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitRecord {
    pub id: i64,
    pub subject_kind: String,
    pub subject_id: i64,
    pub requests: u64,
    pub window_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaInput {
    pub subject_kind: String,
    pub subject_id: i64,
    pub quota_total: rust_decimal::Decimal,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_daily: Option<rust_decimal::Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_weekly: Option<rust_decimal::Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_monthly: Option<rust_decimal::Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_5h: Option<rust_decimal::Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_7d: Option<rust_decimal::Decimal>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaRecord {
    pub id: i64,
    pub subject_kind: String,
    pub subject_id: i64,
    pub quota_total: rust_decimal::Decimal,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_daily: Option<rust_decimal::Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_weekly: Option<rust_decimal::Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_monthly: Option<rust_decimal::Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_5h: Option<rust_decimal::Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_7d: Option<rust_decimal::Decimal>,
    pub enabled: bool,
}
