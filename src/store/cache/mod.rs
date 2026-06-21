//! Pluggable cache backend abstraction.
//!
//! Native implementations:
//! - [`MemoryCache`] — in-process, no external dependencies; single-instance deployments.
//! - [`RedisCache`] — Redis-backed; multi-instance / shared cache.
//!
//! Edge (wasm32) implementations:
//! - [`LibsqlCache`] — libSQL/Turso HTTP-backed kv table.
//! - [`UpstashCache`] — Upstash Redis REST API.
//!
//! Business code depends only on [`CacheBackend`]; the concrete impl is
//! selected at startup based on `CacheConfig`.

use std::time::Duration;

#[cfg(all(not(target_arch = "wasm32"), feature = "cache-memory"))]
pub mod memory;
#[cfg(all(not(target_arch = "wasm32"), feature = "cache-redis"))]
pub mod redis;

#[cfg(any(
    test,
    all(
        target_arch = "wasm32",
        any(feature = "cache-libsql", feature = "cache-upstash")
    )
))]
pub mod b64;
#[cfg(all(target_arch = "wasm32", feature = "cache-libsql"))]
pub mod libsql;
#[cfg(all(target_arch = "wasm32", feature = "cache-upstash"))]
pub mod upstash;

#[cfg(all(not(target_arch = "wasm32"), feature = "cache-memory"))]
pub use memory::MemoryCache;
#[cfg(all(not(target_arch = "wasm32"), feature = "cache-redis"))]
pub use redis::RedisCache;

#[cfg(all(target_arch = "wasm32", feature = "cache-libsql"))]
pub use libsql::LibsqlCache;
#[cfg(all(target_arch = "wasm32", feature = "cache-upstash"))]
pub use upstash::UpstashCache;

/// Single Redis pub/sub channel for control-plane invalidation. A message on
/// it tells every instance to `reload_snapshot` (§7.2). Payload is a hint
/// (`cred:{id}` / `config`); the listener reloads the whole snapshot.
pub const INVALIDATE_CHANNEL: &str = "gproxy:invalidate";

/// Cache key holding the monotonically-increasing control-plane config
/// version (§7.2). [`broadcast`](crate::app::invalidation::broadcast) bumps it
/// alongside the pub/sub message; edge isolates (whose `subscribe` is a no-op)
/// poll it with a short throttle and lazily rebuild their snapshot when it
/// moves.
pub const CONFIG_VERSION_KEY: &str = "gproxy:cfg-version";

/// Handler invoked for each message received on a subscribed channel.
///
/// Boxed to keep [`CacheBackend`] object-safe. `Send + Sync` on native; unbounded
/// on wasm (single-threaded edge runtimes).
#[cfg(not(target_arch = "wasm32"))]
pub type InvalidationHandler = Box<dyn Fn(Vec<u8>) + Send + Sync>;
/// Handler invoked for each message received on a subscribed channel.
#[cfg(target_arch = "wasm32")]
pub type InvalidationHandler = Box<dyn Fn(Vec<u8>)>;

/// A counter operation failed at the backend (network/db error; details are
/// logged by the backend). Callers enforcing security or quota policy (login
/// throttle, rate limits, quota admission, credential budgets) MUST fail
/// CLOSED on this — never treat it as "0 increments seen".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CounterError;

impl std::fmt::Display for CounterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cache counter backend unavailable")
    }
}

impl std::error::Error for CounterError {}

/// A cache write failed at the backend (network/db error; details are logged
/// by the backend). Callers whose correctness depends on the value actually
/// being stored (admin session / login-flow state) MUST propagate this instead
/// of pretending success; best-effort callers (affinity hints, TTL slides)
/// may ignore it explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheError;

impl std::fmt::Display for CacheError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cache backend write failed")
    }
}

impl std::error::Error for CacheError {}

/// A pluggable cache backend.
///
/// Native impls: [`MemoryCache`] (in-process) and [`RedisCache`] (Redis-backed).
/// Edge impls: [`LibsqlCache`] (libSQL/Turso) and [`UpstashCache`] (Upstash REST).
/// Business code calls only this trait.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait CacheBackend: Send + Sync {
    /// Fetch raw bytes for `key`, or `None` if absent or expired.
    async fn get(&self, key: &str) -> Option<Vec<u8>>;

    /// Store `value` under `key` with an optional time-to-live.
    ///
    /// A backend failure is `Err(CacheError)` — callers that hand out tokens
    /// referencing the stored value must fail the operation, not the lookup.
    async fn set(&self, key: &str, value: Vec<u8>, ttl: Option<Duration>)
    -> Result<(), CacheError>;

    /// Atomically add `delta` to the integer at `key` (missing = 0) and
    /// return the new value. Used by rate-limit / quota counters.
    /// The `ttl` is applied only when the key does not yet exist (or has
    /// expired); it is NOT refreshed on an already-live key (matches Redis
    /// INCR + EXPIRE-on-create semantics).
    ///
    /// A backend failure is `Err(CounterError)` — the caller decides the
    /// policy: allow/deny gates fail closed, best-effort recording ignores it.
    async fn incr(&self, key: &str, delta: i64, ttl: Option<Duration>)
    -> Result<i64, CounterError>;

    /// Remove `key` if present.
    async fn delete(&self, key: &str);

    /// Publish an invalidation `payload` to `channel` for other instances.
    ///
    /// Single-instance backends (memory) and edge runtimes are no-ops. Real
    /// cross-instance pub/sub lands in the multi-instance phase.
    async fn publish(&self, channel: &str, payload: &[u8]);

    /// Subscribe to `channel`, invoking `handler` for each future message until
    /// the backend connection drops. memory/edge are no-ops (single instance
    /// needs no cross-instance invalidation). Lands in the multi-instance phase.
    async fn subscribe(&self, channel: &str, handler: InvalidationHandler);

    /// Acquire a best-effort distributed lock (redis `SET NX PX`). Returns
    /// `true` if acquired. Default `true` — single-instance backends rely on
    /// the caller's local mutex; only redis needs cross-instance exclusion.
    async fn try_lock(&self, _key: &str, _ttl: Duration) -> bool {
        true
    }

    /// Release a lock acquired via [`CacheBackend::try_lock`]. Default no-op.
    async fn unlock(&self, _key: &str) {}
}
