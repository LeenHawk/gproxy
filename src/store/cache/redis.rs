//! Redis-backed [`CacheBackend`] using a multiplexed connection manager.

use std::time::Duration;

use async_trait::async_trait;
use redis::AsyncCommands;

use super::{CacheBackend, CacheError, CounterError, InvalidationHandler};

/// First reconnect backoff after a dropped subscription.
const RECONNECT_BASE: Duration = Duration::from_millis(500);
/// Maximum reconnect backoff (exponential growth is capped here).
const RECONNECT_CAP: Duration = Duration::from_secs(30);
/// A subscription that stays up at least this long is treated as healthy, so
/// the next drop restarts backoff from [`RECONNECT_BASE`] rather than the cap.
const HEALTHY_AFTER: Duration = Duration::from_secs(60);
/// Quiet period that ends a coalesced burst: once no new invalidation arrives
/// within this window, the pending reload fires.
const COALESCE_WINDOW: Duration = Duration::from_millis(50);
/// Hard cap on how long a single burst is coalesced, so a steady stream of
/// invalidations can't starve the reload indefinitely.
const COALESCE_MAX: Duration = Duration::from_millis(500);

/// Exponential reconnect backoff for attempt `n` (0-based): `0.5s * 2^n`,
/// capped at [`RECONNECT_CAP`]. Saturates instead of overflowing on large `n`.
fn backoff_delay(attempt: u32) -> Duration {
    RECONNECT_BASE
        .saturating_mul(1u32.checked_shl(attempt).unwrap_or(u32::MAX))
        .min(RECONNECT_CAP)
}

/// Redis-backed cache. Requires a running Redis server.
///
/// Uses [`redis::aio::ConnectionManager`] for automatic reconnection and
/// multiplexed access — safe to clone and share across tasks.
///
/// # TTL-on-create semantics
///
/// `incr`'s TTL is applied only when the key did not previously exist, detected
/// via a preceding `EXISTS` call inside the Lua script. This ensures the window
/// expiry is never reset by subsequent increments (unlike the old `v == delta`
/// heuristic, which misfired whenever a live counter happened to equal `delta`).
pub struct RedisCache {
    /// Kept for opening dedicated pub/sub connections (a `ConnectionManager`
    /// multiplexes and cannot enter subscribe mode).
    client: redis::Client,
    cm: redis::aio::ConnectionManager,
}

impl RedisCache {
    /// Open a connection manager to the Redis server at `url`.
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        let client = redis::Client::open(url)
            .map_err(|e| anyhow::anyhow!("redis client open failed: {e}"))?;
        let cm = redis::aio::ConnectionManager::new(client.clone())
            .await
            .map_err(|e| anyhow::anyhow!("redis connection manager failed: {e}"))?;
        Ok(Self { client, cm })
    }

    /// Verify connectivity with a `PING`.
    pub async fn health(&self) -> anyhow::Result<()> {
        let mut cm = self.cm.clone();
        let _: String = redis::cmd("PING")
            .query_async(&mut cm)
            .await
            .map_err(|e| anyhow::anyhow!("redis ping failed: {e}"))?;
        Ok(())
    }

    /// Run one connect → subscribe → consume cycle, coalescing bursts before
    /// each `handler` call. Returns `true` if the connection stayed up past
    /// [`HEALTHY_AFTER`] (so the caller resets its backoff), `false` if it
    /// failed fast or never connected (so the caller backs off).
    async fn run_subscription(&self, channel: &str, handler: &InvalidationHandler) -> bool {
        use futures_util::StreamExt;

        let mut pubsub = match self.client.get_async_pubsub().await {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, channel, "redis pubsub connect failed");
                return false;
            }
        };
        if let Err(e) = pubsub.subscribe(channel).await {
            tracing::warn!(error = %e, channel, "redis subscribe failed");
            return false;
        }
        tracing::info!(channel, "redis invalidation subscription established");
        let started = std::time::Instant::now();
        let mut stream = pubsub.on_message();

        while let Some(msg) = stream.next().await {
            // Coalesce a burst: keep the latest payload, then drain any further
            // messages arriving within the (extending) coalesce window so N
            // rapid writes fire `handler` once.
            let mut payload = msg.get_payload_bytes().to_vec();
            let deadline = std::time::Instant::now() + COALESCE_MAX;
            loop {
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                let window = COALESCE_WINDOW.min(remaining);
                if window.is_zero() {
                    break;
                }
                match tokio::time::timeout(window, stream.next()).await {
                    Ok(Some(next)) => payload = next.get_payload_bytes().to_vec(),
                    // Quiet window elapsed, or stream ended: stop draining.
                    Ok(None) | Err(_) => break,
                }
            }
            handler(payload);
        }

        tracing::warn!(channel, "redis subscription ended (connection dropped)");
        started.elapsed() >= HEALTHY_AFTER
    }
}

