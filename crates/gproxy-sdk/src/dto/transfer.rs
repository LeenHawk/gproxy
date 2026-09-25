//! Configuration transfer: one instance's rows as a document another instance
//! can read back.
//!
//! The document is the same DTOs the management families already exchange, so
//! an export is exactly what a console would have listed — one row per row, no
//! second serialization of the schema. What the families do not expose is not
//! in here either: identity, usage, captures and every runtime table stay
//! behind.
//!
//! A credential is the one exception to "a DTO never carries a secret". The
//! sealed blob travels **as it is stored**, never opened and never plaintext,
//! so an export is exactly as sensitive as the database file it came from and
//! no more. An importer that does not hold the source key cannot read it
//! either, which is why [`ImportRequest::source_master_key`] exists.

use serde::{Deserialize, Serialize};

use super::{
    ConnectionProfileDto, CredentialDto, ModelDto, OperationEndpointDto, OperationRuleDto,
    PriceRateDto, PriceRuleDto, PriceTierDto, ProviderDto, ProviderModelDto, ProviderRuleSetDto,
    QuotaDto, RewriteRuleDto, RouteDto, RouteMemberDto, RuleSetDto, SettingsDto,
};

/// The only version this build writes, and the only one it accepts. It is not
/// the schema version: it names the shape of the envelope below.
pub const EXPORT_FORMAT_VERSION: u32 = 5;

/// Codec names as they appear in [`SealedSecretDto::codec`], read from the
/// envelope byte the sealed blob starts with rather than from configuration:
/// what a row actually is matters more than what this instance is set to.
pub const CODEC_PLAINTEXT: &str = "plaintext";
pub const CODEC_AES_GCM: &str = "aes-gcm";
pub const CODEC_UNKNOWN: &str = "unknown";

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ExportRequest {
    /// Whether the sealed credential blobs travel with the configuration. An
    /// export without them still restores every row; the credentials simply
    /// arrive unable to authenticate until someone logs in again.
    #[serde(default)]
    pub include_secrets: bool,
}

/// One instance's configuration as a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ConfigurationExportDto {
    pub format_version: u32,
    pub exported_at_ms: i64,
    /// True when the export was taken without secrets: every credential's
    /// `secret` is null.
    pub secrets_omitted: bool,
    /// The sealing codecs the exported blobs actually use, sorted and without
    /// duplicates; empty when the secrets were omitted. An importer reads it
    /// before it starts, to decide whether it needs a source master key at all.
    pub secrets: Vec<String>,
    pub data: ConfigurationDataDto,
}

/// Every exported table, in the order a fresh instance may replay them: a row
/// never appears before the row it points at. Each field defaults to empty, so
/// a document written by a build that knew fewer tables still imports.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ConfigurationDataDto {
    #[serde(default)]
    pub connection_profiles: Vec<ConnectionProfileDto>,
    #[serde(default)]
    pub providers: Vec<ProviderDto>,
    #[serde(default)]
    pub credentials: Vec<ExportCredentialDto>,
    #[serde(default)]
    pub models: Vec<ModelDto>,
    #[serde(default)]
    pub provider_models: Vec<ProviderModelDto>,
    #[serde(default)]
    pub routes: Vec<RouteDto>,
    #[serde(default)]
    pub route_members: Vec<RouteMemberDto>,
    #[serde(default)]
    pub operation_rules: Vec<OperationRuleDto>,
    #[serde(default)]
    pub operation_endpoints: Vec<OperationEndpointDto>,
    #[serde(default)]
    pub rewrite_rule_sets: Vec<RuleSetDto>,
    #[serde(default)]
    pub rewrite_rules: Vec<RewriteRuleDto>,
    #[serde(default)]
    pub provider_rewrite_rule_sets: Vec<ProviderRuleSetDto>,
    #[serde(default)]
    pub quotas: Vec<QuotaDto>,
    #[serde(default)]
    pub price_rules: Vec<PriceRuleDto>,
    #[serde(default)]
    pub price_rates: Vec<PriceRateDto>,
    #[serde(default)]
    pub price_tiers: Vec<PriceTierDto>,
    /// The single settings row. `config_revision` travels but is never
    /// imported: the destination's revision is its own counter.
    #[serde(default)]
    pub settings: Option<SettingsDto>,
}

/// A credential row plus the sealed bytes behind it. The columns are flattened,
/// so the object is a [`CredentialDto`] with one field added.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ExportCredentialDto {
    #[serde(flatten)]
    pub credential: CredentialDto,
    /// The database column verbatim; null when the export omitted secrets.
    #[serde(default)]
    pub secret: Option<SealedSecretDto>,
}

/// A sealed secret as it travels: the envelope's own codec, and its bytes in
/// standard base64. Nothing here is plaintext, and nothing here can be opened
/// without the key the source sealed it with.
#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct SealedSecretDto {
    /// `plaintext`, `aes-gcm`, or `unknown` for an envelope this build does
    /// not recognize. Carried so an importer can refuse early rather than
    /// after decrypting nothing.
    pub codec: String,
    pub bytes: String,
}

// Sealed bytes are not plaintext, but a debug log is the wrong place for them.
impl std::fmt::Debug for SealedSecretDto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealedSecretDto")
            .field("codec", &self.codec)
            .field("bytes", &"<sealed>")
            .finish()
    }
}

/// What an import does with a row the destination already has under that id.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", ts(rename_all = "snake_case"))]
pub enum ImportMode {
    /// Write every row of the document, leaving anything it does not mention
    /// alone. This is the additive mode: two deployments can be merged.
    #[default]
    Merge,
    /// The same, and then delete every row of an exported kind that the
    /// document does not mention. The destination becomes the document.
    /// Identity, usage and capture tables are never touched by it.
    Replace,
}

#[derive(Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ImportRequest {
    pub export: ConfigurationExportDto,
    #[serde(default)]
    pub mode: ImportMode,
    /// The 32-byte master key the source sealed its credentials with, in
    /// standard base64. With it every secret is opened and resealed under this
    /// instance's codec; without it the blobs are stored verbatim, which only
    /// works if both instances share a key.
    #[serde(default)]
    pub source_master_key: Option<String>,
}

impl std::fmt::Debug for ImportRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImportRequest")
            .field("mode", &self.mode)
            .field("export", &"<configuration document>")
            .field("source_master_key", &"<redacted>")
            .finish()
    }
}

/// What one import did. The counters are rows, across every kind; the warnings
/// are the things that were silently changed rather than refused, which is the
/// only place a caller learns that a credential arrived unusable.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ImportReportDto {
    pub created: u64,
    pub updated: u64,
    /// Rows the document named that were not written, which today is exactly
    /// the credentials whose secret could not be opened. What `Replace`
    /// deleted is reported as a warning instead: a removal is not a skip.
    pub skipped: u64,
    pub credentials_resealed: u64,
    pub credentials_skipped: u64,
    pub warnings: Vec<String>,
}
