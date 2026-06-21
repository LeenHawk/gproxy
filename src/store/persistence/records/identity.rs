//! Identity records (§8-C): orgs, teams, users, and user keys.

use serde::{Deserialize, Serialize};

/// An organization — the top of the identity hierarchy (§8-C).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Org {
    pub id: i64,
    /// Unique org name.
    pub name: String,
    pub enabled: bool,
    pub description: Option<String>,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds.
    pub updated_at: i64,
}

/// Upsert input for an org. `id = None` inserts; `Some(id)` updates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrgInput {
    pub id: Option<i64>,
    pub name: String,
    pub enabled: bool,
    pub description: Option<String>,
}

/// A team within an org (§8-C). Unique per `(org_id, name)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Team {
    pub id: i64,
    pub org_id: i64,
    pub name: String,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Upsert input for a team.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamInput {
    pub id: Option<i64>,
    pub org_id: i64,
    pub name: String,
    pub enabled: bool,
}

/// A user belonging to an org and (optionally) a team (§8-C).
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    /// Unique user name.
    pub name: String,
    pub org_id: i64,
    pub team_id: Option<i64>,
    pub password: Option<String>,
    pub enabled: bool,
    pub is_admin: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Redacts the password hash from debug output (presence only), so a stray
/// `{:?}` can never leak the stored hash into logs.
impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("org_id", &self.org_id)
            .field("team_id", &self.team_id)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("enabled", &self.enabled)
            .field("is_admin", &self.is_admin)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// Upsert input for a user.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct UserInput {
    pub id: Option<i64>,
    pub name: String,
    pub org_id: i64,
    pub team_id: Option<i64>,
    pub password: Option<String>,
    pub enabled: bool,
    pub is_admin: bool,
}

/// Redacts the (plaintext) new password from debug output — even more
/// sensitive than the stored hash, so never serialize it via `{:?}`.
impl std::fmt::Debug for UserInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserInput")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("org_id", &self.org_id)
            .field("team_id", &self.team_id)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("enabled", &self.enabled)
            .field("is_admin", &self.is_admin)
            .finish()
    }
}

/// An API key issued to a user (§8-C). `api_key_digest` is unique.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct UserKey {
    pub id: i64,
    pub user_id: i64,
    pub api_key_ciphertext: String,
    pub api_key_digest: String,
    pub label: Option<String>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Redacts the API-key ciphertext from debug output.
impl std::fmt::Debug for UserKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserKey")
            .field("id", &self.id)
            .field("user_id", &self.user_id)
            .field("api_key_ciphertext", &"<redacted>")
            .field("api_key_digest", &self.api_key_digest)
            .field("label", &self.label)
            .field("enabled", &self.enabled)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// Upsert input for a user key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserKeyInput {
    pub id: Option<i64>,
    pub user_id: i64,
    pub api_key_ciphertext: String,
    pub api_key_digest: String,
    pub label: Option<String>,
    pub enabled: bool,
}
