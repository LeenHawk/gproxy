use std::sync::Arc;
use std::time::Duration;

use super::*;
use crate::store::cache::{CacheBackend, CounterError, InvalidationHandler};
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

#[test]
fn disabled_org_denies() {
    let identity = test_identity();
    let mut cp = ControlPlaneSnapshot::empty(1);
    cp.orgs_by_id.insert(
        10,
        Arc::new(Org {
            id: 10,
            name: "o".into(),
            enabled: false,
            description: None,
            created_at: 0,
            updated_at: 0,
        }),
    );
    cp.permissions_by_scope
        .insert((Scope::User, 1), Arc::new(vec!["*".into()]));
    assert!(matches!(
        check_permission(&cp, &identity, "claude-main"),
        Err(PipelineError::Forbidden)
    ));
}

/// A cache whose counters always fail, modeling a Redis/Turso outage.
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

/// A counter-backend outage must fail closed for rate limits and quotas.
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
    let limits = prepare_limits(&cp, &identity, "claude-main");
    let quota = prepare_quota(&cp, &identity);
    assert!(matches!(
        precheck_limits(&limits, &DownCache, 0).await,
        Err(PipelineError::CounterUnavailable)
    ));
    assert!(matches!(
        precheck_quota(&quota, &DownCache, 0).await,
        Err(PipelineError::CounterUnavailable)
    ));
}
