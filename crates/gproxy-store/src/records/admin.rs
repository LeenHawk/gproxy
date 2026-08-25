use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::CredentialEnvelope;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminAccountRecord {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub enabled: bool,
    pub created_at: i64,
}

impl std::fmt::Debug for AdminAccountRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AdminAccountRecord")
            .field("id", &self.id)
            .field("username", &self.username)
            .field("password_hash", &"<redacted>")
            .field("enabled", &self.enabled)
            .field("created_at", &self.created_at)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminSessionInput {
    pub token_digest: Vec<u8>,
    pub admin_id: i64,
    pub created_at: i64,
    pub expires_at: i64,
}

impl std::fmt::Debug for AdminSessionInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AdminSessionInput")
            .field("token_digest", &"<redacted>")
            .field("admin_id", &self.admin_id)
            .field("created_at", &self.created_at)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEventInput {
    pub actor_admin_id: i64,
    pub action: String,
    pub target_kind: String,
    pub target_id: Option<i64>,
    pub at: i64,
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
