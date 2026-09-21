//! Console and portal sessions, as an operator or the session's owner sees
//! them. There is no token field and no digest field: a session can be listed
//! and ended, never read back.

use gproxy_store::entity::identity::user_session;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct UserSessionDto {
    pub id: String,
    pub user_id: String,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
}

impl From<user_session::Model> for UserSessionDto {
    fn from(row: user_session::Model) -> Self {
        // `token_hash` is dropped here, deliberately and permanently. It is
        // not a secret by itself, but publishing it turns a read-only console
        // bug into an offline guessing target against a 32-byte token.
        Self {
            id: row.id,
            user_id: row.user_id,
            created_at_ms: row.created_at_ms,
            expires_at_ms: row.expires_at_ms,
        }
    }
}
