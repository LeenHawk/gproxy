//! Candidate selection (§3.3, §6.4): healthy-member ordering per route
//! strategy, then each member's credential pool filtered through credential
//! health and ordered per the provider's credential strategy (round_robin
//! rotation or sticky cache affinity).

mod strategy;

use std::sync::Arc;
use std::time::Duration;

use crate::app::snapshot::{ControlPlaneSnapshot, ResolvedRoute};
use crate::health::config::{breaker_config, breaker_config_merged};
use crate::health::{CredAdmit, HealthState};
use crate::pipeline::context::Candidate;
use crate::pipeline::error::PipelineError;
use crate::store::cache::CacheBackend;
use crate::store::persistence::records::{Credential, Provider};
use crate::util::time::unix_now;

/// Sticky-affinity pin TTL — rolling: refreshed on every pick.
const AFFINITY_TTL: Duration = Duration::from_secs(3600);

/// Build the ordered candidate list for failover: healthy members per the
/// route strategy, each expanded across its provider's filtered + ordered
/// credential pool. `user_key_id` keys sticky credential affinity.
pub async fn candidates(
    cp: &ControlPlaneSnapshot,
    route: &ResolvedRoute,
    health: &HealthState,
    cache: &dyn CacheBackend,
    user_key_id: Option<i64>,
) -> Result<Vec<Candidate>, PipelineError> {
    let now = unix_now();
    let ordered = strategy::order_members(
        &route.route.strategy,
        &route.members,
        |m| {
            cp.providers_by_id
                .get(&m.provider_id)
                .filter(|p| p.enabled)
                .map(|p| breaker_config(&p.settings_json))
        },
        health,
        || health.next_route_rotation(route.route.id),
        now,
    );
    if ordered.is_empty() {
        return Err(PipelineError::NoMembers);
    }

    let mut out = Vec::new();
    for member in ordered {
        let provider = cp
            .providers_by_id
            .get(&member.provider_id)
            .expect("member admitted only with a live provider");
        // Member breaker thresholds: route override merged over the provider.
        let breaker_cfg =
            breaker_config_merged(route.route.settings_json.as_ref(), &provider.settings_json);
        for cred in credential_pool(cp, provider, health, cache, user_key_id, now).await {
            out.push(Candidate {
                provider: Arc::clone(provider),
                credential: cred,
                upstream_model_id: member.upstream_model_id.clone(),
                member_id: Some(member.id),
                breaker_cfg: breaker_cfg.clone(),
            });
        }
    }

    if out.is_empty() {
        return Err(PipelineError::NoCredentials);
    }
    Ok(out)
}

/// One provider's credential pool: health-filtered (breaker/cooldown `No`
/// excluded), rotated, and — for `sticky` providers — pinned per user key via
/// `aff:{provider_id}:{user_key_id}` with the pin (re-)set to the front
/// credential on every pick (rolling TTL).
async fn credential_pool(
    cp: &ControlPlaneSnapshot,
    provider: &Arc<Provider>,
    health: &HealthState,
    cache: &dyn CacheBackend,
    user_key_id: Option<i64>,
    now: i64,
) -> Vec<Arc<Credential>> {
    let Some(pool) = cp.credentials_by_provider.get(&provider.id) else {
        return Vec::new();
    };
    let cfg = breaker_config(&provider.settings_json);
    let filtered: Vec<Arc<Credential>> = pool
        .iter()
        .filter(|c| health.admit_credential(c.id, &cfg, now) != CredAdmit::No)
        .cloned()
        .collect();
    if filtered.is_empty() {
        return filtered;
    }

    let rotation = health.next_credential_rotation(provider.id);
    let sticky_key = match (provider.credential_strategy.as_str(), user_key_id) {
        ("sticky", Some(uk)) => Some(format!("aff:{}:{uk}", provider.id)),
        _ => None,
    };
    let pinned = match &sticky_key {
        Some(key) => cache
            .get(key)
            .await
            .and_then(|v| String::from_utf8(v).ok())
            .and_then(|s| s.parse::<i64>().ok()),
        None => None,
    };

    let ordered = strategy::order_credentials(&filtered, rotation, pinned);
    if let Some(key) = sticky_key
        && let Some(first) = ordered.first()
    {
        // Affinity is a best-effort hint: a failed write just loses
        // stickiness for this window.
        let _ = cache
            .set(&key, first.id.to_string().into_bytes(), Some(AFFINITY_TTL))
            .await;
    }
    ordered
}