#[async_trait]
impl CacheBackend for RedisCache {
    async fn get(&self, key: &str) -> Option<Vec<u8>> {
        let mut cm = self.cm.clone();
        cm.get::<_, Option<Vec<u8>>>(key).await.ok().flatten()
    }

    async fn set(
        &self,
        key: &str,
        value: Vec<u8>,
        ttl: Option<Duration>,
    ) -> Result<(), CacheError> {
        let mut cm = self.cm.clone();
        let result: Result<(), _> = if let Some(d) = ttl {
            // Duration::ZERO treated as no-expiry (PSETEX 0 is rejected by Redis).
            let ms = u64::try_from(d.as_millis()).unwrap_or(u64::MAX);
            if ms == 0 {
                cm.set(key, value).await
            } else {
                cm.pset_ex(key, value, ms).await
            }
        } else {
            cm.set(key, value).await
        };
        result.map_err(|e| {
            tracing::error!(key, error = %e, "redis set failed");
            CacheError
        })
    }

    async fn delete(&self, key: &str) {
        let mut cm = self.cm.clone();
        let _: Result<u64, _> = cm.del(key).await;
    }

    /// Atomically increment `key` by `delta`. TTL is applied only on creation.
    ///
    /// Uses an EXISTS-before-INCRBY Lua script so the TTL is set in the same
    /// round-trip as INCRBY, and only when the key did not previously exist —
    /// not whenever the counter value happens to equal `delta`. This prevents
    /// spurious TTL resets on live counters (e.g. two increments of the same
    /// delta on a pre-existing key).
    ///
    /// Any Redis/Lua error is logged and surfaced as [`CounterError`] so policy
    /// callers can fail closed instead of mistaking the outage for count 0.
    async fn incr(
        &self,
        key: &str,
        delta: i64,
        ttl: Option<Duration>,
    ) -> Result<i64, CounterError> {
        // local exists = redis.call('EXISTS', KEYS[1])
        // local v = redis.call('INCRBY', KEYS[1], ARGV[1])
        // if exists == 0 and tonumber(ARGV[2]) > 0 then
        //   redis.call('PEXPIRE', KEYS[1], ARGV[2])
        // end
        // return v
        static INCR_SCRIPT: std::sync::OnceLock<redis::Script> = std::sync::OnceLock::new();
        let script = INCR_SCRIPT.get_or_init(|| {
            redis::Script::new(
                "local exists = redis.call('EXISTS', KEYS[1])\n\
                 local v = redis.call('INCRBY', KEYS[1], ARGV[1])\n\
                 if exists == 0 and tonumber(ARGV[2]) > 0 then\n\
                   redis.call('PEXPIRE', KEYS[1], ARGV[2])\n\
                 end\n\
                 return v",
            )
        });

        let ttl_ms: i64 = ttl
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        let mut cm = self.cm.clone();
        script
            .key(key)
            .arg(delta)
            .arg(ttl_ms)
            .invoke_async::<i64>(&mut cm)
            .await
            .map_err(|e| {
                tracing::error!("redis incr failed: {e}");
                CounterError
            })
    }

    /// Publish `payload` to `channel`. Best-effort: a failed publish is logged
    /// but never propagated, so an invalidation hiccup can't break the caller.
    async fn publish(&self, channel: &str, payload: &[u8]) {
        let mut cm = self.cm.clone();
        let r: Result<i64, _> = redis::cmd("PUBLISH")
            .arg(channel)
            .arg(payload)
            .query_async(&mut cm)
            .await;
        if let Err(e) = r {
            tracing::warn!(error = %e, channel, "redis publish failed");
        }
    }

