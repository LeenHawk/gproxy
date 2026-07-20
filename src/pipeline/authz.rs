//! Three-level authz (§8-C): permission union and rate-limit precheck run
//! org → team → user after route resolution and before balance; the
//! estimate-aware quota precheck runs separately in candidate admission once
//! the §17 pre-deduct estimate is known. Counters live in the cache (redis-direct in
//! multi-instance); nothing here reads persistence on the hot path.

use std::sync::Arc;
use std::time::Duration;

use crate::app::snapshot::{ControlPlaneSnapshot, KeyIdentity};
use crate::billing::pending;
use crate::pipeline::error::PipelineError;
use crate::store::cache::CacheBackend;
use crate::store::persistence::records::{Quota, RateLimit, Scope};
use crate::util::glob;

const MINUTE: i64 = 60;
const DAY: i64 = 86_400;

/// Snapshot-owned rate-limit input for one already-permitted request.
pub(crate) struct AuthorizationPlan {
    name: String,
    rate_limits: Vec<Arc<Vec<RateLimit>>>,
}

struct QuotaEntry {
    scope: Scope,
    scope_id: i64,
    quota: Arc<Quota>,
}

/// Snapshot-owned quota rows for one identity's scope chain.
pub(crate) struct QuotaPlan {
    entries: Vec<QuotaEntry>,
}

/// The identity's scope chain, most-specific first (check order §8-C).
fn scopes(identity: &KeyIdentity) -> Vec<(Scope, i64)> {
    let user = &identity.user;
    let mut chain = Vec::with_capacity(3);
    chain.push((Scope::User, user.id));
    if let Some(team_id) = user.team_id {
        chain.push((Scope::Team, team_id));
    }
    chain.push((Scope::Org, user.org_id));
    chain
}

fn identity_scopes_enabled(
    cp: &ControlPlaneSnapshot,
    identity: &KeyIdentity,
) -> Result<(), PipelineError> {
    let user = &identity.user;
    match cp.orgs_by_id.get(&user.org_id) {
        Some(org) if org.enabled => {}
        _ => return Err(PipelineError::Forbidden),
    }
    if let Some(team_id) = user.team_id {
        match cp.teams_by_id.get(&team_id) {
            Some(team) if team.enabled => {}
            _ => return Err(PipelineError::Forbidden),
        }
    }
    Ok(())
}

fn effective_patterns<'a>(
    cp: &'a ControlPlaneSnapshot,
    identity: &KeyIdentity,
) -> impl Iterator<Item = &'a str> {
    scopes(identity)
        .into_iter()
        .filter_map(|scope| cp.permissions_by_scope.get(&scope))
        .flat_map(|patterns| patterns.iter().map(String::as_str))
}

/// 403 unless the org (and team, when set) is enabled AND the permission
/// union matches `name`. No matching pattern anywhere = deny (secure default).
pub fn check_permission(
    cp: &ControlPlaneSnapshot,
    identity: &KeyIdentity,
    name: &str,
) -> Result<(), PipelineError> {
    identity_scopes_enabled(cp, identity)?;
    // Effective permission = UNION of user ∪ team ∪ org patterns.
    if effective_patterns(cp, identity).any(|pattern| glob::matches(pattern, name)) {
        return Ok(());
    }
    Err(PipelineError::Forbidden)
}

/// Canonical public authorization name for a provider model.
pub fn provider_model_name(provider: &str, model: &str) -> String {
    format!("{provider}/{model}")
}

/// Hierarchical provider/model permission check. A provider-name grant is a
/// parent grant for all of its models (backward compatible); otherwise the
/// complete `provider/model` name must match a permission glob.
pub fn check_provider_model_permission(
    cp: &ControlPlaneSnapshot,
    identity: &KeyIdentity,
    provider: &str,
    model: &str,
) -> Result<(), PipelineError> {
    identity_scopes_enabled(cp, identity)?;
    let full = provider_model_name(provider, model);
    if effective_patterns(cp, identity)
        .any(|pattern| glob::matches(pattern, provider) || glob::matches(pattern, &full))
    {
        Ok(())
    } else {
        Err(PipelineError::Forbidden)
    }
}

