//! Credential DTOs that keep the sealed secret off the wire.
//!
//! [`CredentialView`] redacts `secret_json` on reads (replacing it with a
//! `has_secret` flag); [`CredentialUpsert`] takes a PLAINTEXT `secret_json` on
//! writes, which the handler seals (or, when omitted on an update, leaves the
//! stored ciphertext untouched).

use serde_json::Value;

use crate::store::persistence::records::Credential;

/// Read-side credential shape: every pool field plus a `has_secret` flag. The
/// sealed `secret_json` is never serialized.
#[derive(serde::Serialize)]
pub struct CredentialView {
    pub id: i64,
    pub provider_id: i64,
    /// Human label — stored in the `name` column.
    pub label: Option<String>,
    pub kind: String,
    pub weight: i64,
    pub rpm_limit: Option<i64>,
    pub tpm_limit: Option<i64>,
    pub proxy_url: Option<String>,
    pub tls_fingerprint: Option<Value>,
    pub enabled: bool,
    /// True when a non-empty secret is stored (never the secret itself).
    pub has_secret: bool,
}

impl From<Credential> for CredentialView {
    fn from(c: Credential) -> Self {
        let has_secret = !c.secret_json.is_null()
            && match &c.secret_json {
                Value::Object(m) => !m.is_empty(),
                Value::String(s) => !s.is_empty(),
                _ => true,
            };
        CredentialView {
            id: c.id,
            provider_id: c.provider_id,
            label: c.name,
            kind: c.kind,
            weight: c.weight,
            rpm_limit: c.rpm_limit,
            tpm_limit: c.tpm_limit,
            proxy_url: c.proxy_url,
            tls_fingerprint: c.tls_fingerprint,
            enabled: c.enabled,
            has_secret,
        }
    }
}

fn default_kind() -> String {
    "api_key".to_string()
}

fn default_weight() -> i64 {
    100
}

fn default_true() -> bool {
    true
}

/// Batch import request: create-only items (an `id` in any item is rejected
/// per-item). Each item is sealed and deduplicated like a single upsert.
#[derive(serde::Deserialize)]
pub struct CredentialImportRequest {
    pub items: Vec<CredentialUpsert>,
}

/// Per-item import outcome. `status` is `created` | `existing` | `error`.
#[derive(serde::Serialize)]
pub struct CredentialImportItem {
    pub index: usize,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Whole-batch import summary plus per-item results (same order as request).
#[derive(serde::Serialize)]
pub struct CredentialImportOutcome {
    pub created: usize,
    pub existing: usize,
    pub failed: usize,
    pub results: Vec<CredentialImportItem>,
}

/// Write-side credential shape. `secret_json` is PLAINTEXT (sealed by the
/// handler): required on create, optional on update (omit to keep the stored
/// sealed value). `id = None` creates, `Some(id)` updates.
#[derive(serde::Deserialize)]
pub struct CredentialUpsert {
    #[serde(default)]
    pub id: Option<i64>,
    /// Provider id — taken from the path; ignored if present in the body.
    #[serde(default)]
    pub provider_id: i64,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default = "default_kind")]
    pub kind: String,
    /// Plaintext secret. `Some` → sealed; `None` on update → keep existing;
    /// `None` on create → rejected (400).
    #[serde(default)]
    pub secret_json: Option<Value>,
    #[serde(default = "default_weight")]
    pub weight: i64,
    #[serde(default)]
    pub rpm_limit: Option<i64>,
    #[serde(default)]
    pub tpm_limit: Option<i64>,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub tls_fingerprint: Option<Value>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}
