//! Admin user DTOs that keep the password hash off the wire.
//!
//! [`UserView`] redacts the stored hash on reads (replacing it with a
//! `has_password` flag); [`UserUpsert`] takes a PLAINTEXT password on writes,
//! which the handler hashes (or, when omitted on an update, leaves untouched).

use crate::store::persistence::records::User;

/// Read-side user shape: every [`User`] field except `password`, plus a
/// `has_password` boolean. The hash is never serialized.
#[derive(serde::Serialize)]
pub struct UserView {
    pub id: i64,
    pub name: String,
    pub org_id: i64,
    pub team_id: Option<i64>,
    pub has_password: bool,
    pub enabled: bool,
    pub is_admin: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<User> for UserView {
    fn from(u: User) -> Self {
        UserView {
            id: u.id,
            name: u.name,
            org_id: u.org_id,
            team_id: u.team_id,
            has_password: u.password.is_some(),
            enabled: u.enabled,
            is_admin: u.is_admin,
            created_at: u.created_at,
            updated_at: u.updated_at,
        }
    }
}

/// Write-side user shape. Identical to `UserInput` except `password` is the
/// PLAINTEXT to hash (or `None` to keep/omit). `id = None` creates,
/// `Some(id)` updates.
#[derive(serde::Deserialize)]
pub struct UserUpsert {
    #[serde(default)]
    pub id: Option<i64>,
    pub name: String,
    pub org_id: i64,
    #[serde(default)]
    pub team_id: Option<i64>,
    /// Plaintext password. `Some` → hashed; `None` on update → keep existing;
    /// `None` on create → no password (no login until set).
    #[serde(default)]
    pub password: Option<String>,
    pub enabled: bool,
    pub is_admin: bool,
}