    /// Subscribe to `channel`, invoking `handler` for invalidation events until
    /// the caller drops the task. Blocks for the lifetime of the subscription,
    /// so the caller is expected to spawn this.
    ///
    /// # Reconnection
    ///
    /// The connection is re-established with exponential backoff
    /// ([`RECONNECT_BASE`] → [`RECONNECT_CAP`]) whenever it fails to open or the
    /// message stream ends (a dropped connection yields `None`). The loop never
    /// terminates on a transient error, so the listener survives Redis
    /// restarts / network blips. Backoff resets after a connection that stayed
    /// up past [`HEALTHY_AFTER`].
    ///
    /// # Coalescing
    ///
    /// A burst of N invalidation messages collapses into a single `handler`
    /// call: after the first message, the loop drains every message that
    /// arrives within [`COALESCE_WINDOW`] (extending the window on each new
    /// message up to [`COALESCE_MAX`]), then invokes `handler` once with the
    /// most-recent payload. This turns N rapid config writes into one snapshot
    /// rebuild instead of N.
    async fn subscribe(&self, channel: &str, handler: InvalidationHandler) {
        let mut attempt: u32 = 0;
        loop {
            match self.run_subscription(channel, &handler).await {
                // `true` => the subscription stayed up long enough to be
                // considered healthy; reset the backoff for the next cycle.
                true => attempt = 0,
                false => {
                    let delay = backoff_delay(attempt);
                    attempt = attempt.saturating_add(1);
                    tracing::warn!(
                        channel,
                        delay_ms = delay.as_millis() as u64,
                        "redis subscription down; reconnecting after backoff"
                    );
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    /// Acquire a best-effort distributed lock via `SET key 1 NX PX <ms>`.
    /// Returns `true` only when the key was newly set (`OK`); a held lock
    /// (`nil`) or any error returns `false`, so callers fall back to their
    /// local mutex / bounded wait rather than blocking.
    async fn try_lock(&self, key: &str, ttl: Duration) -> bool {
        let mut cm = self.cm.clone();
        let ms = ttl.as_millis().max(1) as u64;
        let res: redis::RedisResult<Option<String>> = redis::cmd("SET")
            .arg(key)
            .arg("1")
            .arg("NX")
            .arg("PX")
            .arg(ms)
            .query_async(&mut cm)
            .await;
        matches!(res, Ok(Some(_)))
    }

    /// Release a lock acquired via [`RedisCache::try_lock`] with a plain `DEL`.
    /// The short lock TTL bounds a stuck lock if this never runs; a
    /// token-scoped check-and-delete is a noted hardening for later.
    async fn unlock(&self, key: &str) {
        let mut cm = self.cm.clone();
        let _: redis::RedisResult<i64> = redis::cmd("DEL").arg(key).query_async(&mut cm).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_exponentially_then_caps() {
        assert_eq!(backoff_delay(0), Duration::from_millis(500));
        assert_eq!(backoff_delay(1), Duration::from_secs(1));
        assert_eq!(backoff_delay(2), Duration::from_secs(2));
        assert_eq!(backoff_delay(6), Duration::from_secs(30)); // 32s -> capped
        // Large attempt counts must saturate to the cap, never overflow/panic.
        assert_eq!(backoff_delay(100), RECONNECT_CAP);
        assert_eq!(backoff_delay(u32::MAX), RECONNECT_CAP);
    }

    // This test requires a live Redis server and is skipped in normal CI.
    // Set GPROXY_TEST_REDIS_URL (e.g. redis://127.0.0.1:6379) to run it.
    #[tokio::test]
    #[ignore = "requires live Redis server via GPROXY_TEST_REDIS_URL"]
    async fn integration_set_get_incr() {
        let url = std::env::var("GPROXY_TEST_REDIS_URL")
            .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let cache = RedisCache::connect(&url).await.expect("connect");
        cache.health().await.expect("health");

        let key = "gproxy_test_integration";
        cache.set(key, b"hello".to_vec(), None).await.expect("set");
        assert_eq!(cache.get(key).await, Some(b"hello".to_vec()));

        cache.delete(key).await;
        assert_eq!(cache.get(key).await, None);

        let ctr = "gproxy_test_counter";
        cache.delete(ctr).await;
        assert_eq!(cache.incr(ctr, 1, None).await, Ok(1));
        assert_eq!(cache.incr(ctr, 4, None).await, Ok(5));
        cache.delete(ctr).await;
    }

    /// Round-trips one message through real PUBLISH/SUBSCRIBE. Skips (does not
    /// fail) when no Redis is reachable, so it's safe to run unconditionally in
    /// CI without a Redis service.
    #[tokio::test]
    async fn redis_publish_subscribe_roundtrip() {
        let Ok(cache) = RedisCache::connect("redis://127.0.0.1:6379").await else {
            eprintln!("skipping: no redis at 127.0.0.1:6379");
            return;
        };
        if cache.health().await.is_err() {
            eprintln!("skipping: redis unreachable");
            return;
        }

        let channel = "gproxy_test_pubsub";
        let (tx, rx) = tokio::sync::oneshot::channel::<Vec<u8>>();
        let tx = std::sync::Mutex::new(Some(tx));
        let sub = {
            let cache = RedisCache::connect("redis://127.0.0.1:6379")
                .await
                .expect("subscriber connect");
            tokio::spawn(async move {
                cache
                    .subscribe(
                        channel,
                        Box::new(move |payload| {
                            if let Some(tx) = tx.lock().unwrap().take() {
                                let _ = tx.send(payload);
                            }
                        }),
                    )
                    .await;
            })
        };

        // Give the subscription time to establish before publishing.
        tokio::time::sleep(Duration::from_millis(200)).await;
        cache.publish(channel, b"ping").await;

        let got = tokio::time::timeout(Duration::from_secs(1), rx)
            .await
            .expect("handler did not receive within 1s")
            .expect("sender dropped");
        assert_eq!(got, b"ping");
        sub.abort();
    }
}
