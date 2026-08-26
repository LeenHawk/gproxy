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
        health: &CredentialHealthMap,
        counters: &RotationCounters,
    ) -> Result<Plan, CoreError> {
        let targets = balance::order(seeds, balance_key, health, counters)
            .into_iter()
            .filter_map(|seed| {
                self.providers.get(&seed.provider_id).map(|stored| {
                    let mut provider = stored.clone();
                    if seed.fingerprint.is_some() {
                        provider.fingerprint = seed.fingerprint;
                    }
                    if seed.proxy_url.is_some() {
                        provider.proxy_url = seed.proxy_url;
                    }
                    Target {
                        provider,
                        credential: seed.credential,
                        upstream_model: seed.upstream_model,
                        tier: seed.tier,
                    }
                })
            })
            .collect::<Vec<_>>();
        if targets.is_empty() {
            return Err(CoreError::NoCredentials);
        }
        let max_attempts =
            max_attempts.unwrap_or_else(|| u32::try_from(targets.len()).unwrap_or(u32::MAX));
        Ok(Plan {
            targets,
            budget: FailoverBudget { max_attempts },
        })
    }
}
