use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::sync::Arc;

use gproxy_channel_api::CredentialId;
use gproxy_core::ProviderRef;
use gproxy_store::StoreError;
use gproxy_store::records::ControlSnapshot;

use super::types::{CompiledRoute, CompiledSnapshot, CredentialStrategy, TargetSeed};
use super::{index, pricing, rules};

impl CompiledSnapshot {
    pub(super) fn build(
        stored: ControlSnapshot,
        runtime: &super::super::settings::RuntimeOverrides,
    ) -> Result<Self, StoreError> {
        validate_windows(&stored)?;
        let effective = super::super::settings::EffectiveSettings::read(&stored.settings, runtime);
        let stored = Arc::new(stored);
        let providers = stored
            .providers
            .iter()
            .filter(|provider| provider.enabled)
            .map(|provider| {
                let settings = gproxy_channels::canonical_provider_settings(
                    &provider.channel,
                    &provider.settings,
                )
                .map_err(|message| StoreError::InvalidData {
                    field: "provider settings",
                    message,
                })?;
                let credential_strategy = match provider.credential_strategy.as_str() {
                    "round_robin" => CredentialStrategy::RoundRobin,
                    "sticky" => CredentialStrategy::Sticky,
                    value => {
                        return Err(StoreError::InvalidData {
                            field: "provider credential_strategy",
                            message: format!("unsupported strategy {value}"),
                        });
                    }
                };
                Ok((
                    provider.id,
                    (
                        ProviderRef {
                            id: provider.id,
                            name: provider.name.clone(),
                            channel: gproxy_channels::canonical_channel_id(&provider.channel)
                                .into(),
                            settings,
                            fingerprint: effective
                                .spoof_emulation
                                .then(|| {
                                    super::super::fingerprint::parse(
                                        provider.tls_fingerprint.as_ref(),
                                    )
                                })
                                .flatten(),
                            proxy_url: super::super::settings::effective_proxy(
                                None,
                                provider.proxy_url.as_deref(),
                                effective.proxy.as_deref(),
                            ),
                        },
                        credential_strategy,
                    ),
                ))
            })
            .collect::<Result<BTreeMap<_, _>, StoreError>>()?;
        let strategies = providers
            .iter()
            .map(|(id, (_, strategy))| (*id, *strategy))
            .collect::<BTreeMap<_, _>>();
        let providers = providers
            .into_iter()
            .map(|(id, (provider, _))| (id, provider))
            .collect::<BTreeMap<_, _>>();
        let provider_names = providers
            .values()
            .map(|provider| (provider.name.clone(), provider.id))
            .collect();
        let (credentials, credential_providers) =
            credentials(&stored, &providers, effective.spoof_emulation);
        let routes = routes(
            &stored,
            &providers,
            &strategies,
            &credentials,
            &credential_providers,
        );
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
        let (routing_rules, process_rules) = rules::compile(&stored)?;
        Ok(Self {
            stored,
            settings: effective,
            providers,
            strategies,
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
            routing_rules,
            process_rules,
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
    for quota in &stored.quotas {
        for (field, value) in [
            ("quota_total", Some(quota.quota_total)),
            ("quota_daily", quota.quota_daily),
            ("quota_weekly", quota.quota_weekly),
            ("quota_monthly", quota.quota_monthly),
            ("quota_5h", quota.quota_5h),
            ("quota_7d", quota.quota_7d),
        ] {
            if value.is_some_and(|value| value <= rust_decimal::Decimal::ZERO) {
                return Err(StoreError::InvalidData {
                    field,
                    message: format!("quota {} has a non-positive limit", quota.id),
                });
            }
        }
    }
    Ok(())
}

fn credentials(
    stored: &ControlSnapshot,
    providers: &BTreeMap<i64, ProviderRef>,
    spoof_emulation: bool,
) -> (
    BTreeMap<i64, Vec<super::types::CredentialSeed>>,
    BTreeMap<i64, i64>,
) {
    let mut by_provider: BTreeMap<i64, Vec<super::types::CredentialSeed>> = BTreeMap::new();
    let mut credential_providers = BTreeMap::new();
    for credential in stored
        .credentials
        .iter()
        .filter(|credential| credential.enabled && providers.contains_key(&credential.provider_id))
    {
        by_provider
            .entry(credential.provider_id)
            .or_default()
            .push(super::types::CredentialSeed {
                id: CredentialId(credential.id),
                version: credential.version,
                weight: credential.weight,
                proxy_url: credential.proxy_url.clone(),
                fingerprint: spoof_emulation
                    .then(|| super::super::fingerprint::parse(credential.tls_fingerprint.as_ref()))
                    .flatten(),
            });
        credential_providers.insert(credential.id, credential.provider_id);
    }
    for credentials in by_provider.values_mut() {
        credentials.sort_by_key(|credential| credential.id.0);
    }
    (by_provider, credential_providers)
}

fn routes(
    stored: &ControlSnapshot,
    providers: &BTreeMap<i64, ProviderRef>,
    strategies: &BTreeMap<i64, CredentialStrategy>,
    credentials: &BTreeMap<i64, Vec<super::types::CredentialSeed>>,
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
            members.sort_by_key(|member| (member.tier, Reverse(member.weight), member.id));
            let mut targets = Vec::new();
            for member in members {
                if !providers.contains_key(&member.provider_id) {
                    continue;
                }
                let member_credentials: Vec<_> = match member.credential_id {
                    Some(id) if credential_providers.get(&id) == Some(&member.provider_id) => {
                        credentials
                            .get(&member.provider_id)
                            .into_iter()
                            .flatten()
                            .filter(|credential| credential.id == CredentialId(id))
                            .cloned()
                            .collect()
                    }
                    Some(_) => Vec::new(),
                    None => credentials
                        .get(&member.provider_id)
                        .cloned()
                        .unwrap_or_default(),
                };
                targets.extend(member_credentials.into_iter().map(|credential| TargetSeed {
                    member_id: member.id,
                    tier: member.tier,
                    member_weight: member.weight,
                    provider_id: member.provider_id,
                    credential: credential.id,
                    credential_version: credential.version,
                    credential_weight: credential.weight,
                    credential_strategy: strategies[&member.provider_id],
                    proxy_url: credential.proxy_url,
                    fingerprint: credential.fingerprint,
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
