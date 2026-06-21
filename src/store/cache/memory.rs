//! In-memory [`CacheBackend`] backed by a sharded `DashMap`.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use dashmap::DashMap;

use super::{CacheBackend, CacheError, CounterError, InvalidationHandler};

struct Entry {
    data: Vec<u8>,
    expires_at: Option<Instant>,
}

impl Entry {
    fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|t| Instant::now() >= t)
    }
}

/// In-memory cache. TTL'd entries are evicted lazily on access. Suitable
/// for single-instance deployments with no external dependencies.
#[derive(Default)]
pub struct MemoryCache {
    map: DashMap<String, Entry>,
}

impl MemoryCache {
    pub fn new() -> Self {
        Self {
            map: DashMap::new(),
        }
    }

    fn deadline(ttl: Option<Duration>) -> Option<Instant> {
        ttl.map(|d| Instant::now() + d)
    }
}

#[async_trait]
impl CacheBackend for MemoryCache {
    async fn get(&self, key: &str) -> Option<Vec<u8>> {
        let entry = self.map.get(key)?;
        if entry.is_expired() {
            drop(entry);
            // Re-check under the write lock so we never evict a value a
            // concurrent set() inserted between the drop and the removal.
            self.map.remove_if(key, |_, v| v.is_expired());
            return None;
        }
        Some(entry.data.clone())
    }

    async fn set(
        &self,
        key: &str,
        value: Vec<u8>,
        ttl: Option<Duration>,
    ) -> Result<(), CacheError> {
        self.map.insert(
            key.to_string(),
            Entry {
                data: value,
                expires_at: Self::deadline(ttl),
            },
        );
        Ok(())
    }

    async fn incr(
        &self,
        key: &str,
        delta: i64,
        ttl: Option<Duration>,
    ) -> Result<i64, CounterError> {
        let mut entry = self.map.entry(key.to_string()).or_insert_with(|| Entry {
            data: b"0".to_vec(),
            expires_at: Self::deadline(ttl),
        });
        if entry.is_expired() {
            entry.data = b"0".to_vec();
            entry.expires_at = Self::deadline(ttl);
        }
        let current: i64 = std::str::from_utf8(&entry.data)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let next = current + delta;
        entry.data = next.to_string().into_bytes();
        Ok(next)
    }

    async fn delete(&self, key: &str) {
        self.map.remove(key);
    }

    // Single instance: no cross-instance invalidation needed.
    async fn publish(&self, _channel: &str, _payload: &[u8]) {}

    async fn subscribe(&self, _channel: &str, _handler: InvalidationHandler) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn set_get_delete_roundtrip() {
        let cache = MemoryCache::new();
        cache.set("k", b"v".to_vec(), None).await.unwrap();
        assert_eq!(cache.get("k").await, Some(b"v".to_vec()));
        cache.delete("k").await;
        assert_eq!(cache.get("k").await, None);
    }

    #[tokio::test]
    async fn incr_accumulates() {
        let cache = MemoryCache::new();
        assert_eq!(cache.incr("c", 1, None).await, Ok(1));
        assert_eq!(cache.incr("c", 4, None).await, Ok(5));
    }

    #[tokio::test]
    async fn ttl_expires() {
        let cache = MemoryCache::new();
        cache
            .set("k", b"v".to_vec(), Some(Duration::from_millis(10)))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert_eq!(cache.get("k").await, None);
    }

    /// Memory inherits the default `try_lock` (always `true`): single-instance
    /// exclusion is the caller's local mutex, so the refresh single-flight must
    /// see the lock as always acquired and proceed.
    #[tokio::test]
    async fn try_lock_default_true_on_memory() {
        let cache = MemoryCache::new();
        assert!(cache.try_lock("lk", Duration::from_secs(30)).await);
        cache.unlock("lk").await; // no-op, must not panic
    }
}
