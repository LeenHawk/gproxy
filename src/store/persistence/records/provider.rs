//! Provider record (§8-B). A `provider` is one upstream endpoint family
//! (channel) with its settings and credential strategy. Also carries the
//! provider's exposed models (§8-A).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A persisted upstream provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provider {
    pub id: i64,
    /// Unique provider name.
    pub name: String,
    /// Channel / adapter family — must equal a `Channel::id` in the
    /// `ChannelRegistry`, e.g. `openai`, `claudeapi`, `aistudio`, `custom`.
    pub channel: String,
    pub label: Option<String>,
    /// Free-form settings: `base_url`, channel scalar toggles, `circuit_breaker`,
    /// etc. (see §7.4 / §3.2).
    pub settings_json: Value,
    /// Credential pool strategy: `round_robin` | `sticky` (§3.3).
    pub credential_strategy: String,
    /// Provider-level default outbound proxy; `credential.proxy_url` overrides,
    /// the global default is the final fallback (§7.4). Edge ignores.
    pub proxy_url: Option<String>,
    /// Provider-level default TLS-emulation fingerprint (JSON); overridden by
    /// `credential.tls_fingerprint` (§7.4). `None` = no emulation.
    #[serde(default)]
    pub tls_fingerprint: Option<Value>,
    pub enabled: bool,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds.
    pub updated_at: i64,
}

/// Upsert input for a provider. `id = None` inserts; `Some(id)` updates.
/// `created_at` / `updated_at` are managed by the backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderInput {
    pub id: Option<i64>,
    pub name: String,
    pub channel: String,
    pub label: Option<String>,
    pub settings_json: Value,
    pub credential_strategy: String,
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub tls_fingerprint: Option<Value>,
    pub enabled: bool,
}

/// A persisted upstream credential (one key in a provider's pool, §8-B).
///
/// `secret_json` is the **opaque envelope-encrypted** secret (§14.1); the
/// persistence layer stores it verbatim — encryption/decryption is domain code.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Credential {
    pub id: i64,
    pub provider_id: i64,
    pub name: Option<String>,
    pub kind: String,
    pub secret_json: Value,
    pub weight: i64,
    pub rpm_limit: Option<i64>,
    pub tpm_limit: Option<i64>,
    /// Per-credential outbound proxy override; falls back to the global proxy
    /// (§7.4). Edge ignores.
    pub proxy_url: Option<String>,
    /// Per-credential TLS-emulation fingerprint override; falls back to the
    /// channel's default (§7.4). `None` + channel default `None` = no emulation.
    #[serde(default)]
    pub tls_fingerprint: Option<Value>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Redacts `secret_json` so a credential can never leak its plaintext secret
/// into a debug log.
impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credential")
            .field("id", &self.id)
            .field("provider_id", &self.provider_id)
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("secret_json", &"<redacted>")
            .field("weight", &self.weight)
            .field("rpm_limit", &self.rpm_limit)
            .field("tpm_limit", &self.tpm_limit)
            .field("proxy_url", &self.proxy_url)
            .field("tls_fingerprint", &self.tls_fingerprint)
            .field("enabled", &self.enabled)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

/// Upsert input for a credential.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialInput {
    pub id: Option<i64>,
    pub provider_id: i64,
    pub name: Option<String>,
    pub kind: String,
    pub secret_json: Value,
    pub weight: i64,
    pub rpm_limit: Option<i64>,
    pub tpm_limit: Option<i64>,
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub tls_fingerprint: Option<Value>,
    pub enabled: bool,
}

/// Audit snapshot of a credential's health for one channel (§8-B).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialStatus {
    pub id: i64,
    pub credential_id: i64,
    pub channel: String,
    pub health_kind: String,
    pub health_json: Option<Value>,
    pub checked_at: Option<i64>,
    pub last_error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Upsert input for a credential status (unique per `(credential_id, channel)`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialStatusInput {
    pub id: Option<i64>,
    pub credential_id: i64,
    pub channel: String,
    pub health_kind: String,
    pub health_json: Option<Value>,
    pub checked_at: Option<i64>,
    pub last_error: Option<String>,
}

/// A model exposed by a provider, with optional pricing (§8-A).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderModel {
    pub id: i64,
    pub provider_id: i64,
    pub model_id: String,
    pub display_name: Option<String>,
    pub pricing_json: Option<Value>,
    /// Suffix-variant config (§8-B): `["-x"]` or `{expose_base, suffixes}`.
    #[serde(default)]
    pub variants_json: Option<Value>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Upsert input for a provider model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderModelInput {
    pub id: Option<i64>,
    pub provider_id: i64,
    pub model_id: String,
    pub display_name: Option<String>,
    pub pricing_json: Option<Value>,
    /// Suffix-variant config (§8-B): `["-x"]` or `{expose_base, suffixes}`.
    #[serde(default)]
    pub variants_json: Option<Value>,
    pub enabled: bool,
}
