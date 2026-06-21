//! User-key DTOs that keep key material off the wire.
//!
//! [`UserKeyView`] exposes only a short `key_prefix` (first 8 chars of the
//! digest) on reads; writes never carry key material — the create handler
//! GENERATES the key server-side (returned once via `UserKeyView.api_key`)
//! and updates touch only label/enabled.

use crate::store::persistence::records::UserKey;

/// Read-side user-key shape: identity fields plus a short non-reversible
/// `key_prefix`. The bare key rides ONLY the create response (`api_key`,
/// shown once — the server just generated it); it is never serialized on any
/// read path, and the ciphertext never leaves the store.
#[derive(serde::Serialize)]
pub struct UserKeyView {
    pub id: i64,
    pub user_id: i64,
    pub label: Option<String>,
    pub enabled: bool,
    /// First 8 chars of the digest — never the bare key.
    pub key_prefix: String,
    /// The freshly-generated bare key, present ONLY on the create response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
}

impl From<UserKey> for UserKeyView {
    fn from(k: UserKey) -> Self {
        let key_prefix: String = k.api_key_digest.chars().take(8).collect();
        UserKeyView {
            id: k.id,
            user_id: k.user_id,
            label: k.label,
            enabled: k.enabled,
            key_prefix,
            api_key: None,
        }
    }
}

fn default_true() -> bool {
    true
}

/// Write-side user-key shape. `id = None` creates (the key material is
/// GENERATED server-side and returned once), `Some(id)` updates label/enabled
/// (key material is immutable — rotate by create + delete). `api_key` is kept
/// in the shape only so a caller supplying one gets an explicit 400 instead of
/// a silent ignore; external key material enters via the import path alone.
#[derive(serde::Deserialize)]
pub struct UserKeyUpsert {
    #[serde(default)]
    pub id: Option<i64>,
    /// User id — taken from the path; ignored if present in the body.
    #[serde(default)]
    pub user_id: i64,
    /// Rejected if present (400) — keys are server-generated.
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}
