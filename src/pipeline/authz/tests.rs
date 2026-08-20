//! Unit tests for §8-C authz (split out to keep `authz.rs` within size limits).

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
            api_key_digest_version: 2,
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

/// Regression: `cost_used` is billing-owned — settle writes it on every
/// request, and the control-plane snapshot only rebuilds on config
/// invalidation, so its copy freezes. Reading spend from the snapshot let keys
/// run hundreds of dollars past their limit without the gate ever tripping;
/// the gate must judge on the live rows instead.
#[tokio::test]
async fn quota_gate_reads_live_rows_not_the_stale_snapshot() {
    let identity = test_identity();
    let mut cp = ControlPlaneSnapshot::empty(1);
    let mut quota = Quota {
        id: 1,
        scope: Scope::User,
        scope_id: 1,
        quota_total: "10".parse().unwrap(),
        quota_daily: None,
        quota_weekly: None,
        quota_monthly: None,
        cost_used: "0".parse().unwrap(),
        day_used: Default::default(),
        day_anchor: 0,
        week_used: Default::default(),
        week_anchor: 0,
        month_used: Default::default(),
        month_anchor: 0,
        created_at: 0,
        updated_at: 0,
    };
    // Snapshot froze at boot, when nothing had been spent yet.
    cp.quotas_by_scope
        .insert((Scope::User, 1), Arc::new(quota.clone()));
    let plan = prepare_quota(&cp, &identity);
    let cache = MemoryCache::new();

    // Live rows say the $10 budget is spent → refuse, despite the snapshot.
    quota.cost_used = "10".parse().unwrap();
    let live: QuotaTable = [((Scope::User, 1), Arc::new(quota))].into_iter().collect();
    assert!(matches!(
        precheck_quota(&plan, &live, &cache, 0, 0).await,
        Err(PipelineError::QuotaExceeded)
    ));
    // Sanity: the same plan against the snapshot's own (stale) view admits —
    // that divergence IS the bug.
    assert!(
        precheck_quota(&plan, &cp.quotas_by_scope, &cache, 0, 0)
            .await
            .is_ok()
    );
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
            quota_daily: None,
            quota_weekly: None,
            quota_monthly: None,
            cost_used: "9".parse().unwrap(),
            day_used: Default::default(),
            day_anchor: 0,
            week_used: Default::default(),
            week_anchor: 0,
            month_used: Default::default(),
            month_anchor: 0,
            created_at: 0,
            updated_at: 0,
        }),
    );
    let cache = MemoryCache::new();
    let plan = prepare_quota(&cp, &identity);

    // An estimate that exactly fits the remainder is admitted.
    assert!(
        precheck_quota(&plan, &cp.quotas_by_scope, &cache, 1_000_000, 0)
            .await
            .is_ok()
    );
    // Regression: an estimate over the remainder is rejected up front
    // (previously admitted and blew through the quota).
    assert!(matches!(
        precheck_quota(&plan, &cp.quotas_by_scope, &cache, 1_000_001, 0).await,
        Err(PipelineError::QuotaExceeded)
    ));
    // est = 0 reduces to the plain exhaustion check: remaining > 0 admits.
    assert!(
        precheck_quota(&plan, &cp.quotas_by_scope, &cache, 0, 0)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn daily_quota_uses_only_the_current_window() {
    let identity = test_identity();
    let now = 10 * DAY;
    let mut quota = Quota {
        id: 1,
        scope: Scope::User,
        scope_id: 1,
        quota_total: "100".parse().unwrap(),
        quota_daily: Some("5".parse().unwrap()),
        quota_weekly: None,
        quota_monthly: None,
        cost_used: "1".parse().unwrap(),
        day_used: "5".parse().unwrap(),
        day_anchor: timewindow::day_key(now),
        week_used: Default::default(),
        week_anchor: 0,
        month_used: Default::default(),
        month_anchor: 0,
        created_at: 0,
        updated_at: 0,
    };
    let cache = MemoryCache::new();
    let mut cp = ControlPlaneSnapshot::empty(1);
    cp.quotas_by_scope
        .insert((Scope::User, 1), Arc::new(quota.clone()));
    assert!(matches!(
        precheck_quota(
            &prepare_quota(&cp, &identity),
            &cp.quotas_by_scope,
            &cache,
            0,
            now
        )
        .await,
        Err(PipelineError::QuotaExceeded)
    ));

    quota.day_anchor -= 1;
    cp.quotas_by_scope.insert((Scope::User, 1), Arc::new(quota));
    assert!(
        precheck_quota(
            &prepare_quota(&cp, &identity),
            &cp.quotas_by_scope,
            &cache,
            0,
            now
        )
        .await
        .is_ok()
    );
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
