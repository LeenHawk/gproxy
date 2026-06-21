//! Instance settings record (§8). Per-instance runtime configuration:
//! identity, outbound proxy, TLS-emulation toggle, logging and usage flags.

use serde::{Deserialize, Serialize};

/// Per-instance settings, keyed by a unique `instance_name`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceSettings {
    pub id: i64,
    /// Unique instance name.
    pub instance_name: String,
    /// Default outbound proxy URL for this instance; `None` = direct.
    pub proxy: Option<String>,
    /// Whether TLS-emulation (fingerprint spoofing) is enabled; `None` =
    /// inherit / unset.
    pub spoof_emulation: Option<bool>,
    /// Record usage / accounting.
    pub enable_usage: bool,
    /// Log upstream (provider) requests/responses.
    pub enable_upstream_log: bool,
    /// Include upstream request/response bodies in logs.
    pub enable_upstream_log_body: bool,
    /// Log downstream (client) requests/responses.
    pub enable_downstream_log: bool,
    /// Include downstream request/response bodies in logs.
    pub enable_downstream_log_body: bool,
    /// Disable secret/PII redaction in logs.
    pub disable_log_redaction: bool,
    /// Allow the tokenizer registry to download missing vocabs from HF.
    #[serde(default)]
    pub enable_tokenizer_download: bool,
    /// Update channel, e.g. `stable` | `beta`; `None` = default.
    pub update_channel: Option<String>,
    /// Purge usage/request-log rows older than this many days (§8-D). `None`
    /// or `<= 0` = retain forever (the historical behaviour).
    #[serde(default)]
    pub retention_days: Option<i64>,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds.
    pub updated_at: i64,
}

/// Upsert input for instance settings. `id = None` inserts; `Some(id)` updates.
/// `created_at` / `updated_at` are managed by the backend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceSettingsInput {
    pub id: Option<i64>,
    pub instance_name: String,
    pub proxy: Option<String>,
    pub spoof_emulation: Option<bool>,
    pub enable_usage: bool,
    pub enable_upstream_log: bool,
    pub enable_upstream_log_body: bool,
    pub enable_downstream_log: bool,
    pub enable_downstream_log_body: bool,
    pub disable_log_redaction: bool,
    /// Allow the tokenizer registry to download missing vocabs from HF.
    #[serde(default)]
    pub enable_tokenizer_download: bool,
    pub update_channel: Option<String>,
    #[serde(default)]
    pub retention_days: Option<i64>,
}
