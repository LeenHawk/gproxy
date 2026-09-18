//! Native assembly: needs the outbound client pool, which has no wasm form yet.

use super::{Assembly, AssemblyError, LiveBlocks, ProviderConfig, block_from_row, provider_view};
use crate::{
    ConfigRevision, CoreData, CredentialData, CredentialState, CredentialVersion, ExecutionLimits,
    OperationEndpointKey, ProviderData, RewriteRuleSetData, SecretCodec, rewrite::compile_rule,
};
use gproxy_channel::{ChannelRegistry, channel::CredentialView};
use gproxy_protocol::{Dialect, Operation, OperationKey};
use gproxy_store::{ControlData, entity::config::connection_profile};
use std::{collections::HashMap, sync::Arc};

pub async fn assemble(
    control: &ControlData,
    channels: &ChannelRegistry,
    codec: &dyn SecretCodec,
    clients: &gproxy_client::ClientPool,
    previous: Option<&CoreData>,
    now_ms: i64,
) -> Result<Assembly, AssemblyError> {
    let (revision, limits) = match &control.settings {
        Some(settings) => (
            ConfigRevision(u64::try_from(settings.config_revision).unwrap_or(0)),
            ExecutionLimits::from_settings(settings)?,
        ),
        None => (ConfigRevision(0), ExecutionLimits::default()),
    };
    let profiles: HashMap<&str, &connection_profile::Model> = control
        .connection_profiles
        .iter()
        .map(|profile| (profile.id.as_str(), profile))
        .collect();
    let default_profile_id = control
        .settings
        .as_ref()
        .and_then(|settings| settings.connection_profile_id.as_deref());

    let mut rule_sets = HashMap::new();
    for set in control.rewrite_rule_sets.iter().filter(|set| set.enabled) {
        let entity = Arc::new(set.clone());
        let mut rules = Vec::new();
        for rule in control
            .rewrite_rules
            .iter()
            .filter(|rule| rule.enabled && rule.rule_set_id == set.id)
        {
            rules.push(Arc::new(compile_rule(Arc::new(rule.clone())).map_err(
                |source| AssemblyError::Rewrite {
                    rule_id: rule.id.clone(),
                    source,
                },
            )?));
        }
        rule_sets.insert(
            set.id.clone(),
            Arc::new(RewriteRuleSetData { entity, rules }),
        );
    }

    let mut providers = HashMap::new();
    for row in control.providers.iter().filter(|p| p.enabled) {
        let channel = channels
            .get(&row.channel)
            .ok_or_else(|| AssemblyError::UnknownChannel {
                provider_id: row.id.clone(),
                channel: row.channel.clone(),
            })?
            .clone();
        let config: ProviderConfig = serde_json::from_value(row.config.clone()).map_err(|e| {
            AssemblyError::InvalidProviderConfig {
                provider_id: row.id.clone(),
                reason: e.to_string(),
            }
        })?;
        let mut operation_urls = HashMap::new();
        for endpoint in control
            .operation_endpoints
            .iter()
            .filter(|e| e.enabled && e.provider_id == row.id)
        {
            let invalid = |reason: &str| AssemblyError::InvalidEndpoint {
                id: endpoint.id.clone(),
                reason: reason.to_owned(),
            };
            let operation = Operation::from_id(&endpoint.operation)
                .ok_or_else(|| invalid("unknown operation"))?;
            let dialect =
                Dialect::from_id(&endpoint.dialect).ok_or_else(|| invalid("unknown dialect"))?;
            let url = endpoint.url.trim();
            if url.is_empty() || !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err(invalid("url must be an absolute http(s) URL"));
            }
            operation_urls.insert(
                OperationEndpointKey {
                    operation: OperationKey { operation, dialect },
                    transport: endpoint.transport,
                },
                url.to_owned(),
            );
        }
        let rewrite_rule_sets = control
            .provider_rewrite_rule_sets
            .iter()
            .filter(|a| {
                a.enabled && a.provider_id == row.id && rule_sets.contains_key(&a.rule_set_id)
            })
            .map(|a| Arc::new(a.clone()))
            .collect();
        providers.insert(
            row.id.clone(),
            ProviderData {
                entity: Arc::new(row.clone()),
                channel,
                credential_ids: Vec::new(),
                models: control
                    .provider_models
                    .iter()
                    .filter(|m| m.enabled && m.provider_id == row.id)
                    .map(|m| Arc::new(m.clone()))
                    .collect(),
                operation_rules: control
                    .operation_rules
                    .iter()
                    .filter(|r| r.provider_id == row.id)
                    .map(|r| Arc::new(r.clone()))
                    .collect(),
                operation_urls,
                rewrite_rule_sets,
                credential_strategy: config.credential_strategy,
            },
        );
    }

    let mut credentials = HashMap::new();
    for row in &control.credentials {
        let Some(provider) = providers.get_mut(&row.provider_id) else {
            continue;
        };
        let profile_id = row
            .connection_profile_id
            .as_deref()
            .or(provider.entity.connection_profile_id.as_deref())
            .or(default_profile_id);
        let config = match profile_id {
            Some(id) => connection_config(
                profiles
                    .get(id)
                    .ok_or_else(|| AssemblyError::MissingConnectionProfile(id.to_owned()))?,
            )?,
            None => gproxy_client::ConnectionConfig::default(),
        };
        let client_error = |source| AssemblyError::Client {
            credential_id: row.id.clone(),
            source,
        };
        let client = clients.get(&config).await.map_err(client_error)?;
        let websocket_client = clients.get_websocket(&config).await.map_err(client_error)?;
        let secret = codec
            .open(&row.id, &row.secret)
            .map_err(|source| AssemblyError::Secret {
                credential_id: row.id.clone(),
                source,
            })?;
        let quota = provider
            .channel
            .quota_model()
            .map(|model| {
                model.dimensions(
                    provider_view(&provider.entity),
                    CredentialView {
                        id: &row.id,
                        provider_id: &row.provider_id,
                        auth_kind: &row.auth_kind,
                        secret: &secret,
                        version: row.version,
                        expires_at_ms: row.expires_at_ms,
                    },
                )
            })
            .unwrap_or_default();
        let version = Arc::new(CredentialVersion {
            version: row.version,
            expires_at_ms: row.expires_at_ms,
            secret,
            status: row.status,
            status_reason: row.status_reason.clone(),
        });
        let state = match previous
            .and_then(|data| data.credentials.get(&row.id))
            .filter(|existing| !existing.state.is_retired())
        {
            Some(existing) => {
                existing.state.publish_if_newer(version);
                existing.state.clone()
            }
            None => Arc::new(CredentialState::new(version)),
        };
        provider.credential_ids.push(row.id.clone());
        credentials.insert(
            row.id.clone(),
            Arc::new(CredentialData {
                id: row.id.clone(),
                provider_id: row.provider_id.clone(),
                label: row.label.clone(),
                auth_kind: row.auth_kind.clone(),
                enabled: row.enabled,
                metadata: row.metadata.clone(),
                client,
                websocket_client,
                state,
                quota,
            }),
        );
    }

    let mut blocks: LiveBlocks = HashMap::new();
    for row in control
        .credential_blocks
        .iter()
        .filter(|b| b.until_ms > now_ms && credentials.contains_key(&b.credential_id))
    {
        blocks
            .entry(row.credential_id.clone())
            .or_default()
            .push(block_from_row(row)?);
    }

    Ok(Assembly {
        data: CoreData {
            revision,
            limits,
            providers: providers
                .into_iter()
                .map(|(id, provider)| (id, Arc::new(provider)))
                .collect(),
            credentials,
            rewrite_rule_sets: rule_sets,
        },
        blocks,
    })
}

