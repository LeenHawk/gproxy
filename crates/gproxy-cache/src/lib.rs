//! Portable short-lived state and atomic coordination, without business entities.
//! Notifications are lossy invalidation hints. Subscribe first, reload durable
//! state on ResyncRequired/reconnect, and periodically check its revision.
//! A transport error or cancellation after dispatch does not prove no mutation
//! occurred. Non-idempotent operations are never automatically replayed here.

#![forbid(unsafe_code)]

use std::{fmt, time::Duration};

#[cfg(feature = "memory")]
mod memory;
#[cfg(feature = "memory")]
pub use memory::{MemoryCache, MemoryOptions};
#[cfg(all(feature = "redis", not(target_arch = "wasm32")))]
mod redis_backend;
#[cfg(all(feature = "redis", not(target_arch = "wasm32")))]
pub use redis_backend::{RedisCache, RedisOptions};

pub type Result<T> = std::result::Result<T, CacheError>;

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("invalid cache operation: {0}")]
    Invalid(&'static str),
    #[error("cache capacity exhausted")]
    Capacity,
    #[error("counter would underflow")]
    Underflow,
    #[error("cache state is corrupt or belongs to another implementation")]
    Corrupt,
    #[error("cache mutex poisoned")]
    Poisoned,
    #[error("notification subscription closed; resubscribe and reload durable state")]
    Closed,
    #[error("cannot generate cache token: {0}")]
    Entropy(String),
    #[cfg(all(feature = "redis", not(target_arch = "wasm32")))]
    #[error("Redis operation failed: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("cache connection timed out")]
    Timeout,
}

