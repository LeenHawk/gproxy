//! Providers, credentials and the model catalog: the rows core assembles a
//! provider from.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::double_option;
use gproxy_store::entity::upstream::{credential, model, provider, provider_model};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ProviderDto {
    pub id: String,
    /// Invocation URL prefix.
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    /// The channel implementation this provider speaks through; must be one
    /// of `Gproxy::channels()`.
    pub channel: String,
    pub base_url: Option<String>,
    pub connection_profile_id: Option<String>,
    #[serde(default)]
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ mode: 'direct' } | { mode: 'system' } | { mode: 'explicit', url: string } | null"
        )
    )]
    pub proxy: Option<Value>,
    /// Channel-specific configuration; always a JSON object.
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub config: Value,
    pub enabled: bool,
    pub created_at_ms: i64,
}

impl From<provider::Model> for ProviderDto {
    fn from(row: provider::Model) -> Self {
        Self {
            id: row.id,
            name: row.name,
            display_name: row.display_name,
            channel: row.channel,
            base_url: row.base_url,
            connection_profile_id: row.connection_profile_id,
            proxy: row.proxy,
            config: row.config,
            enabled: row.enabled,
            created_at_ms: row.created_at_ms,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ProviderWrite {
    /// Supplied ids let a host import its own; blank or absent mints one.
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    pub channel: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub connection_profile_id: Option<String>,
    #[serde(default)]
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ mode: 'direct' } | { mode: 'system' } | { mode: 'explicit', url: string } | null"
        )
    )]
    pub proxy: Option<Value>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "unknown | null"))]
    pub config: Option<Value>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ProviderPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub display_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub base_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub connection_profile_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ mode: 'direct' } | { mode: 'system' } | { mode: 'explicit', url: string } | null"
        )
    )]
    pub proxy: Option<Option<Value>>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "unknown | null"))]
    pub config: Option<Value>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// A credential without its secret. `has_secret` is the only thing said about
/// the sealed bytes; reading them back is `Credentials::reveal_secret`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct CredentialDto {
    pub id: String,
    pub provider_id: String,
    /// Opaque to this crate: the application layer decides what owns what.
    pub organization_id: Option<String>,
    pub team_id: Option<String>,
    pub user_id: Option<String>,
    pub label: Option<String>,
    pub auth_kind: String,
    pub has_secret: bool,
    /// Bumped by every secret or lifecycle write; peers reload on a change.
    pub version: i64,
    pub connection_profile_id: Option<String>,
    #[serde(default)]
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ mode: 'direct' } | { mode: 'system' } | { mode: 'explicit', url: string } | null"
        )
    )]
    pub proxy: Option<Value>,
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub metadata: Value,
    pub expires_at_ms: Option<i64>,
    /// `active` or `dead`.
    pub status: String,
    pub status_reason: Option<String>,
    pub enabled: bool,
}

impl From<credential::Model> for CredentialDto {
    fn from(row: credential::Model) -> Self {
        Self {
            id: row.id,
            provider_id: row.provider_id,
            organization_id: row.organization_id,
            team_id: row.team_id,
            user_id: row.user_id,
            label: row.label,
            auth_kind: row.auth_kind,
            has_secret: !row.secret.is_empty(),
            version: row.version,
            connection_profile_id: row.connection_profile_id,
            proxy: row.proxy,
            metadata: row.metadata,
            expires_at_ms: row.expires_at_ms,
            status: status_name(row.status).to_owned(),
            status_reason: row.status_reason,
            enabled: row.enabled,
        }
    }
}

pub(crate) fn status_name(status: credential::CredentialStatus) -> &'static str {
    match status {
        credential::CredentialStatus::Active => "active",
        credential::CredentialStatus::Dead => "dead",
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct CredentialWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub provider_id: String,
    #[serde(default)]
    pub label: Option<String>,
    /// How the channel injects it: `api_key`, `oauth`, `cookie`, …
    pub auth_kind: String,
    /// Plaintext on the way in, sealed before it reaches the database.
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub secret: Value,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "unknown | null"))]
    pub metadata: Option<Value>,
    #[serde(default)]
    pub connection_profile_id: Option<String>,
    #[serde(default)]
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ mode: 'direct' } | { mode: 'system' } | { mode: 'explicit', url: string } | null"
        )
    )]
    pub proxy: Option<Value>,
    #[serde(default)]
    pub expires_at_ms: Option<i64>,
    #[serde(default)]
    pub organization_id: Option<String>,
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
}

