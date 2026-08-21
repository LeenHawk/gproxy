//! Candidate selection (§3.3, §6.4): healthy-member ordering per route
//! strategy, then each member's credential pool filtered through credential
//! health and ordered per the provider's credential strategy (round_robin
//! rotation or sticky cache affinity).

mod affinity;
mod credential_affinity;
mod strategy;

use std::collections::HashMap;
use std::sync::Arc;

use crate::app::snapshot::{ControlPlaneSnapshot, ResolvedRoute};
use crate::health::config::breaker_config;
use crate::health::{CredAdmit, HealthState};
use crate::pipeline::context::Candidate;
use crate::pipeline::error::PipelineError;
use crate::store::cache::CacheBackend;
use crate::store::persistence::records::{Credential, Provider, Route, RouteMember};
use crate::util::time::unix_now;

pub(crate) use affinity::MemberAffinityPlan;
pub(crate) use affinity::take_session_id;
pub(crate) use credential_affinity::media_model as bound_media_model;
pub(crate) use credential_affinity::read as read_credential_binding;
pub(crate) use credential_affinity::realtime_model as bound_realtime_model;
pub(crate) use credential_affinity::record_media_response;
pub(crate) use credential_affinity::record_response as record_response_affinity;
pub(crate) use credential_affinity::request_key as credential_binding_key;

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

/// Health-filtered credential order for non-model Codex service operations.
/// User stickiness is intentionally excluded: only an explicit resource pin
/// may constrain the pool.
pub(crate) fn service_credentials(
    cp: &ControlPlaneSnapshot,
    provider: &Arc<Provider>,
    health: &HealthState,
    hard_pinned_credential: Option<i64>,
) -> Vec<Arc<Credential>> {
    let now = unix_now();
    let pool = cp
        .credentials_by_provider
        .get(&provider.id)
        .map(Vec::as_slice)
        .unwrap_or_default();
    if let Some(id) = hard_pinned_credential {
        return pool
            .iter()
            .find(|credential| credential.id == id)
            .filter(|credential| health.credential_available(credential.id, now) != CredAdmit::No)
            .cloned()
            .into_iter()
            .collect();
    }
    let filtered = pool
        .iter()
        .filter(|credential| health.credential_available(credential.id, now) != CredAdmit::No)
        .cloned()
        .collect::<Vec<_>>();
    strategy::order_credentials(
        &filtered,
        health.next_credential_rotation(provider.id),
        /*pinned*/ None,
    )
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
        hard_pinned_credential: Option<i64>,
    ) -> Result<Vec<Candidate>, PipelineError> {
        if self.credentials.is_empty() {
            return Err(PipelineError::NoCredentials);
        }
        let selection = CredentialSelection {
            health,
            cache,
            user_key_id,
            hard_pinned_credential,
            now: unix_now(),
        };
        let credentials = credential_pool(
            &self.credentials,
            &self.provider,
            &self.upstream_model_id,
            &selection,
        )
        .await;
        if credentials.is_empty() {
            return Err(PipelineError::NoCredentials);
        }
        Ok(credentials
            .into_iter()
            .map(|credential| {
                Candidate::for_provider(
                    Arc::clone(&self.provider),
                    credential,
                    self.upstream_model_id.clone(),
                )
            })
            .collect())
    }
}

struct CredentialSelection<'a> {
    health: &'a HealthState,
    cache: &'a dyn CacheBackend,
    user_key_id: Option<i64>,
    hard_pinned_credential: Option<i64>,
    now: i64,
}

/// Build the ordered candidate list for failover: healthy members per the
/// route strategy, each expanded across its provider's filtered + ordered
/// credential pool. `user_key_id` keys sticky credential affinity.
pub(crate) async fn candidates(
    prepared: &PreparedRoute,
    health: &HealthState,
    cache: &dyn CacheBackend,
    user_key_id: Option<i64>,
    session_id: Option<&str>,
    hard_pinned_credential: Option<i64>,
    conversation_fingerprint: Option<&[u8; 32]>,
) -> Result<Vec<Candidate>, PipelineError> {
    let now = unix_now();
    let selection = CredentialSelection {
        health,
        cache,
        user_key_id,
        hard_pinned_credential,
        now,
    };
    let member_affinity = affinity::prepare(
        cache,
        &prepared.route,
        user_key_id,
        session_id,
        conversation_fingerprint,
    )
    .await;
    let pinned_member = member_affinity
        .as_deref()
        .and_then(MemberAffinityPlan::pinned_member);
    let mut ordered = strategy::order_members(
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
    affinity::prefer_member(&mut ordered, pinned_member);
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
        for cred in credential_pool(pool, provider, &member.upstream_model_id, &selection).await {
            out.push(Candidate {
                provider: Arc::clone(provider),
                credential: cred,
                upstream_model_id: member.upstream_model_id.clone(),
                member_id: Some(member.id),
                member_affinity: member_affinity.clone(),
                credential_binding_key: None,
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
    selection: &CredentialSelection<'_>,
) -> Vec<Arc<Credential>> {
    if let Some(credential_id) = selection.hard_pinned_credential {
        return pool
            .iter()
            .find(|credential| credential.id == credential_id)
            .filter(|credential| {
                selection.health.credential_model_available(
                    credential.id,
                    upstream_model_id,
                    selection.now,
                ) != CredAdmit::No
            })
            .cloned()
            .into_iter()
            .collect();
    }

    let filtered: Vec<Arc<Credential>> = pool
        .iter()
        .filter(|c| {
            selection
                .health
                .credential_model_available(c.id, upstream_model_id, selection.now)
                != CredAdmit::No
        })
        .cloned()
        .collect();
    if filtered.is_empty() {
        return filtered;
    }

    let rotation = selection.health.next_credential_rotation(provider.id);
    let sticky_key = match (provider.credential_strategy.as_str(), selection.user_key_id) {
        ("sticky", Some(uk)) => Some(format!("aff:{}:{uk}", provider.id)),
        _ => None,
    };
    let pinned = affinity::read_pin(selection.cache, sticky_key.as_deref()).await;

    let ordered = strategy::order_credentials(&filtered, rotation, pinned);
    if let Some(key) = sticky_key
        && let Some(first) = ordered.first()
    {
        // Affinity is a best-effort hint: a failed write just loses
        // stickiness for this window.
        affinity::write_pin(selection.cache, &key, first.id).await;
    }
    ordered
}

/// Refresh a route-member pin only after that member actually served a 2xx.
pub(crate) async fn record_affinity(cache: &dyn CacheBackend, candidate: &Candidate) {
    if let (Some(plan), Some(member_id)) = (&candidate.member_affinity, candidate.member_id) {
        plan.record_success(cache, member_id).await;
    }
    if let Some(key) = candidate.credential_binding_key.as_deref() {
        credential_affinity::write(cache, key, candidate.credential.id).await;
    }
}
