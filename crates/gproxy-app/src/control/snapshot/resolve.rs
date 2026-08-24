use gproxy_core::control::FailoverBudget;
use gproxy_core::{CoreError, Plan, RoutingMode, Target};
use rust_decimal::Decimal;

use super::types::{
    CompiledSnapshot, CredentialPressure, CredentialPressureMap, TargetSeed, namespace_route_ids,
};

impl CompiledSnapshot {
    pub(super) fn resolve(
        &self,
        model: Option<&str>,
        mode: &RoutingMode,
    ) -> Result<Plan, CoreError> {
        match mode {
            RoutingMode::Aggregated => self.aggregated(model),
            RoutingMode::Namespace { namespace } => self.namespace(namespace, model),
            RoutingMode::Scoped { provider } => self.scoped(provider, model),
            RoutingMode::Named { name } => {
                if self.namespaces.contains_key(&name.to_ascii_lowercase()) {
                    self.namespace(name, model)
                } else if let Some(route_id) = self.route_names.get(name) {
                    self.route(*route_id)
                } else {
                    self.scoped(name, model)
                }
            }
        }
    }

    fn aggregated(&self, model: Option<&str>) -> Result<Plan, CoreError> {
        let Some(model) = model else {
            return self.all_providers("");
        };
        let exposed = self
            .global_aliases
            .get(model)
            .map(String::as_str)
            .unwrap_or(model);
        let route_id = self
            .exposed
            .get(exposed)
            .ok_or_else(|| CoreError::UnknownRoute(model.to_owned()))?;
        self.route(*route_id)
    }

    fn namespace(&self, namespace: &str, model: Option<&str>) -> Result<Plan, CoreError> {
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
            return self.plan(seeds, None);
        };
        let exposed = self
            .global_aliases
            .get(model)
            .map(String::as_str)
            .unwrap_or(model);
        let route_id = routes
            .get(exposed)
            .ok_or_else(|| CoreError::UnknownRoute(format!("{namespace}/{model}")))?;
        self.route(*route_id)
    }

    fn scoped(&self, provider: &str, model: Option<&str>) -> Result<Plan, CoreError> {
        let provider_id = self
            .provider_names
            .get(provider)
            .copied()
            .ok_or_else(|| CoreError::UnknownProvider(provider.to_owned()))?;
        let upstream_model = model
            .map(|model| {
                self.provider_aliases
                    .get(&provider_id)
                    .and_then(|aliases| aliases.get(model))
                    .cloned()
                    .unwrap_or_else(|| model.to_owned())
            })
            .unwrap_or_default();
        let seeds = self
            .credentials
            .get(&provider_id)
            .into_iter()
            .flatten()
            .copied()
            .map(|credential| TargetSeed {
                provider_id,
                credential,
                upstream_model: upstream_model.clone(),
            })
            .collect();
        self.plan(seeds, None)
    }

    fn all_providers(&self, upstream_model: &str) -> Result<Plan, CoreError> {
        let seeds = self
            .credentials
            .iter()
            .flat_map(|(provider_id, credentials)| {
                credentials.iter().copied().map(|credential| TargetSeed {
                    provider_id: *provider_id,
                    credential,
                    upstream_model: upstream_model.to_owned(),
                })
            })
            .collect();
        self.plan(seeds, None)
    }

    fn route(&self, route_id: i64) -> Result<Plan, CoreError> {
        let route = self
            .routes
            .get(&route_id)
            .ok_or_else(|| CoreError::UnknownRoute(route_id.to_string()))?;
        self.plan(route.targets.clone(), Some(route.max_attempts))
    }

    fn plan(&self, seeds: Vec<TargetSeed>, max_attempts: Option<u32>) -> Result<Plan, CoreError> {
        let targets = seeds
            .into_iter()
            .filter_map(|seed| {
                self.providers
                    .get(&seed.provider_id)
                    .map(|provider| Target {
                        provider: provider.clone(),
                        credential: seed.credential,
                        upstream_model: seed.upstream_model,
                    })
            })
            .collect::<Vec<_>>();
        if targets.is_empty() {
            return Err(CoreError::NoCredentials);
        }
        let budget = max_attempts.unwrap_or_else(|| attempt_count(targets.len()));
        Ok(Plan {
            targets,
            budget: FailoverBudget {
                max_attempts: budget,
            },
        })
    }
}

fn attempt_count(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

pub(super) fn apply_pressure(plan: &mut Plan, pressure: &CredentialPressureMap, now: i64) {
    plan.targets
        .sort_by_key(|target| pressure_tier(pressure.get(&target.credential), now));
}

fn pressure_tier(
    pressure: Option<&std::collections::BTreeMap<String, CredentialPressure>>,
    now: i64,
) -> u8 {
    let pressure = pressure
        .into_iter()
        .flat_map(|windows| windows.values())
        .filter(|window| window.period_end.is_none_or(|period_end| period_end > now))
        .map(|window| window.used_percent)
        .max();
    match pressure {
        Some(pressure) if pressure >= Decimal::from(100) => 2,
        Some(pressure) if pressure >= Decimal::from(90) => 1,
        _ => 0,
    }
}
