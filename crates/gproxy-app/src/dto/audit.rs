//! The audit trail as it is read back.

use gproxy_store::entity::identity::audit_event;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct AuditEventDto {
    pub id: String,
    pub actor_user_id: Option<String>,
    pub actor_api_key_id: Option<String>,
    pub source_ip: Option<String>,
    /// The operation's name, e.g. `users.create`.
    pub action: String,
    pub entity_kind: Option<String>,
    pub entity_id: Option<String>,
    /// `ok` or `error`. A rejected operation is audited too.
    pub outcome: String,
    /// The redacted request summary the operation wrote. Never a secret: see
    /// [`AuditEntry::redacted`](crate::audit::AuditEntry::redacted).
    pub detail: serde_json::Value,
    pub created_at_ms: i64,
}

impl From<audit_event::Model> for AuditEventDto {
    fn from(row: audit_event::Model) -> Self {
        Self {
            id: row.id,
            actor_user_id: row.actor_user_id,
            actor_api_key_id: row.actor_api_key_id,
            source_ip: row.source_ip,
            action: row.action,
            entity_kind: row.entity_kind,
            entity_id: row.entity_id,
            outcome: row.outcome,
            detail: row.detail,
            created_at_ms: row.created_at_ms,
        }
    }
}

/// How the trail is paged and narrowed. Separate from
/// [`ListQuery`](super::ListQuery) because the trail's useful filters are the
/// actor, the action and a time range, none of which any configuration family
/// has.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct AuditQuery {
    /// 1-based; zero and absent both mean the first page.
    pub page: Option<u64>,
    /// Clamped to 1..=500; absent means 50.
    pub page_size: Option<u64>,
    pub actor_user_id: Option<String>,
    pub actor_api_key_id: Option<String>,
    /// Case-insensitive substring, so `users.` selects a whole family.
    pub action: Option<String>,
    pub entity_kind: Option<String>,
    pub entity_id: Option<String>,
    pub outcome: Option<String>,
    /// Inclusive lower bound on `createdAtMs`.
    pub since_ms: Option<i64>,
    /// Exclusive upper bound on `createdAtMs`.
    pub until_ms: Option<i64>,
}

impl AuditQuery {
    pub fn bounds(&self) -> (u64, u64) {
        let limit = self.page_size.unwrap_or(50).clamp(1, 500);
        let page = self.page.unwrap_or(1).max(1);
        (page.saturating_sub(1).saturating_mul(limit), limit)
    }
}
