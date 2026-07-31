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
use crate::util::timewindow;

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

/// One matching rate-limit budget, materialized for the parallel incr pass.
struct LimitCheck {
    key: String,
    /// 1 for request counters; 0 for the read-only token budget (settle-time
    /// reconciliation feeds `rlt:*`).
    delta: i64,
    ttl: Duration,
    limit: i64,
    retry_after_secs: u64,
}

/// user → team → org; first exceeded rule (in chain order) wins. Incr-then-
/// check: the rejected request is still counted (cheap, deterministic — no
/// read-modify-write race, at the cost of rejected requests consuming budget
/// on EVERY matching counter — the incrs run in parallel, so a tripped rule
/// no longer spares its siblings). One parallel round instead of N serial
/// RTTs on remote counter backends. A counter backend failure refuses the
/// request (fail-closed) — enforced limits must not silently vanish with the
/// cache.
async fn precheck_limits(
    plan: &AuthorizationPlan,
    cache: &dyn CacheBackend,
    now_unix: i64,
) -> Result<(), PipelineError> {
    let mut checks: Vec<LimitCheck> = Vec::new();
    for rows in &plan.rate_limits {
        for row in rows
            .iter()
            .filter(|r| glob::matches(&r.route_pattern, &plan.name))
        {
            if let Some(limit) = row.rpm {
                checks.push(LimitCheck {
                    key: format!("rl:{}:m{}", row.id, now_unix / MINUTE),
                    delta: 1,
                    ttl: Duration::from_secs(120),
                    limit,
                    retry_after_secs: (MINUTE - now_unix % MINUTE) as u64,
                });
            }
            if let Some(limit) = row.rpd {
                checks.push(LimitCheck {
                    key: format!("rl:{}:d{}", row.id, now_unix / DAY),
                    delta: 1,
                    ttl: Duration::from_secs(48 * 3600),
                    limit,
                    retry_after_secs: (DAY - now_unix % DAY) as u64,
                });
            }
            if let Some(limit) = row.total_tokens {
                checks.push(LimitCheck {
                    key: format!("rlt:{}:d{}", row.id, now_unix / DAY),
                    delta: 0,
                    ttl: Duration::from_secs(48 * 3600),
                    limit,
                    retry_after_secs: (DAY - now_unix % DAY) as u64,
                });
            }
        }
    }
    if checks.is_empty() {
        return Ok(());
    }
    let counts = futures_util::future::join_all(
        checks
            .iter()
            .map(|c| cache.incr(&c.key, c.delta, Some(c.ttl))),
    )
    .await;
    for (check, count) in checks.iter().zip(counts) {
        let count = count.map_err(|_| PipelineError::CounterUnavailable)?;
        if count > check.limit {
            return Err(PipelineError::RateLimited {
                retry_after_secs: check.retry_after_secs,
            });
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
    now_unix: i64,
) -> Result<(), PipelineError> {
    if plan.entries.is_empty() {
        return Ok(());
    }
    let day_key = timewindow::day_key(now_unix);
    let week_key = timewindow::week_key(now_unix);
    let month_key = timewindow::month_key(now_unix);
    // All scope pendings read in parallel (one RTT round on remote backends).
    let reads = futures_util::future::join_all(
        plan.entries
            .iter()
            .map(|entry| pending::read(cache, entry.scope, entry.scope_id)),
    )
    .await;
    for (entry, in_flight) in plan.entries.iter().zip(reads) {
        // In-flight pending unreadable → the quota can't be checked → refuse
        // (fail-closed), consistent with precheck_limits.
        let in_flight = in_flight
            .map_err(|_| PipelineError::CounterUnavailable)?
            .max(0);
        let pending_cost = pending::micros_to_cost(in_flight);
        let projected_cost = pending::micros_to_cost(in_flight + est_micros.max(0));
        let exceeds = |used, limit| used + pending_cost >= limit || used + projected_cost > limit;
        let quota = &entry.quota;
        let day_used = if quota.day_anchor == day_key {
            quota.day_used
        } else {
            rust_decimal::Decimal::ZERO
        };
        let week_used = if quota.week_anchor == week_key {
            quota.week_used
        } else {
            rust_decimal::Decimal::ZERO
        };
        let month_used = if quota.month_anchor == month_key {
            quota.month_used
        } else {
            rust_decimal::Decimal::ZERO
        };
        if exceeds(quota.cost_used, quota.quota_total)
            || quota
                .quota_daily
                .is_some_and(|limit| exceeds(day_used, limit))
            || quota
                .quota_weekly
                .is_some_and(|limit| exceeds(week_used, limit))
            || quota
                .quota_monthly
                .is_some_and(|limit| exceeds(month_used, limit))
        {
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
#[path = "authz/tests.rs"]
mod tests;
