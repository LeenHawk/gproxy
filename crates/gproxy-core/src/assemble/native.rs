//! Assembly proper: profiles resolve to pooled clients on every target.

use super::{Assembly, AssemblyError, LiveBlocks, ProviderConfig, block_from_row, provider_view};
use crate::{
    ConfigRevision, CoreData, CredentialData, CredentialState, CredentialVersion, ExecutionLimits,
    OperationEndpointKey, ProviderData, RewriteRuleSetData, SecretCodec,
    credential_limit::CredentialLimit, rewrite::compile_rule,
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
    vocabularies: super::VocabularyMap,
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
                session_affinity: config.session_affinity.unwrap_or(matches!(
                    config.credential_strategy,
                    crate::CredentialStrategy::Sticky
                        | crate::CredentialStrategy::RoundRobinAffinity
                )),
            },
        );
    }

    // Operator limits on credentials: `quotas` rows owned by a credential or
    // a provider. An unusable row is skipped, never fatal.
    let mut credential_limits = Vec::new();
    for row in control
        .quotas
        .iter()
        .filter(|q| q.enabled && CredentialLimit::is_limit_row(q))
    {
        match CredentialLimit::compile(row) {
            Ok(limit) => credential_limits.push(Arc::new(limit)),
            Err(reason) => tracing::warn!(quota = %row.id, %reason, "credential limit skipped"),
        }
    }

    let mut credentials = HashMap::new();
    for row in &control.credentials {
        let Some(provider) = providers.get_mut(&row.provider_id) else {
            continue;
        };
        // Credential profile → provider profile → Setting default profile →
        // channel default client → built-in defaults.
        let resolve = |id: &str| {
            connection_config(
                profiles
                    .get(id)
                    .ok_or_else(|| AssemblyError::MissingConnectionProfile(id.to_owned()))?,
            )
        };
        let explicit = row
            .connection_profile_id
            .as_deref()
            .or(provider.entity.connection_profile_id.as_deref())
            .or(default_profile_id);
        let mut config = match explicit {
            Some(id) => resolve(id)?,
            None => provider
                .channel
                .default_connection_for(gproxy_channel::channel::ConnectionPurpose::Request)
                .unwrap_or_default(),
        };
        config.proxy = super::resolve_proxy(
            row.proxy.as_ref(),
            provider.entity.proxy.as_ref(),
            control.settings.as_ref().and_then(|s| s.proxy.as_ref()),
        )?
        .0;
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
        let mut quota = provider
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
                        metadata: &row.metadata,
                        version: row.version,
                        expires_at_ms: row.expires_at_ms,
                    },
                )
            })
            .unwrap_or_default();
        // Operator limits follow the channel's own dimensions.
        let limits =
            crate::credential_limit::limits_for(&credential_limits, &row.provider_id, &row.id);
        quota.extend(limits.iter().map(|limit| limit.dimension()));
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
                limits,
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

    // Which custom vocabulary each provider model counts with: the catalog
    // model's file, else the Setting default; tokenizer selection is lazy.
    let local_tokenizer = {
        let files: HashMap<&str, &str> = control
            .models
            .iter()
            .filter_map(|m| Some((m.id.as_str(), m.vocabulary_file_id.as_deref()?)))
            .collect();
        let models = control
            .provider_models
            .iter()
            .filter(|pm| pm.enabled)
            .filter_map(|pm| {
                let file = files.get(pm.model_id.as_deref()?)?;
                Some((
                    (pm.provider_id.clone(), pm.upstream_name.clone()),
                    (*file).to_owned(),
                ))
            })
            .collect();
        Arc::new(crate::estimate::Estimator::new(
            if control
                .settings
                .as_ref()
                .is_some_and(|s| !s.enable_tokenizer_vocabs)
            {
                Default::default()
            } else {
                vocabularies
            },
            control
                .settings
                .as_ref()
                .and_then(|s| s.default_vocabulary_file_id.clone()),
            models,
        ))
    };
    let estimation = control
        .settings
        .as_ref()
        .is_none_or(|s| s.enable_usage || s.enable_settlement)
        .then(|| local_tokenizer.clone());
    // Caller budgets and pricing: an unusable row is skipped, never fatal,
    // so one bad price rule cannot keep a snapshot from publishing. Rows
    // owned by a credential or provider are limits, handled above.
    let mut budgets = Vec::new();
    for row in control
        .quotas
        .iter()
        .filter(|q| q.enabled && !CredentialLimit::is_limit_row(q))
    {
        match crate::budget::BudgetData::compile(row) {
            Ok(budget) => budgets.push(Arc::new(budget)),
            Err(reason) => tracing::warn!(quota = %row.id, %reason, "budget skipped"),
        }
    }
    let pricing = Arc::new(crate::pricing::PriceBook::compile(
        &control.price_rules,
        &control.price_rates,
        &control.price_tiers,
    ));
    Ok(Assembly {
        data: CoreData {
            revision,
            observation: crate::ObservationSettings::from_setting(control.settings.as_ref()),
            limits,
            estimation,
            local_tokenizer,
            budgets,
            credential_limits,
            pricing,
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

pub fn connection_config(
    profile: &connection_profile::Model,
) -> Result<gproxy_client::ConnectionConfig, AssemblyError> {
    use connection_profile::{Backend, RetryPolicy};
    use gproxy_client as client;
    let invalid = |reason: &str| AssemblyError::InvalidConnectionProfile {
        id: profile.id.clone(),
        reason: reason.to_owned(),
    };
    Ok(client::ConnectionConfig {
        backend: match profile.backend {
            Backend::Reqwest => client::Backend::Reqwest,
            Backend::Wreq => client::Backend::Wreq,
            Backend::ReqwestNative => client::Backend::ReqwestNative,
        },
        proxy: client::ProxyConfig::Direct,
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

#[cfg(test)]
mod vocabulary_settings_tests {
    use super::*;
    #[tokio::test]
    async fn disabling_custom_vocabularies_drops_cached_vocabularies_but_keeps_estimation() {
        use sea_orm::Set;
        let db = sea_orm::Database::connect("sqlite::memory:").await.unwrap();
        let store = gproxy_store::Store::new(db);
        store.sync().await.unwrap();
        let vocabulary = gproxy_tokenizer::Vocabulary::from_bytes(br#"{"version":"1.0","model":{"type":"WordLevel","vocab":{"[UNK]":0,"hello":1},"unk_token":"[UNK]"}}"#).unwrap();
        for enabled in [true, false, true] {
            store
                .settings()
                .update(gproxy_store::entity::config::setting::ActiveModel {
                    enable_tokenizer_vocabs: Set(enabled),
                    ..Default::default()
                })
                .await
                .unwrap();
            let data = store.load_control_data().await.unwrap();
            let out = assemble(
                &data,
                &ChannelRegistry::new(),
                &crate::PlaintextCodec,
                &gproxy_client::ClientPool::default(),
                HashMap::from([("fixture".to_owned(), vocabulary.clone())]),
                None,
                0,
            )
            .await
            .unwrap();
            let estimator = out.data.estimation.unwrap();
            assert_eq!(estimator.vocabulary("fixture").is_some(), enabled);
        }
    }
}