/// Whether the identity has any grant in a provider's model namespace. This
/// admits a model-list request before its live catalogue is known; individual
/// returned entries are still filtered with [`provider_model_permitted`].
pub fn provider_listing_permitted(
    cp: &ControlPlaneSnapshot,
    identity: &KeyIdentity,
    provider: &str,
) -> bool {
    if identity_scopes_enabled(cp, identity).is_err() {
        return false;
    }
    let prefix = format!("{provider}/");
    effective_patterns(cp, identity)
        .any(|pattern| glob::matches(pattern, provider) || glob::can_match_prefix(pattern, &prefix))
}

/// Boolean form of [`check_provider_model_permission`] for model listings.
pub fn provider_model_permitted(
    cp: &ControlPlaneSnapshot,
    identity: &KeyIdentity,
    provider: &str,
    model: &str,
) -> bool {
    check_provider_model_permission(cp, identity, provider, model).is_ok()
}

/// user → team → org; first exceeded rule wins. Incr-then-check: the
/// rejected request is still counted (cheap, deterministic — no read-modify-
/// write race, at the cost of rejected requests consuming budget). A counter
/// backend failure refuses the request (fail-closed) — enforced limits must
/// not silently vanish with the cache.
async fn precheck_limits(
    plan: &AuthorizationPlan,
    cache: &dyn CacheBackend,
    now_unix: i64,
) -> Result<(), PipelineError> {
    for rows in &plan.rate_limits {
        for row in rows
            .iter()
            .filter(|r| glob::matches(&r.route_pattern, &plan.name))
        {
            if let Some(limit) = row.rpm {
                let key = format!("rl:{}:m{}", row.id, now_unix / MINUTE);
                let count = cache
                    .incr(&key, 1, Some(Duration::from_secs(120)))
                    .await
                    .map_err(|_| PipelineError::CounterUnavailable)?;
                if count > limit {
                    return Err(PipelineError::RateLimited {
                        retry_after_secs: (MINUTE - now_unix % MINUTE) as u64,
                    });
                }
            }
            if let Some(limit) = row.rpd {
                let key = format!("rl:{}:d{}", row.id, now_unix / DAY);
                let count = cache
                    .incr(&key, 1, Some(Duration::from_secs(48 * 3600)))
                    .await
                    .map_err(|_| PipelineError::CounterUnavailable)?;
                if count > limit {
                    return Err(PipelineError::RateLimited {
                        retry_after_secs: (DAY - now_unix % DAY) as u64,
                    });
                }
            }
            if let Some(limit) = row.total_tokens {
                // Read-only precheck of the daily token budget; settle-time
                // reconciliation (M6 §17) increments `rlt:*` with each
                // request's actual total tokens.
                let key = format!("rlt:{}:d{}", row.id, now_unix / DAY);
                let count = cache
                    .incr(&key, 0, Some(Duration::from_secs(48 * 3600)))
                    .await
                    .map_err(|_| PipelineError::CounterUnavailable)?;
                if count > limit {
                    return Err(PipelineError::RateLimited {
                        retry_after_secs: (DAY - now_unix % DAY) as u64,
                    });
                }
            }
        }
    }
    Ok(())
}

/// §17 quota admission, estimate-aware. Every scope quota must satisfy BOTH:
/// persisted `cost_used` + in-flight pending (the §17 pre-deduct, read from
/// `qp:*`) < `quota_total` (the plain exhaustion check — all `est_micros == 0`
/// reduces to exactly this), AND the request's own estimate must still fit:
/// `cost_used` + cost(in-flight + est) <= `quota_total` (an estimate that
/// exactly fits is admitted). The estimate is summed with the in-flight
/// micros BEFORE the `micros_to_cost` conversion — that sum is precisely what
/// the `qp:*` counter holds after `pending::charge`, so admission, settle and
/// refund all reconcile against the same number. Negative pending (stray
/// refunds) never grants extra budget.
pub(crate) async fn precheck_quota(
    plan: &QuotaPlan,
    cache: &dyn CacheBackend,
    est_micros: i64,
) -> Result<(), PipelineError> {
    for entry in &plan.entries {
        // In-flight pending unreadable → the quota can't be checked → refuse
        // (fail-closed), consistent with precheck_limits.
        let in_flight = pending::read(cache, entry.scope, entry.scope_id)
            .await
            .map_err(|_| PipelineError::CounterUnavailable)?
            .max(0);
        let exhausted =
            entry.quota.cost_used + pending::micros_to_cost(in_flight) >= entry.quota.quota_total;
        let overshoots = entry.quota.cost_used
            + pending::micros_to_cost(in_flight + est_micros.max(0))
            > entry.quota.quota_total;
        if exhausted || overshoots {
            return Err(PipelineError::QuotaExceeded);
        }
    }
    Ok(())
}

