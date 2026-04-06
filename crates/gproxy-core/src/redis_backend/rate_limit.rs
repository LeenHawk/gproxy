//! Redis-backed rate limiter using INCR + EXPIRE for sliding windows.

use std::time::Duration;

use redis::AsyncCommands;
use redis::aio::ConnectionManager;

use gproxy_sdk::provider::backend::traits::RateLimitBackend;
use gproxy_sdk::provider::backend::types::{RateLimitExceeded, RateLimitWindow};

/// Redis-backed rate limiter for multi-instance deployments.
///
/// Uses Redis INCR + EXPIRE for atomic window-based counting.
/// Each (key, window_epoch) pair maps to a Redis key with TTL.
#[derive(Clone)]
pub struct RedisRateLimit {
    conn: ConnectionManager,
    key_prefix: String,
}

impl RedisRateLimit {
    /// Create a new Redis rate limiter.
    pub fn new(conn: ConnectionManager) -> Self {
        Self {
            conn,
            key_prefix: "gproxy:rl:".to_string(),
        }
    }

    /// Create with a custom key prefix.
    pub fn with_prefix(conn: ConnectionManager, prefix: impl Into<String>) -> Self {
        Self {
            conn,
            key_prefix: prefix.into(),
        }
    }
}

impl RateLimitBackend for RedisRateLimit {
    fn try_acquire(
        &self,
        key: &str,
        window: RateLimitWindow,
    ) -> impl std::future::Future<Output = Result<u64, RateLimitExceeded>> + Send {
        let mut conn = self.conn.clone();
        let redis_key = format!(
            "{}{}:{}",
            self.key_prefix,
            key,
            window_epoch(window)
        );
        let limit = window_limit(window);
        let ttl_secs = window_seconds(window);

        async move {
            // Atomic INCR — creates key with value 1 if it doesn't exist
            let count: u64 = redis::cmd("INCR")
                .arg(&redis_key)
                .query_async(&mut conn)
                .await
                .map_err(|_| RateLimitExceeded {
                    retry_after: Duration::from_secs(ttl_secs),
                    window,
                })?;

            // Set TTL on first increment (when count == 1)
            if count == 1 {
                let _: () = conn
                    .expire(&redis_key, ttl_secs as i64)
                    .await
                    .unwrap_or(());
            }

            if count > limit {
                // Over limit — get TTL for retry-after
                let ttl: i64 = conn.ttl(&redis_key).await.unwrap_or(ttl_secs as i64);
                Err(RateLimitExceeded {
                    retry_after: Duration::from_secs(ttl.max(1) as u64),
                    window,
                })
            } else {
                Ok(count)
            }
        }
    }

    fn current_count(
        &self,
        key: &str,
        window: RateLimitWindow,
    ) -> impl std::future::Future<Output = u64> + Send {
        let mut conn = self.conn.clone();
        let redis_key = format!(
            "{}{}:{}",
            self.key_prefix,
            key,
            window_epoch(window)
        );

        async move {
            conn.get::<_, Option<u64>>(&redis_key)
                .await
                .ok()
                .flatten()
                .unwrap_or(0)
        }
    }
}

fn window_epoch(window: RateLimitWindow) -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    now / window_seconds(window)
}

fn window_seconds(window: RateLimitWindow) -> u64 {
    match window {
        RateLimitWindow::PerMinute { .. } => 60,
        RateLimitWindow::PerDay { .. } => 86_400,
    }
}

fn window_limit(window: RateLimitWindow) -> u64 {
    match window {
        RateLimitWindow::PerMinute { limit } | RateLimitWindow::PerDay { limit } => limit,
    }
}
