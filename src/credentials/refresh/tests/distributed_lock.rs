use super::*;

use crate::store::cache::{CacheError, CounterError, InvalidationHandler};

struct ContendedOnceCache {
    inner: MemoryCache,
    attempts: Arc<AtomicUsize>,
}

pub(super) struct LostLeaseCache(pub(super) MemoryCache);

#[async_trait]
impl CacheBackend for LostLeaseCache {
    async fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.0.get(key).await
    }

    async fn set(
        &self,
        key: &str,
        value: Vec<u8>,
        ttl: Option<std::time::Duration>,
    ) -> Result<(), CacheError> {
        self.0.set(key, value, ttl).await
    }

    async fn incr(
        &self,
        key: &str,
        delta: i64,
        ttl: Option<std::time::Duration>,
    ) -> Result<i64, CounterError> {
        self.0.incr(key, delta, ttl).await
    }

    async fn delete(&self, key: &str) {
        self.0.delete(key).await;
    }

    async fn publish(&self, channel: &str, payload: &[u8]) {
        self.0.publish(channel, payload).await;
    }

    async fn subscribe(&self, channel: &str, handler: InvalidationHandler) {
        self.0.subscribe(channel, handler).await;
    }

    async fn try_lock(&self, _key: &str, _owner: &str, _ttl: std::time::Duration) -> LockAttempt {
        LockAttempt::Acquired
    }

    async fn extend_lock(&self, _key: &str, _owner: &str, _ttl: std::time::Duration) -> bool {
        false
    }
}

#[async_trait]
impl CacheBackend for ContendedOnceCache {
    async fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.inner.get(key).await
    }

    async fn set(
        &self,
        key: &str,
        value: Vec<u8>,
        ttl: Option<std::time::Duration>,
    ) -> Result<(), CacheError> {
        self.inner.set(key, value, ttl).await
    }

    async fn incr(
        &self,
        key: &str,
        delta: i64,
        ttl: Option<std::time::Duration>,
    ) -> Result<i64, CounterError> {
        self.inner.incr(key, delta, ttl).await
    }

    async fn delete(&self, key: &str) {
        self.inner.delete(key).await;
    }

    async fn publish(&self, channel: &str, payload: &[u8]) {
        self.inner.publish(channel, payload).await;
    }

    async fn subscribe(&self, channel: &str, handler: InvalidationHandler) {
        self.inner.subscribe(channel, handler).await;
    }

    async fn try_lock(&self, _key: &str, _owner: &str, _ttl: std::time::Duration) -> LockAttempt {
        if self.attempts.fetch_add(1, Ordering::SeqCst) > 0 {
            LockAttempt::Acquired
        } else {
            LockAttempt::Busy
        }
    }
}

#[tokio::test]
async fn contention_retries_before_refreshing() {
    let cipher = cipher();
    let (mut state, cred, _dir) = state_with_cred(cipher, json!({"access_token": "old"})).await;
    let attempts = Arc::new(AtomicUsize::new(0));
    state.cache = Arc::new(ContendedOnceCache {
        inner: MemoryCache::new(),
        attempts: Arc::clone(&attempts),
    });
    let refreshes = Arc::new(AtomicUsize::new(0));
    let channel: Arc<dyn Channel> = Arc::new(FakeRefreshChannel {
        refreshes: Arc::clone(&refreshes),
        sleep_ms: 0,
    });

    let got = state
        .ensure_fresh_credential(
            &channel,
            &cred,
            &test_provider(),
            json!({"access_token": "old"}),
            false,
        )
        .await
        .unwrap();

    assert_eq!(got, json!({"access_token": "new"}));
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn lost_lease_finishes_refresh_with_cas_writeback() {
    let cipher = cipher();
    let (mut state, cred, _dir) =
        state_with_cred(Arc::clone(&cipher), json!({"access_token": "old"})).await;
    state.cache = Arc::new(LostLeaseCache(MemoryCache::new()));
    let refreshes = Arc::new(AtomicUsize::new(0));
    let channel: Arc<dyn Channel> = Arc::new(FakeRefreshChannel {
        refreshes,
        sleep_ms: 20,
    });

    let got = state
        .ensure_fresh_credential(
            &channel,
            &cred,
            &test_provider(),
            json!({"access_token": "old"}),
            false,
        )
        .await
        .unwrap();

    assert_eq!(got, json!({"access_token": "new"}));
    assert_eq!(
        cipher.open(&stored_secret(&state, &cred).await).unwrap(),
        json!({"access_token": "new"})
    );
}
