use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AuditEventDto {
    pub id: i64,
    pub actor_user_id: i64,
    pub action: String,
    pub target_kind: String,
    pub target_id: Option<i64>,
    pub at: i64,
    pub client_ip: Option<String>,
    #[ts(type = "unknown | null")]
    pub details: Option<Value>,
}
