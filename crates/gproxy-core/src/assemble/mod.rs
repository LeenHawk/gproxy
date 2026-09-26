//! ControlData rows into one CoreData snapshot: profile resolution, channel
//! lookup, secret opening, rule compilation, quota dimension declaration.
//! Any invalid row fails the whole assembly so the previous snapshot stays.

use crate::CredentialStrategy;
use crate::{
    BlockSource, CoreData, CredentialBlock, LimitsError, RewriteCompileError, SecretError,
};
use gproxy_channel::channel::{CredentialView, ProviderView, QuotaScope};
use gproxy_protocol::Operation;
use gproxy_store::entity::{
    limits::credential_block,
    upstream::{credential, provider},
};
use serde::Deserialize;
use std::{collections::HashMap, sync::Arc};

mod native;
pub use native::{assemble, connection_config};

/// Custom vocabularies parsed for estimation, by file id.
pub type VocabularyMap = HashMap<String, gproxy_tokenizer::Vocabulary>;

#[derive(Debug, thiserror::Error)]
pub enum AssemblyError {
    #[error("invalid outbound proxy configuration")]
    InvalidProxy,
    #[error("provider `{provider_id}` names unregistered channel `{channel}`")]
    UnknownChannel {
        provider_id: String,
        channel: String,
    },
    #[error("connection profile `{0}` is referenced but missing")]
    MissingConnectionProfile(String),
    #[error("connection profile `{id}` is invalid: {reason}")]
    InvalidConnectionProfile { id: String, reason: String },
    #[error("provider `{provider_id}` config is invalid: {reason}")]
    InvalidProviderConfig { provider_id: String, reason: String },
    #[error("outbound client for credential `{credential_id}` could not be built")]
    Client {
        credential_id: String,
        #[source]
        source: Arc<gproxy_client::Error>,
    },
    /// The provider-level client, resolved without a credential for a login
    /// flow or a connectivity probe.
    #[error("outbound client for provider `{provider_id}` could not be built")]
    ProviderClient {
        provider_id: String,
        #[source]
        source: Arc<gproxy_client::Error>,
    },
    #[error("secret of credential `{credential_id}` could not be opened")]
    Secret {
        credential_id: String,
        #[source]
        source: SecretError,
    },
    #[error(transparent)]
    Limits(#[from] LimitsError),
    #[error("operation endpoint `{id}` is invalid: {reason}")]
    InvalidEndpoint { id: String, reason: String },
    #[error("rewrite rule `{rule_id}` failed to compile")]
    Rewrite {
        rule_id: String,
        #[source]
        source: RewriteCompileError,
    },
    #[error("credential block `{id}` is invalid: {reason}")]
    InvalidBlock { id: String, reason: String },
}

/// Persisted blocks still in force at assembly time, keyed by credential.
pub type LiveBlocks = HashMap<String, Vec<CredentialBlock>>;

pub struct Assembly {
    pub data: CoreData,
    /// For warming the cache; the snapshot itself holds no blocks.
    pub blocks: LiveBlocks,
}

/// Provider-level knobs read from the `config` JSON column. Unknown keys are
/// the channel's business and are left alone.
#[derive(Deserialize, Default)]
pub(super) struct ProviderConfig {
    #[serde(default)]
    pub(super) credential_strategy: CredentialStrategy,
    pub(super) session_affinity: Option<bool>,
}

pub fn provider_view(entity: &provider::Model) -> ProviderView<'_> {
    ProviderView {
        id: &entity.id,
        channel: &entity.channel,
        base_url: entity.base_url.as_deref(),
        config: &entity.config,
    }
}

pub fn credential_view<'a>(
    row: &'a credential::Model,
    secret: &'a serde_json::Value,
) -> CredentialView<'a> {
    CredentialView {
        id: &row.id,
        provider_id: &row.provider_id,
        auth_kind: &row.auth_kind,
        secret,
        metadata: &row.metadata,
        version: row.version,
        expires_at_ms: row.expires_at_ms,
    }
}

pub fn block_from_row(row: &credential_block::Model) -> Result<CredentialBlock, AssemblyError> {
    let invalid = |reason: String| AssemblyError::InvalidBlock {
        id: row.id.clone(),
        reason,
    };
    let scope: QuotaScope =
        serde_json::from_value(row.scope.clone()).map_err(|e| invalid(e.to_string()))?;
    let source: BlockSource =
        serde_json::from_value(row.source.clone()).map_err(|e| invalid(e.to_string()))?;
    let operation = row
        .operation
        .as_deref()
        .map(|id| {
            Operation::from_id(id).ok_or_else(|| invalid(format!("unknown operation `{id}`")))
        })
        .transpose()?;
    Ok(CredentialBlock {
        scope,
        operation,
        until_ms: row.until_ms,
        source,
        observed_at_ms: row.observed_at_ms,
    })
}

/// Credential → provider → global, independently of HTTP transport selection.
/// Explicit direct/system modes terminate inheritance just like an explicit URL.
pub fn resolve_proxy(
    credential: Option<&serde_json::Value>,
    provider: Option<&serde_json::Value>,
    global: Option<&serde_json::Value>,
) -> Result<(gproxy_client::ProxyConfig, &'static str), AssemblyError> {
    for (value, source) in [
        (credential, "credential"),
        (provider, "provider"),
        (global, "global"),
    ] {
        if let Some(value) = value.filter(|v| !v.is_null()) {
            return serde_json::from_value(value.clone())
                .map(|proxy| (proxy, source))
                .map_err(|_| AssemblyError::InvalidProxy);
        }
    }
    Ok((gproxy_client::ProxyConfig::Direct, "direct"))
}
