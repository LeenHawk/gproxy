//! In-memory [`CacheBackend`] backed by a sharded `DashMap`.

use std::collections::BTreeSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use dashmap::DashMap;

#[cfg(test)]
use super::LockAttempt;
use super::{CacheBackend, CacheError, CounterError, InvalidationHandler};

const EXPIRY_PRUNE_BATCH: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Expiration {
    deadline: Instant,
    generation: u64,
}

struct Entry {
    data: Vec<u8>,
    expiration: Option<Expiration>,
}

impl Entry {
    fn is_expired(&self) -> bool {
        self.expiration
            .is_some_and(|expiration| Instant::now() >= expiration.deadline)
    }
}

/// In-memory cache. TTL'd entries are evicted on access and amortized across
/// subsequent writes. Suitable for single-instance deployments with no
/// external dependencies.
#[derive(Default)]
pub struct MemoryCache {
    map: DashMap<String, Entry>,
    /// One current deadline per TTL-backed key. Expired one-shot keys would
    /// otherwise remain in `map` forever because reads only revisit hot keys.
    expirations: Mutex<BTreeSet<(Instant, u64, String)>>,
    next_expiration_generation: AtomicU64,
}

impl MemoryCache {
    pub fn new() -> Self {
        Self::default()
    }

    fn expiration(&self, ttl: Option<Duration>) -> Option<Expiration> {
        ttl.map(|duration| Expiration {
            deadline: Instant::now() + duration,
            generation: self
                .next_expiration_generation
                .fetch_add(1, Ordering::Relaxed),
        })
    }

    fn update_expiration(
        &self,
        key: String,
        previous: Option<Expiration>,
        current: Option<Expiration>,
    ) {
        if previous == current {
            return;
        }
        let mut expirations = self
            .expirations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(expiration) = previous {
            expirations.remove(&(expiration.deadline, expiration.generation, key.clone()));
        }
        if let Some(expiration) = current {
            expirations.insert((expiration.deadline, expiration.generation, key));
        }
    }

    /// Amortized expiry collection on writes. No background task is required,
    /// and an idle cache consumes no CPU. A bounded batch avoids a latency spike
    /// on the first write after a long idle period.
    fn prune_expired(&self) {
        let now = Instant::now();
        let expired = {
            let mut expirations = self
                .expirations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let mut keys = Vec::new();
            while expirations
                .first()
                .is_some_and(|(deadline, _, _)| *deadline <= now)
                && keys.len() < EXPIRY_PRUNE_BATCH
            {
                let (deadline, generation, key) =
                    expirations.pop_first().expect("first checked above");
                keys.push((
                    key,
                    Expiration {
                        deadline,
                        generation,
                    },
                ));
            }
            keys
        };
        for (key, expiration) in expired {
            self.map
                .remove_if(&key, |_, entry| entry.expiration == Some(expiration));
        }
    }
}

#[async_trait]
impl CacheBackend for MemoryCache {
    async fn get(&self, key: &str) -> Option<Vec<u8>> {
        let entry = self.map.get(key)?;
        if entry.is_expired() {
            let expiration = entry.expiration;
            drop(entry);
            // Re-check under the write lock so we never evict a value a
            // concurrent set() inserted between the drop and the removal.
            if self
                .map
                .remove_if(key, |_, entry| entry.expiration == expiration)
                .is_some()
            {
                self.update_expiration(key.to_owned(), expiration, None);
            }
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
        self.prune_expired();
        let key = key.to_owned();
        let expiration = self.expiration(ttl);
        let previous = self
            .map
            .insert(
                key.clone(),
                Entry {
                    data: value,
                    expiration,
                },
            )
            .and_then(|entry| entry.expiration);
        self.update_expiration(key, previous, expiration);
        Ok(())
    }

    async fn incr(
        &self,
        key: &str,
        delta: i64,
        ttl: Option<Duration>,
    ) -> Result<i64, CounterError> {
        self.prune_expired();
        let key = key.to_owned();
        let mut expiration_change = None;
        let mut entry = self.map.entry(key.clone()).or_insert_with(|| {
            let expiration = self.expiration(ttl);
            expiration_change = Some((None, expiration));
            Entry {
                data: b"0".to_vec(),
                expiration,
            }
        });
        if entry.is_expired() {
            let previous = entry.expiration;
            let expiration = self.expiration(ttl);
            entry.data = b"0".to_vec();
            entry.expiration = expiration;
            expiration_change = Some((previous, expiration));
        }
        let current: i64 = std::str::from_utf8(&entry.data)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let next = current + delta;
        entry.data = next.to_string().into_bytes();
        drop(entry);
        if let Some((previous, current)) = expiration_change {
            self.update_expiration(key, previous, current);
        }
        Ok(next)
    }

    async fn delete(&self, key: &str) {
        if let Some((key, entry)) = self.map.remove(key) {
            self.update_expiration(key, entry.expiration, None);
        }
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

    #[tokio::test]
    async fn write_prunes_unrelated_expired_keys() {
        let cache = MemoryCache::new();
        cache
            .set("expired", b"v".to_vec(), Some(Duration::from_millis(10)))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(25)).await;

        cache.set("live", b"v".to_vec(), None).await.unwrap();

        assert!(!cache.map.contains_key("expired"));
        assert!(cache.map.contains_key("live"));
        assert!(cache.expirations.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn stale_generation_cannot_prune_a_slid_value() {
        let cache = MemoryCache::new();
        cache
            .set("k", b"old".to_vec(), Some(Duration::from_millis(10)))
            .await
            .unwrap();
        let stale = cache.map.get("k").unwrap().expiration.unwrap();
        cache
            .set("k", b"new".to_vec(), Some(Duration::from_secs(1)))
            .await
            .unwrap();
        assert_eq!(cache.expirations.lock().unwrap().len(), 1);
        // Model a delayed index update from a concurrent writer. The exact
        // generation check must prevent this stale deadline removing `new`.
        cache.expirations.lock().unwrap().insert((
            stale.deadline,
            stale.generation,
            "k".to_owned(),
        ));

        tokio::time::sleep(Duration::from_millis(25)).await;
        cache.set("trigger", b"v".to_vec(), None).await.unwrap();

        assert_eq!(cache.get("k").await, Some(b"new".to_vec()));
        assert_eq!(cache.expirations.lock().unwrap().len(), 1);
    }

    /// Memory inherits the default `try_lock` (always acquired): single-instance
    /// exclusion is the caller's local mutex, so the refresh single-flight must
    /// see the lock as always acquired and proceed.
    #[tokio::test]
    async fn try_lock_default_true_on_memory() {
        let cache = MemoryCache::new();
        assert_eq!(
            cache.try_lock("lk", "owner", Duration::from_secs(30)).await,
            LockAttempt::Acquired
        );
        cache.unlock("lk", "owner").await; // no-op, must not panic
    }
}
