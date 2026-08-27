use super::balance::RotationCounters;
use super::types::{CompiledSnapshot, CredentialHealthMap, TargetSeed, namespace_route_ids};
use gproxy_core::{CoreError, Plan, RoutingMode};

impl CompiledSnapshot {
    pub(super) fn resolve_alias(&self, model: &str, mode: &RoutingMode) -> String {
        let global = self
            .global_aliases
            .get(model)
            .map(String::as_str)
            .unwrap_or(model);
        let provider = match mode {
            RoutingMode::Scoped { provider } => self.provider_names.get(provider),
            RoutingMode::Named { name }
                if !self.namespaces.contains_key(&name.to_ascii_lowercase())
                    && !self.route_names.contains_key(name) =>
            {
                self.provider_names.get(name)
            }
            RoutingMode::Aggregated | RoutingMode::Namespace { .. } | RoutingMode::Named { .. } => {
                None
            }
        };
        provider
            .and_then(|provider| self.provider_aliases.get(provider))
            .and_then(|aliases| aliases.get(global))
            .cloned()
            .unwrap_or_else(|| global.to_owned())
    }

    pub(super) fn resolve_preprocessed(
        &self,
        model: Option<&str>,
        mode: &RoutingMode,
        affinity: Option<i64>,
        health: &CredentialHealthMap,
        counters: &RotationCounters,
    ) -> Result<Plan, CoreError> {
        match mode {
            RoutingMode::Aggregated => self.aggregated(model, affinity, health, counters),
            RoutingMode::Namespace { namespace } => {
                self.namespace(namespace, model, affinity, health, counters)
            }
            RoutingMode::Scoped { provider } => {
                self.scoped(provider, model, affinity, health, counters)
            }
            RoutingMode::Named { name } => {
                if self.namespaces.contains_key(&name.to_ascii_lowercase()) {
                    self.namespace(name, model, affinity, health, counters)
                } else if let Some(route_id) = self.route_names.get(name) {
                    self.route(*route_id, affinity, health, counters)
                } else {
                    self.scoped(name, model, affinity, health, counters)
                }
            }
        }
    }

    fn aggregated(
        &self,
        model: Option<&str>,
        affinity: Option<i64>,
        health: &CredentialHealthMap,
        counters: &RotationCounters,
    ) -> Result<Plan, CoreError> {
        let Some(model) = model else {
            return self.all_providers("", affinity, health, counters);
        };
        let route_id = self
            .exposed
            .get(model)
            .ok_or_else(|| CoreError::UnknownRoute(model.to_owned()))?;
        self.route(*route_id, affinity, health, counters)
    }

    fn namespace(
        &self,
        namespace: &str,
        model: Option<&str>,
        affinity: Option<i64>,
        health: &CredentialHealthMap,
        counters: &RotationCounters,
    ) -> Result<Plan, CoreError> {
        let namespace_key = namespace.to_ascii_lowercase();
        let routes = self
            .namespaces
            .get(&namespace_key)
            .ok_or_else(|| CoreError::UnknownRoute(namespace.to_owned()))?;
        let Some(model) = model else {
            let seeds = namespace_route_ids(routes)
                .filter_map(|id| self.routes.get(&id))
                .flat_map(|route| route.targets.iter().cloned())
                .collect();
            return self.plan(seeds, None, 0, affinity, health, counters);
        };
        let route_id = routes
            .get(model)
            .ok_or_else(|| CoreError::UnknownRoute(format!("{namespace}/{model}")))?;
        self.route(*route_id, affinity, health, counters)
    }

    fn scoped(
        &self,
        provider: &str,
        model: Option<&str>,
        affinity: Option<i64>,
        health: &CredentialHealthMap,
        counters: &RotationCounters,
    ) -> Result<Plan, CoreError> {
        let provider_id = self
            .provider_names
            .get(provider)
            .copied()
            .ok_or_else(|| CoreError::UnknownProvider(provider.to_owned()))?;
        let upstream_model = model.unwrap_or_default().to_owned();
        let seeds = self
            .credentials
            .get(&provider_id)
            .into_iter()
            .flatten()
            .cloned()
            .map(|credential| TargetSeed {
                member_id: provider_id,
                tier: 0,
                member_weight: 100,
                provider_id,
                credential: credential.id,
                credential_version: credential.version,
                credential_weight: credential.weight,
                credential_strategy: self.provider_strategy(provider_id),
                proxy_url: credential.proxy_url,
                fingerprint: credential.fingerprint,
                upstream_model: upstream_model.clone(),
            })
            .collect();
        self.plan(seeds, None, -provider_id, affinity, health, counters)
    }

    fn all_providers(
        &self,
        upstream_model: &str,
        affinity: Option<i64>,
        health: &CredentialHealthMap,
        counters: &RotationCounters,
    ) -> Result<Plan, CoreError> {
        let seeds = self
            .credentials
            .iter()
            .flat_map(|(provider_id, credentials)| {
                credentials.iter().cloned().map(|credential| TargetSeed {
                    member_id: *provider_id,
                    tier: 0,
                    member_weight: 100,
                    provider_id: *provider_id,
                    credential: credential.id,
                    credential_version: credential.version,
                    credential_weight: credential.weight,
                    credential_strategy: self.provider_strategy(*provider_id),
                    proxy_url: credential.proxy_url,
                    fingerprint: credential.fingerprint,
                    upstream_model: upstream_model.to_owned(),
                })
            })
            .collect();
        self.plan(seeds, None, 0, affinity, health, counters)
    }

    fn route(
        &self,
        route_id: i64,
        affinity: Option<i64>,
        health: &CredentialHealthMap,
        counters: &RotationCounters,
    ) -> Result<Plan, CoreError> {
        let route = self
            .routes
            .get(&route_id)
            .ok_or_else(|| CoreError::UnknownRoute(route_id.to_string()))?;
        self.plan(
            route.targets.clone(),
            Some(route.max_attempts),
            route_id,
            affinity,
            health,
            counters,
        )
    }

    fn provider_strategy(&self, provider_id: i64) -> super::types::CredentialStrategy {
        self.strategies
            .get(&provider_id)
            .copied()
            .unwrap_or(super::types::CredentialStrategy::RoundRobin)
    }
}
