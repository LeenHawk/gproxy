use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::CredentialEnvelope;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminUserRecord {
    pub id: i64,
    pub name: String,
    pub password_hash: String,
    pub enabled: bool,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserAuthRecord {
    pub id: i64,
    pub name: String,
    pub organization_id: Option<i64>,
    pub team_id: Option<i64>,
    pub password_hash: String,
    pub enabled: bool,
}

impl std::fmt::Debug for UserAuthRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UserAuthRecord")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("organization_id", &self.organization_id)
            .field("team_id", &self.team_id)
            .field("password_hash", &"<redacted>")
            .field("enabled", &self.enabled)
            .finish()
    }
}

impl std::fmt::Debug for AdminUserRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AdminUserRecord")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("password_hash", &"<redacted>")
            .field("enabled", &self.enabled)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserSessionInput {
    pub token_digest: Vec<u8>,
    pub user_id: i64,
    pub created_at: i64,
    pub expires_at: i64,
}

impl std::fmt::Debug for UserSessionInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UserSessionInput")
            .field("token_digest", &"<redacted>")
            .field("user_id", &self.user_id)
            .field("created_at", &self.created_at)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEventInput {
    pub actor_user_id: i64,
    pub action: String,
    pub target_kind: String,
    pub target_id: Option<i64>,
    pub at: i64,
    pub client_ip: Option<String>,
    pub details: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEventRecord {
    pub id: i64,
    pub event: AuditEventInput,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserKeySecretRecord {
    pub id: i64,
    pub envelope: Option<CredentialEnvelope>,
}

impl std::fmt::Debug for UserKeySecretRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UserKeySecretRecord")
            .field("id", &self.id)
            .field("envelope", &self.envelope.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}
