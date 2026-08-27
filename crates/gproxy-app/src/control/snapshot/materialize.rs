use gproxy_core::control::FailoverBudget;
use gproxy_core::{CoreError, Plan, Target};

use super::balance::{self, RotationCounters};
use super::types::{CompiledSnapshot, CredentialHealthMap, TargetSeed};

impl CompiledSnapshot {
    pub(super) fn plan(
        &self,
        seeds: Vec<TargetSeed>,
        max_attempts: Option<u32>,
        balance_key: i64,
        affinity: Option<i64>,
        health: &CredentialHealthMap,
        counters: &RotationCounters,
    ) -> Result<Plan, CoreError> {
        let targets = balance::order(seeds, balance_key, affinity, health, counters)
            .into_iter()
            .filter_map(|seed| {
                self.providers.get(&seed.provider_id).map(|stored| {
                    let mut provider = stored.clone();
                    if seed.fingerprint.is_some() {
                        provider.fingerprint = seed.fingerprint;
                    }
                    provider.proxy_url = super::super::settings::effective_proxy(
                        seed.proxy_url.as_deref(),
                        provider.proxy_url.as_deref(),
                        None,
                    );
                    Target {
                        provider,
                        credential: seed.credential,
                        upstream_model: seed.upstream_model,
                        tier: seed.tier,
                        rules: gproxy_core::TargetRules {
                            routing: self
                                .routing_rules
                                .get(&seed.provider_id)
                                .cloned()
                                .unwrap_or_default(),
                            process: self
                                .process_rules
                                .get(&seed.provider_id)
                                .cloned()
                                .unwrap_or_default(),
                        },
                    }
                })
            })
            .collect::<Vec<_>>();
        if targets.is_empty() {
            return Err(CoreError::NoCredentials);
        }
        let max_attempts =
            max_attempts.unwrap_or_else(|| u32::try_from(targets.len()).unwrap_or(u32::MAX));
        let max_attempts = max_attempts.min(self.settings.max_attempts);
        Ok(Plan {
            targets,
            budget: FailoverBudget { max_attempts },
        })
    }
}