pub(crate) fn prepare_quota(cp: &ControlPlaneSnapshot, identity: &KeyIdentity) -> QuotaPlan {
    let entries = scopes(identity)
        .into_iter()
        .filter_map(|(scope, scope_id)| {
            cp.quotas_by_scope
                .get(&(scope, scope_id))
                .map(|quota| QuotaEntry {
                    scope,
                    scope_id,
                    quota: Arc::clone(quota),
                })
        })
        .collect();
    QuotaPlan { entries }
}

pub(crate) fn prepared_quota_scopes(plan: &QuotaPlan) -> Vec<(Scope, i64)> {
    plan.entries
        .iter()
        .map(|entry| (entry.scope, entry.scope_id))
        .collect()
}

/// The scopes of `identity`'s chain that actually carry a quota row — the
/// targets of pre-deduct, settle-time reconcile, and error refund.
pub fn quota_scopes(cp: &ControlPlaneSnapshot, identity: &KeyIdentity) -> Vec<(Scope, i64)> {
    scopes(identity)
        .into_iter()
        .filter(|key| cp.quotas_by_scope.contains_key(key))
        .collect()
}

/// Rate-limit row ids on `identity`'s chain with a `total_tokens` budget
/// matching `name`. Settle feeds `rlt:{id}:d{day}` with each request's actual
/// total tokens (the counter [`precheck_limits`] reads).
pub fn token_limit_ids(cp: &ControlPlaneSnapshot, identity: &KeyIdentity, name: &str) -> Vec<i64> {
    let mut ids = Vec::new();
    for scope in scopes(identity) {
        if let Some(rows) = cp.rate_limits_by_scope.get(&scope) {
            ids.extend(
                rows.iter()
                    .filter(|r| r.total_tokens.is_some() && glob::matches(&r.route_pattern, name))
                    .map(|r| r.id),
            );
        }
    }
    ids
}

/// Boolean form of [`check_permission`] for filtering model listings.
pub fn permitted(cp: &ControlPlaneSnapshot, identity: &KeyIdentity, name: &str) -> bool {
    check_permission(cp, identity, name).is_ok()
}

fn prepare_limits(
    cp: &ControlPlaneSnapshot,
    identity: &KeyIdentity,
    name: &str,
) -> AuthorizationPlan {
    let rate_limits = scopes(identity)
        .into_iter()
        .filter_map(|scope| cp.rate_limits_by_scope.get(&scope).cloned())
        .collect();
    AuthorizationPlan {
        name: name.to_owned(),
        rate_limits,
    }
}

pub(crate) fn prepare(
    cp: &ControlPlaneSnapshot,
    identity: &KeyIdentity,
    name: &str,
) -> Result<AuthorizationPlan, PipelineError> {
    check_permission(cp, identity, name)?;
    Ok(prepare_limits(cp, identity, name))
}

/// Provider/model authorization entry point: parent provider grants remain
/// valid, while child grants can constrain access to specific model globs.
pub(crate) fn prepare_provider_model(
    cp: &ControlPlaneSnapshot,
    identity: &KeyIdentity,
    provider: &str,
    model: &str,
) -> Result<AuthorizationPlan, PipelineError> {
    check_provider_model_permission(cp, identity, provider, model)?;
    // Rate-limit rows retain their existing provider-level semantics. Model
    // hierarchy is an authorization concern; changing counter attribution here
    // would also require changing settle-time token-limit accounting.
    Ok(prepare_limits(cp, identity, provider))
}

/// Authorize entry into a provider's model-list namespace before the live
/// catalogue is known. The response is always filtered per model afterwards.
pub(crate) fn prepare_provider_listing(
    cp: &ControlPlaneSnapshot,
    identity: &KeyIdentity,
    provider: &str,
) -> Result<AuthorizationPlan, PipelineError> {
    if !provider_listing_permitted(cp, identity, provider) {
        return Err(PipelineError::Forbidden);
    }
    Ok(prepare_limits(cp, identity, provider))
}