/// Random identity for a value generation or a permit holder. Never reused by
/// the library, even after expiry/deletion. This is not a monotonic fencing ID.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Version([u8; 32]);
impl Version {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
    #[cfg(any(
        feature = "memory",
        all(feature = "redis", not(target_arch = "wasm32"))
    ))]
    fn fresh() -> Result<Self> {
        let mut bytes = [0; 32];
        getrandom::fill(&mut bytes).map_err(|e| CacheError::Entropy(e.to_string()))?;
        Ok(Self(bytes))
    }
}
impl fmt::Debug for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Version(..)")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Entry {
    pub value: Vec<u8>,
    pub version: Version,
}
impl fmt::Debug for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entry")
            .field("bytes", &self.value.len())
            .field("version", &self.version)
            .finish()
    }
}
#[derive(Clone)]
pub struct Replacement {
    pub value: Vec<u8>,
    pub ttl: Duration,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CasOutcome {
    Applied(Option<Version>),
    Conflict,
}

/// Exact nonnegative counter, bounded by signed i64 storage. Expiry is fixed
/// when the generation is first created; later increments do not extend it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counter {
    pub value: u64,
    pub generation: Version,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IncrementOutcome {
    Applied(Counter),
    Limited { current: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notification {
    /// Application-defined invalidation payload, not a durable event log.
    Message(Vec<u8>),
    /// Initial subscription, lag or invalid message. Reload authoritative state.
    ResyncRequired,
}

#[async_trait::async_trait]
pub trait Subscription: Send {
    /// Cancel-safe for both supplied backends. Closed subscriptions must be
    /// replaced; each replacement starts with ResyncRequired.
    async fn recv(&mut self) -> Result<Notification>;
}

/// Each method is atomic for its one key. There are no cross-key transactions.
/// KV values, counters, permits and notification topics have separate domains.
#[async_trait::async_trait]
pub trait Cache: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<Entry>>;
    /// `get` for each key, answered in the order asked. Each key is read
    /// atomically on its own, exactly as `get` reads it; the batch is not a
    /// snapshot across keys. It exists so a caller that needs many keys at
    /// once pays one lock, round trip or query instead of one per key.
    async fn get_many(&self, keys: &[String]) -> Result<Vec<Option<Entry>>> {
        let mut entries = Vec::with_capacity(keys.len());
        for key in keys {
            entries.push(self.get(key).await?);
        }
        Ok(entries)
    }
    async fn put(&self, key: &str, value: Vec<u8>, ttl: Duration) -> Result<Version>;
    async fn delete(&self, key: &str) -> Result<bool>;
    /// None expects absence, including expiry. None replacement deletes.
    async fn compare_exchange(
        &self,
        key: &str,
        expected: Option<Version>,
        replacement: Option<Replacement>,
    ) -> Result<CasOutcome>;
    async fn counter(&self, key: &str) -> Result<Option<Counter>>;
    async fn increment(
        &self,
        key: &str,
        amount: u64,
        limit: u64,
        ttl: Duration,
    ) -> Result<IncrementOutcome>;
    /// Generation check prevents a late decrement touching a new window.
    /// This is not an idempotent receipt: the caller must release each charge
    /// once and must not blindly retry after an ambiguous transport failure.
    async fn decrement(
        &self,
        key: &str,
        generation: Version,
        amount: u64,
    ) -> Result<Option<Counter>>;
    /// Expiring concurrency permit. Use limit=1 for a refresh lease. All callers
    /// for a resource must use the same limit. No automatic renewal is performed.
    async fn acquire_permit(&self, key: &str, limit: u32, ttl: Duration)
    -> Result<Option<Version>>;
    async fn renew_permit(&self, key: &str, owner: Version, ttl: Duration) -> Result<bool>;
    async fn release_permit(&self, key: &str, owner: Version) -> Result<bool>;
    async fn acquire_lease(&self, key: &str, ttl: Duration) -> Result<Option<Version>> {
        self.acquire_permit(key, 1, ttl).await
    }
    async fn publish(&self, topic: &str, payload: Vec<u8>) -> Result<()>;
    /// Returns after registration. Initial ResyncRequired ensures changes
    /// before registration are recovered by an authoritative reload.
    async fn subscribe(&self, topic: &str) -> Result<Box<dyn Subscription>>;
}

#[derive(Clone, Debug)]
pub struct Limits {
    pub max_key_bytes: usize,
    pub max_value_bytes: usize,
    pub max_permits_per_key: u32,
    pub notification_capacity: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_key_bytes: 1024,
            max_value_bytes: 1024 * 1024,
            max_permits_per_key: 10_000,
            notification_capacity: 256,
        }
    }
}
#[cfg(any(
    feature = "memory",
    all(feature = "redis", not(target_arch = "wasm32"))
))]
impl Limits {
    fn validate(&self) -> Result<()> {
        if self.max_key_bytes == 0
            || self.max_value_bytes == 0
            || self.max_permits_per_key == 0
            || self.notification_capacity == 0
            || self.notification_capacity > 1_048_576
        {
            return Err(CacheError::Invalid(
                "limits must be positive; notification capacity must be <= 1048576",
            ));
        }
        Ok(())
    }
    fn key(&self, key: &str) -> Result<()> {
        if key.is_empty() || key.len() > self.max_key_bytes {
            return Err(CacheError::Invalid("empty or oversized key/topic"));
        }
        Ok(())
    }
    fn value(&self, value: &[u8]) -> Result<()> {
        if value.len() > self.max_value_bytes {
            return Err(CacheError::Invalid("oversized value/notification"));
        }
        Ok(())
    }
    fn permits(&self, limit: u32) -> Result<()> {
        if limit == 0 || limit > self.max_permits_per_key {
            return Err(CacheError::Invalid("invalid permit limit"));
        }
        Ok(())
    }
}

// Lua sorted-set timestamps use exact IEEE-754 integers. This bound leaves
// room for current Unix milliseconds and is far beyond practical cache TTLs.
#[cfg(any(
    feature = "memory",
    all(feature = "redis", not(target_arch = "wasm32"))
))]
const MAX_TTL_MS: u64 = (1_u64 << 52) - 1;
#[cfg(any(
    feature = "memory",
    all(feature = "redis", not(target_arch = "wasm32"))
))]
fn ttl_ms(ttl: Duration) -> Result<u64> {
    let ms = ttl.as_nanos().div_ceil(1_000_000);
    if ms == 0 || ms > u128::from(MAX_TTL_MS) {
        return Err(CacheError::Invalid(
            "TTL must be positive and representable in exact milliseconds",
        ));
    }
    Ok(ms as u64)
}
#[cfg(any(
    feature = "memory",
    all(feature = "redis", not(target_arch = "wasm32"))
))]
fn counter_args(amount: u64, limit: u64) -> Result<()> {
    if amount == 0 || amount > i64::MAX as u64 || limit > i64::MAX as u64 {
        return Err(CacheError::Invalid(
            "counter amounts must be positive and values <= i64::MAX",
        ));
    }
    Ok(())
}
