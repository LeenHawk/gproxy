//! Gateway users. The password is write-only: a user reports whether it has
//! one, never what it is.

use super::{allowlist, double_option};
use gproxy_store::entity::identity::user;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDto {
    pub id: String,
    pub name: String,
    /// Instance-wide role, `admin` or `user`. Organization and team roles are
    /// memberships and are not this.
    pub role: String,
    pub enabled: bool,
    /// Whether a console/portal password is set. `password_hash` itself has no
    /// field here and no accessor anywhere.
    pub has_password: bool,
    /// None inherits the levels above; an empty list denies every client for
    /// this user.
    pub oauth_client_allowlist: Option<Vec<String>>,
    pub created_at_ms: i64,
}

impl From<user::Model> for UserDto {
    fn from(row: user::Model) -> Self {
        Self {
            id: row.id,
            name: row.name,
            role: row.role,
            enabled: row.enabled,
            has_password: row.password_hash.is_some(),
            oauth_client_allowlist: allowlist(row.oauth_client_allowlist),
            created_at_ms: row.created_at_ms,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    /// Optional: a user created without one cannot sign in until a password is
    /// set, which is what an API-key-only account is.
    #[serde(default)]
    pub password: Option<String>,
    /// `admin` or `user`; absent means `user`.
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub oauth_client_allowlist: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Absent leaves the allowlist alone, `null` clears it back to inheriting,
    /// a list replaces it. The password is deliberately not patchable here:
    /// changing one revokes sessions, so it is its own operation.
    #[serde(default, deserialize_with = "double_option")]
    pub oauth_client_allowlist: Option<Option<Vec<String>>>,
}

/// The body of `users.set_password`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PasswordWrite {
    pub password: String,
}
