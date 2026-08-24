use std::collections::BTreeMap;
use std::sync::Arc;

use gproxy_channel_api::CredentialId;
use gproxy_core::ProviderRef;
use gproxy_store::StoreError;
use gproxy_store::records::ControlSnapshot;

use super::types::{CompiledRoute, CompiledSnapshot, TargetSeed};
use super::{index, pricing};

impl CompiledSnapshot {
    pub(super) fn build(stored: ControlSnapshot) -> Result<Self, StoreError> {
        validate_windows(&stored)?;
        let stored = Arc::new(stored);
        let providers = stored
            .providers
            .iter()
            .filter(|provider| provider.enabled)
            .map(|provider| {
                (
                    provider.id,
                    ProviderRef {
                        id: provider.id,
                        name: provider.name.clone(),
                        channel: provider.channel.clone(),
                        settings: provider.settings.clone(),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let provider_names = providers
            .values()
            .map(|provider| (provider.name.clone(), provider.id))
            .collect();
        let (credentials, credential_providers) = credentials(&stored, &providers);
        let routes = routes(&stored, &providers, &credentials, &credential_providers);
        let route_names = stored
            .routes
            .iter()
            .filter(|route| routes.contains_key(&route.id))
            .map(|route| (route.name.clone(), route.id))
            .collect();
        let (exposed, namespaces) = index::exposed(&stored, &routes);
        let (global_aliases, provider_aliases) = index::aliases(&stored.aliases);
        let pricing = pricing::compile(&stored.price_rules, &stored.price_rates)?;
        let identities = index::identities(&stored);
        Ok(Self {
            stored,
            providers,
            provider_names,
            credentials,
            routes,
            route_names,
            exposed,
            namespaces,
            global_aliases,
            provider_aliases,
            pricing,
            identities,
        })
    }
}

fn validate_windows(stored: &ControlSnapshot) -> Result<(), StoreError> {
    if let Some(limit) = stored
        .rate_limits
        .iter()
        .find(|limit| limit.window_seconds == 0)
    {
        return Err(StoreError::InvalidData {
            field: "window_seconds",
            message: format!("rate limit {} has a zero window", limit.id),
        });
    }
    if let Some(quota) = stored.quotas.iter().find(|quota| quota.window_seconds == 0) {
        return Err(StoreError::InvalidData {
            field: "window_seconds",
            message: format!("quota {} has a zero window", quota.id),
        });
    }
    Ok(())
}

fn credentials(
    stored: &ControlSnapshot,
    providers: &BTreeMap<i64, ProviderRef>,
) -> (BTreeMap<i64, Vec<CredentialId>>, BTreeMap<i64, i64>) {
    let mut by_provider: BTreeMap<i64, Vec<CredentialId>> = BTreeMap::new();
    let mut credential_providers = BTreeMap::new();
    for credential in stored
        .credentials
        .iter()
        .filter(|credential| credential.enabled && providers.contains_key(&credential.provider_id))
    {
        by_provider
            .entry(credential.provider_id)
            .or_default()
            .push(CredentialId(credential.id));
        credential_providers.insert(credential.id, credential.provider_id);
    }
    for credentials in by_provider.values_mut() {
        credentials.sort_by_key(|credential| credential.0);
    }
    (by_provider, credential_providers)
}

fn routes(
    stored: &ControlSnapshot,
    providers: &BTreeMap<i64, ProviderRef>,
    credentials: &BTreeMap<i64, Vec<CredentialId>>,
    credential_providers: &BTreeMap<i64, i64>,
) -> BTreeMap<i64, CompiledRoute> {
    stored
        .routes
        .iter()
        .filter(|route| route.enabled)
        .map(|route| {
            let mut members = stored
                .route_members
                .iter()
                .filter(|member| member.enabled && member.route_id == route.id)
                .collect::<Vec<_>>();
            members.sort_by_key(|member| (member.priority, member.id));
            let mut targets = Vec::new();
            for member in members {
                if !providers.contains_key(&member.provider_id) {
                    continue;
                }
                let member_credentials: Vec<_> = match member.credential_id {
                    Some(id) if credential_providers.get(&id) == Some(&member.provider_id) => {
                        vec![CredentialId(id)]
                    }
                    Some(_) => Vec::new(),
                    None => credentials
                        .get(&member.provider_id)
                        .cloned()
                        .unwrap_or_default(),
                };
                targets.extend(member_credentials.into_iter().map(|credential| TargetSeed {
                    provider_id: member.provider_id,
                    credential,
                    upstream_model: member.upstream_model.clone(),
                }));
            }
            (
                route.id,
                CompiledRoute {
                    max_attempts: route.max_attempts,
                    targets,
                },
            )
        })
        .collect()
}
