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

/// Lua script: atomic INCR + conditional EXPIRE + limit check.
/// Returns count if under limit, -1 if over.
const ACQUIRE_SCRIPT: &str = r#"
local key = KEYS[1]
local limit = tonumber(ARGV[1])
local ttl = tonumber(ARGV[2])
local c = tonumber(redis.call('GET', key) or '0')
if c >= limit then return -1 end
c = redis.call('INCR', key)
if c == 1 then redis.call('EXPIRE', key, ttl) end
return c
"#;

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
            let result: i64 = redis::Script::new(ACQUIRE_SCRIPT)
                .key(&redis_key)
                .arg(limit)
                .arg(ttl_secs)
                .invoke_async(&mut conn)
                .await
                .map_err(|_| RateLimitExceeded {
                    retry_after: Duration::from_secs(ttl_secs),
                    window,
                })?;

            if result < 0 {
                let ttl: i64 = redis::cmd("TTL")
                    .arg(&redis_key)
                    .query_async(&mut conn)
                    .await
                    .unwrap_or(ttl_secs as i64);
                Err(RateLimitExceeded {
                    retry_after: Duration::from_secs(ttl.max(1) as u64),
                    window,
                })
            } else {
                Ok(result as u64)
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
