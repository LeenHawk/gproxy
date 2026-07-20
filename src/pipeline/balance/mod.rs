//! Candidate selection (§3.3, §6.4): healthy-member ordering per route
//! strategy, then each member's credential pool filtered through credential
//! health and ordered per the provider's credential strategy (round_robin
//! rotation or sticky cache affinity).

mod strategy;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::app::snapshot::{ControlPlaneSnapshot, ResolvedRoute};
use crate::health::config::breaker_config;
use crate::health::{CredAdmit, HealthState};
use crate::pipeline::context::Candidate;
use crate::pipeline::error::PipelineError;
use crate::store::cache::CacheBackend;
use crate::store::persistence::records::{Credential, Provider, Route, RouteMember};
use crate::util::time::unix_now;

/// Sticky-affinity pin TTL — rolling: refreshed on every pick.
const AFFINITY_TTL: Duration = Duration::from_secs(3600);

/// Snapshot-owned input for route balancing. Only providers and credential
/// pools referenced by this route are retained.
pub(crate) struct PreparedRoute {
    route: Route,
    members: Vec<RouteMember>,
    providers: HashMap<i64, Arc<Provider>>,
    credentials: HashMap<i64, Vec<Arc<Credential>>>,
}

pub(crate) struct PreparedProvider {
    provider: Arc<Provider>,
    credentials: Vec<Arc<Credential>>,
    upstream_model_id: String,
}

pub(crate) fn prepare(cp: &ControlPlaneSnapshot, resolved: &ResolvedRoute) -> PreparedRoute {
    let mut providers = HashMap::new();
    let mut credentials = HashMap::new();
    for member in &resolved.members {
        if let Some(provider) = cp.providers_by_id.get(&member.provider_id) {
            providers.insert(member.provider_id, Arc::clone(provider));
        }
        if let Some(pool) = cp.credentials_by_provider.get(&member.provider_id) {
            credentials.insert(member.provider_id, pool.clone());
        }
    }
    PreparedRoute {
        route: resolved.route.clone(),
        members: resolved.members.clone(),
        providers,
        credentials,
    }
}

impl PreparedRoute {
    pub(crate) fn provider_models(&self) -> impl Iterator<Item = (i64, &str)> {
        self.members
            .iter()
            .map(|member| (member.provider_id, member.upstream_model_id.as_str()))
    }
}

pub(crate) fn prepare_provider(
    cp: &ControlPlaneSnapshot,
    provider: &Arc<Provider>,
    requested: String,
) -> PreparedProvider {
    let upstream_model_id = cp
        .variant_base_by_provider
        .get(&provider.id)
        .and_then(|index| index.get(&requested))
        .cloned()
        .unwrap_or(requested);
    PreparedProvider {
        provider: Arc::clone(provider),
        credentials: cp
            .credentials_by_provider
            .get(&provider.id)
            .cloned()
            .unwrap_or_default(),
        upstream_model_id,
    }
}

impl PreparedProvider {
    pub(crate) fn provider_model(&self) -> (i64, &str) {
        (self.provider.id, &self.upstream_model_id)
    }

    pub(crate) async fn candidates(
        &self,
        health: &HealthState,
        cache: &dyn CacheBackend,
        user_key_id: Option<i64>,
    ) -> Result<Vec<Candidate>, PipelineError> {
        if self.credentials.is_empty() {
            return Err(PipelineError::NoCredentials);
        }
        let credentials = credential_pool(
            &self.credentials,
            &self.provider,
            &self.upstream_model_id,
            health,
            cache,
            user_key_id,
            unix_now(),
        )
        .await;
        if credentials.is_empty() {
            return Err(PipelineError::NoCredentials);
        }
        Ok(credentials
            .into_iter()
            .map(|credential| Candidate {
                provider: Arc::clone(&self.provider),
                credential,
                upstream_model_id: self.upstream_model_id.clone(),
                member_id: None,
            })
            .collect())
    }
}

/// Build the ordered candidate list for failover: healthy members per the
/// route strategy, each expanded across its provider's filtered + ordered
/// credential pool. `user_key_id` keys sticky credential affinity.
pub(crate) async fn candidates(
    prepared: &PreparedRoute,
    health: &HealthState,
    cache: &dyn CacheBackend,
    user_key_id: Option<i64>,
) -> Result<Vec<Candidate>, PipelineError> {
    let now = unix_now();
    let ordered = strategy::order_members(
        &prepared.route.strategy,
        &prepared.members,
        |m| {
            prepared
                .providers
                .get(&m.provider_id)
                .filter(|p| p.enabled)
                .map(|p| breaker_config(&p.settings_json))
        },
        health,
        || health.next_route_rotation(prepared.route.id),
        now,
    );
    if ordered.is_empty() {
        return Err(PipelineError::NoMembers);
    }

    let mut out = Vec::new();
    for member in ordered {
        let provider = prepared
            .providers
            .get(&member.provider_id)
            .expect("member admitted only with a live provider");
        let pool = prepared
            .credentials
            .get(&provider.id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for cred in credential_pool(
            pool,
            provider,
            &member.upstream_model_id,
            health,
            cache,
            user_key_id,
            now,
        )
        .await
        {
            out.push(Candidate {
                provider: Arc::clone(provider),
                credential: cred,
                upstream_model_id: member.upstream_model_id.clone(),
                member_id: Some(member.id),
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
    pool: &[Arc<Credential>],
    provider: &Arc<Provider>,
    upstream_model_id: &str,
    health: &HealthState,
    cache: &dyn CacheBackend,
    user_key_id: Option<i64>,
    now: i64,
) -> Vec<Arc<Credential>> {
    let filtered: Vec<Arc<Credential>> = pool
        .iter()
        .filter(|c| {
            health.credential_model_available(c.id, upstream_model_id, now) != CredAdmit::No
        })
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