pub(crate) async fn authorize(
    plan: &AuthorizationPlan,
    cache: &dyn CacheBackend,
    now_unix: i64,
) -> Result<(), PipelineError> {
    precheck_limits(plan, cache, now_unix).await
}

#[cfg(all(test, not(target_arch = "wasm32"), feature = "cache-memory"))]
#[path = "authz/outage_tests.rs"]
mod outage_tests;

#[cfg(all(test, not(target_arch = "wasm32"), feature = "cache-memory"))]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::store::cache::MemoryCache;
    use crate::store::persistence::records::{Org, Quota, RateLimit, User, UserKey};

    fn test_identity() -> KeyIdentity {
        KeyIdentity {
            user_key: UserKey {
                id: 1,
                user_id: 1,
                api_key_ciphertext: String::new(),
                api_key_digest: "d".into(),
                label: None,
                enabled: true,
                created_at: 0,
                updated_at: 0,
            },
            user: User {
                id: 1,
                name: "u".into(),
                org_id: 10,
                team_id: None,
                password: None,
                enabled: true,
                is_admin: false,
                created_at: 0,
                updated_at: 0,
            },
        }
    }

    fn org(enabled: bool) -> Org {
        Org {
            id: 10,
            name: "o".into(),
            enabled,
            description: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[tokio::test]
    async fn org_level_grant_unions_down() {
        let identity = test_identity();
        let mut cp = ControlPlaneSnapshot::empty(1);

        // No org row at all → deny (secure default).
        assert!(matches!(
            check_permission(&cp, &identity, "claude-main"),
            Err(PipelineError::Forbidden)
        ));

        cp.orgs_by_id.insert(10, Arc::new(org(true)));
        // Org enabled but no permission rows anywhere → still deny.
        assert!(matches!(
            check_permission(&cp, &identity, "claude-main"),
            Err(PipelineError::Forbidden)
        ));

        cp.permissions_by_scope
            .insert((Scope::Org, 10), Arc::new(vec!["claude-*".into()]));
        assert!(check_permission(&cp, &identity, "claude-main").is_ok());
        assert!(matches!(
            check_permission(&cp, &identity, "gpt-x"),
            Err(PipelineError::Forbidden)
        ));
    }

    #[tokio::test]
    async fn quota_admission_is_estimate_aware() {
        let identity = test_identity();
        let mut cp = ControlPlaneSnapshot::empty(1);
        // $10 total, $9 used → $1.00 (= 1_000_000 micro-dollars) remaining.
        cp.quotas_by_scope.insert(
            (Scope::User, 1),
            Arc::new(Quota {
                id: 1,
                scope: Scope::User,
                scope_id: 1,
                quota_total: "10".parse().unwrap(),
                cost_used: "9".parse().unwrap(),
                created_at: 0,
                updated_at: 0,
            }),
        );
        let cache = MemoryCache::new();
        let plan = prepare_quota(&cp, &identity);

        // An estimate that exactly fits the remainder is admitted.
        assert!(precheck_quota(&plan, &cache, 1_000_000).await.is_ok());
        // Regression: an estimate over the remainder is rejected up front
        // (previously admitted and blew through the quota).
        assert!(matches!(
            precheck_quota(&plan, &cache, 1_000_001).await,
            Err(PipelineError::QuotaExceeded)
        ));
        // est = 0 reduces to the plain exhaustion check: remaining > 0 admits.
        assert!(precheck_quota(&plan, &cache, 0).await.is_ok());
    }

    #[tokio::test]
    async fn rpm_trips_and_retry_after() {
        let identity = test_identity();
        let mut cp = ControlPlaneSnapshot::empty(1);
        cp.rate_limits_by_scope.insert(
            (Scope::User, 1),
            Arc::new(vec![RateLimit {
                id: 7,
                scope: Scope::User,
                scope_id: 1,
                route_pattern: "*".into(),
                rpm: Some(2),
                rpd: None,
                total_tokens: None,
                created_at: 0,
                updated_at: 0,
            }]),
        );
        let cache = MemoryCache::new();
        let plan = prepare_limits(&cp, &identity, "claude-main");
        let now = 1_000_000;
        for _ in 0..2 {
            precheck_limits(&plan, &cache, now)
                .await
                .expect("under limit");
        }
        match precheck_limits(&plan, &cache, now).await {
            Err(PipelineError::RateLimited { retry_after_secs }) => {
                assert!((1..=60).contains(&retry_after_secs));
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }
}
