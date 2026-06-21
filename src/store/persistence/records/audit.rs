//! Admin audit log record (§ admin hardening): an append-only trail of who did
//! what mutating admin action, when, against which target, with what outcome.
//!
//! Carries NOTHING sensitive — only actor identity, the request method/path,
//! the response status, and the source IP. Passwords, secrets, keys, and
//! session tokens are never recorded.

use serde::{Deserialize, Serialize};

/// One audit row: a single mutating admin action (or a login attempt).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: i64,
    /// Unix seconds the action occurred.
    pub at: i64,
    /// Acting admin's user id, when known (`None` for failed logins).
    pub actor_id: Option<i64>,
    /// Acting admin's name, when known.
    pub actor_name: Option<String>,
    /// What happened: an HTTP method ("POST"/"DELETE"/…) for mutating requests,
    /// or "login.success" / "login.fail" for auth attempts.
    pub action: String,
    /// What it acted on: the request path ("/admin/credentials/5") for requests,
    /// or the attempted username for logins.
    pub target: String,
    /// Outcome: the HTTP response status (or a synthetic code for logins).
    pub status: i64,
    /// Source IP from a reverse-proxy header, when present.
    pub source_ip: Option<String>,
    /// Unix seconds the row was written.
    pub created_at: i64,
}

/// Append input for an audit row (append-only; no id/created_at).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditLogInput {
    pub actor_id: Option<i64>,
    pub actor_name: Option<String>,
    pub action: String,
    pub target: String,
    pub status: i64,
    pub source_ip: Option<String>,
}