/// Lifecycle status is not here: it is versioned against concurrent refreshes
/// and changes through `Credentials::set_status`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct CredentialPatch {
    #[serde(default, deserialize_with = "double_option")]
    pub label: Option<Option<String>>,

    /// Resealed and version-bumped; peers then reload this credential alone.
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "unknown | null"))]
    pub secret: Option<Value>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "unknown | null"))]
    pub metadata: Option<Value>,
    #[serde(default, deserialize_with = "double_option")]
    pub connection_profile_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ mode: 'direct' } | { mode: 'system' } | { mode: 'explicit', url: string } | null"
        )
    )]
    pub proxy: Option<Option<Value>>,
    #[serde(default, deserialize_with = "double_option")]
    pub expires_at_ms: Option<Option<i64>>,
    #[serde(default, deserialize_with = "double_option")]
    pub organization_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub team_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub user_id: Option<Option<String>>,
}

impl CredentialPatch {
    /// Whether this patch only touches material `CredentialData` re-reads on
    /// its own. Everything else — label, metadata, the connection
    /// profile, ownership — is frozen into the snapshot at assembly, so
    /// changing it needs a full reload rather than a credential re-read.
    pub(crate) fn state_only(&self) -> bool {
        self.label.is_none()
            && self.enabled.is_none()
            && self.metadata.is_none()
            && self.connection_profile_id.is_none()
            && self.proxy.is_none()
            && self.organization_id.is_none()
            && self.team_id.is_none()
            && self.user_id.is_none()
    }
}

/// What a refresh reports back: never secret material, only its identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct CredentialSummaryDto {
    pub credential_id: String,
    pub version: i64,
    pub expires_at_ms: Option<i64>,
    pub status: String,
    pub status_reason: Option<String>,
}

impl From<gproxy_core::CredentialSummary> for CredentialSummaryDto {
    fn from(summary: gproxy_core::CredentialSummary) -> Self {
        Self {
            credential_id: summary.credential_id,
            version: summary.version,
            expires_at_ms: summary.expires_at_ms,
            status: status_name(summary.status).to_owned(),
            status_reason: summary.status_reason,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ModelDto {
    pub id: String,
    pub name: String,
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub metadata: Value,
    pub vocabulary_file_id: Option<String>,
}

impl From<model::Model> for ModelDto {
    fn from(row: model::Model) -> Self {
        Self {
            id: row.id,
            name: row.name,
            metadata: row.metadata,
            vocabulary_file_id: row.vocabulary_file_id,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ModelWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "unknown | null"))]
    pub metadata: Option<Value>,
    #[serde(default)]
    pub vocabulary_file_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ModelPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "unknown | null"))]
    pub metadata: Option<Value>,
    #[serde(default, deserialize_with = "double_option")]
    pub vocabulary_file_id: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ProviderModelDto {
    pub id: String,
    pub provider_id: String,
    /// The name this provider's upstream answers to, not the public one.
    pub upstream_name: String,
    pub model_id: Option<String>,
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub metadata: Value,
    pub enabled: bool,
    /// Exact enabled provider price rule exists; populated by list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub has_price: Option<bool>,
}

impl From<provider_model::Model> for ProviderModelDto {
    fn from(row: provider_model::Model) -> Self {
        Self {
            id: row.id,
            provider_id: row.provider_id,
            upstream_name: row.upstream_name,
            model_id: row.model_id,
            metadata: row.metadata,
            enabled: row.enabled,
            has_price: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ProviderModelWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub provider_id: String,
    pub upstream_name: String,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "unknown | null"))]
    pub metadata: Option<Value>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct ProviderModelPatch {
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub upstream_name: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub model_id: Option<Option<String>>,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "unknown | null"))]
    pub metadata: Option<Value>,
    #[serde(default)]
    pub enabled: Option<bool>,
}