fn connection_config(
    profile: &connection_profile::Model,
) -> Result<gproxy_client::ConnectionConfig, AssemblyError> {
    use connection_profile::{Backend, ProxyMode, RetryPolicy};
    use gproxy_client as client;
    let invalid = |reason: &str| AssemblyError::InvalidConnectionProfile {
        id: profile.id.clone(),
        reason: reason.to_owned(),
    };
    Ok(client::ConnectionConfig {
        backend: match profile.backend {
            Backend::Reqwest => client::Backend::Reqwest,
            Backend::Wreq => client::Backend::Wreq,
        },
        proxy: match profile.proxy_mode {
            ProxyMode::Direct => client::ProxyConfig::Direct,
            ProxyMode::System => client::ProxyConfig::System,
            ProxyMode::Explicit => client::ProxyConfig::Explicit {
                url: profile
                    .proxy_url
                    .clone()
                    .filter(|url| !url.trim().is_empty())
                    .ok_or_else(|| invalid("explicit proxy mode requires proxy_url"))?,
            },
        },
        emulation: profile
            .emulation
            .as_ref()
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()
            .map_err(|e| invalid(&format!("emulation: {e}")))?,
        gzip: profile.gzip,
        brotli: profile.brotli,
        deflate: profile.deflate,
        zstd: profile.zstd,
        redirect_max_hops: profile.redirect_max_hops,
        retry: match profile.retry {
            RetryPolicy::Never => client::RetryPolicy::Never,
            RetryPolicy::Default => client::RetryPolicy::Default,
        },
        connect_timeout_ms: profile.connect_timeout_ms,
        pool_idle_timeout_ms: profile.pool_idle_timeout_ms,
        pool_max_idle_per_host: profile.pool_max_idle_per_host,
    })
}
