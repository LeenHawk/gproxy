//! Three-level authz (§8-C): permission union and rate-limit precheck run
//! org → team → user after route resolution and before balance; the
//! estimate-aware quota precheck runs separately in `execute`, once the §17
//! pre-deduct estimate is known. Counters live in the cache (redis-direct in
//! multi-instance); nothing here reads persistence on the hot path.

use std::time::Duration;

use crate::app::snapshot::{ControlPlaneSnapshot, KeyIdentity};
use crate::billing::pending;
use crate::pipeline::error::PipelineError;
use crate::store::cache::CacheBackend;
use crate::store::persistence::records::Scope;
use crate::util::glob;

const MINUTE: i64 = 60;
const DAY: i64 = 86_400;

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

/// 403 unless the org (and team, when set) is enabled AND the permission
/// union matches `name`. No matching pattern anywhere = deny (secure default).
pub fn check_permission(
    cp: &ControlPlaneSnapshot,
    identity: &KeyIdentity,
    name: &str,
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
    // Effective permission = UNION of user ∪ team ∪ org patterns.
    for scope in scopes(identity) {
        if let Some(patterns) = cp.permissions_by_scope.get(&scope)
            && patterns.iter().any(|p| glob::matches(p, name))
        {
            return Ok(());
        }
    }
    Err(PipelineError::Forbidden)
}

/// user → team → org; first exceeded rule wins. Incr-then-check: the
/// rejected request is still counted (cheap, deterministic — no read-modify-
/// write race, at the cost of rejected requests consuming budget). A counter
/// backend failure refuses the request (fail-closed) — enforced limits must
/// not silently vanish with the cache.
pub async fn precheck_limits(
    cp: &ControlPlaneSnapshot,
    cache: &dyn CacheBackend,
    identity: &KeyIdentity,
    name: &str,
    now_unix: i64,
) -> Result<(), PipelineError> {
    for scope in scopes(identity) {
        let Some(rows) = cp.rate_limits_by_scope.get(&scope) else {
            continue;
        };
        for row in rows
            .iter()
            .filter(|r| glob::matches(&r.route_pattern, name))
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
pub async fn precheck_quota(
    cp: &ControlPlaneSnapshot,
    cache: &dyn CacheBackend,
    identity: &KeyIdentity,
    est_micros: i64,
) -> Result<(), PipelineError> {
    for (scope, scope_id) in scopes(identity) {
        if let Some(quota) = cp.quotas_by_scope.get(&(scope, scope_id)) {
            // In-flight pending unreadable → the quota can't be checked →
            // refuse (fail-closed), consistent with precheck_limits.
            let in_flight = pending::read(cache, scope, scope_id)
                .await
                .map_err(|_| PipelineError::CounterUnavailable)?
                .max(0);
            let exhausted =
                quota.cost_used + pending::micros_to_cost(in_flight) >= quota.quota_total;
            let overshoots = quota.cost_used
                + pending::micros_to_cost(in_flight + est_micros.max(0))
                > quota.quota_total;
            if exhausted || overshoots {
                return Err(PipelineError::QuotaExceeded);
            }
        }
    }
    Ok(())
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

/// The pipeline entry point for permission → limits. Quota is NOT checked
/// here: the estimate-aware [`precheck_quota`] runs once at the common point
/// in [`execute`](crate::pipeline::execute), after the §17 pre-deduct
/// estimate is known and before `pending::charge`.
pub async fn authorize(
    cp: &ControlPlaneSnapshot,
    cache: &dyn CacheBackend,
    identity: &KeyIdentity,
    name: &str,
    now_unix: i64,
) -> Result<(), PipelineError> {
    check_permission(cp, identity, name)?;
    precheck_limits(cp, cache, identity, name, now_unix).await
}

#[cfg(all(test, not(target_arch = "wasm32"), feature = "cache-memory"))]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::store::cache::{CounterError, InvalidationHandler, MemoryCache};
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
    async fn disabled_org_denies() {
        let identity = test_identity();
        let mut cp = ControlPlaneSnapshot::empty(1);
        cp.orgs_by_id.insert(10, Arc::new(org(false)));
        cp.permissions_by_scope
            .insert((Scope::User, 1), Arc::new(vec!["*".into()]));
        assert!(matches!(
            check_permission(&cp, &identity, "claude-main"),
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

        // An estimate that exactly fits the remainder is admitted.
        assert!(
            precheck_quota(&cp, &cache, &identity, 1_000_000)
                .await
                .is_ok()
        );
        // Regression: an estimate over the remainder is rejected up front
        // (previously admitted and blew through the quota).
        assert!(matches!(
            precheck_quota(&cp, &cache, &identity, 1_000_001).await,
            Err(PipelineError::QuotaExceeded)
        ));
        // est = 0 reduces to the plain exhaustion check: remaining > 0 admits.
        assert!(precheck_quota(&cp, &cache, &identity, 0).await.is_ok());
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
        let now = 1_000_000;
        for _ in 0..2 {
            precheck_limits(&cp, &cache, &identity, "claude-main", now)
                .await
                .expect("under limit");
        }
        match precheck_limits(&cp, &cache, &identity, "claude-main", now).await {
            Err(PipelineError::RateLimited { retry_after_secs }) => {
                assert!((1..=60).contains(&retry_after_secs));
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    /// A cache whose counters always fail — models a Redis/Turso outage.
    struct DownCache;

    #[async_trait::async_trait]
    impl CacheBackend for DownCache {
        async fn get(&self, _key: &str) -> Option<Vec<u8>> {
            None
        }
        async fn set(
            &self,
            _key: &str,
            _value: Vec<u8>,
            _ttl: Option<Duration>,
        ) -> Result<(), crate::store::cache::CacheError> {
            Err(crate::store::cache::CacheError)
        }
        async fn incr(
            &self,
            _key: &str,
            _delta: i64,
            _ttl: Option<Duration>,
        ) -> Result<i64, CounterError> {
            Err(CounterError)
        }
        async fn delete(&self, _key: &str) {}
        async fn publish(&self, _channel: &str, _payload: &[u8]) {}
        async fn subscribe(&self, _channel: &str, _handler: InvalidationHandler) {}
    }

    /// Regression: a counter-backend outage used to read as count 0 (fail-open),
    /// silently disabling configured rate limits and quotas. It must refuse.
    #[tokio::test]
    async fn counter_outage_fails_closed() {
        let identity = test_identity();
        let mut cp = ControlPlaneSnapshot::empty(1);
        cp.rate_limits_by_scope.insert(
            (Scope::User, 1),
            Arc::new(vec![RateLimit {
                id: 7,
                scope: Scope::User,
                scope_id: 1,
                route_pattern: "*".into(),
                rpm: Some(100),
                rpd: None,
                total_tokens: None,
                created_at: 0,
                updated_at: 0,
            }]),
        );
        cp.quotas_by_scope.insert(
            (Scope::User, 1),
            Arc::new(Quota {
                id: 1,
                scope: Scope::User,
                scope_id: 1,
                quota_total: "10".parse().unwrap(),
                cost_used: "0".parse().unwrap(),
                created_at: 0,
                updated_at: 0,
            }),
        );
        assert!(matches!(
            precheck_limits(&cp, &DownCache, &identity, "claude-main", 0).await,
            Err(PipelineError::CounterUnavailable)
        ));
        assert!(matches!(
            precheck_quota(&cp, &DownCache, &identity, 0).await,
            Err(PipelineError::CounterUnavailable)
        ));
    }
}
