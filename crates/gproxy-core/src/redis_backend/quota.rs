//! Redis-backed quota with pre-hold/settle using Lua scripts for atomicity.

use std::sync::atomic::{AtomicBool, Ordering};

use redis::aio::ConnectionManager;

use gproxy_sdk::provider::backend::traits::{QuotaBackend, QuotaHold};
use gproxy_sdk::provider::backend::types::{
    BackendError, QuotaBalance, QuotaError, QuotaExhausted,
};

/// Redis-backed quota backend for multi-instance deployments.
///
/// Uses Redis hashes per identity: `{prefix}{identity_id}` with fields
/// `total`, `used`, `reserved`. Lua scripts ensure atomic check-and-reserve.
#[derive(Clone)]
pub struct RedisQuota {
    conn: ConnectionManager,
    key_prefix: String,
}

impl RedisQuota {
    /// Create a new Redis quota backend.
    pub fn new(conn: ConnectionManager) -> Self {
        Self {
            conn,
            key_prefix: "gproxy:quota:".to_string(),
        }
    }
}

/// Lua script for atomic try_reserve:
/// Returns remaining after reserve, or -1 if insufficient.
const RESERVE_SCRIPT: &str = r#"
local key = KEYS[1]
local amount = tonumber(ARGV[1])
local total = tonumber(redis.call('HGET', key, 'total')) or 0
local used = tonumber(redis.call('HGET', key, 'used')) or 0
local reserved = tonumber(redis.call('HGET', key, 'reserved')) or 0
local remaining = total - used - reserved
if remaining < amount then
    return {-1, remaining}
end
redis.call('HINCRBY', key, 'reserved', amount)
return {1, remaining - amount}
"#;

/// Lua script for atomic settle:
/// Decrements reserved, increments used by actual_cost.
const SETTLE_SCRIPT: &str = r#"
local key = KEYS[1]
local reserved_amount = tonumber(ARGV[1])
local actual_cost = tonumber(ARGV[2])
redis.call('HINCRBY', key, 'reserved', -reserved_amount)
redis.call('HINCRBY', key, 'used', actual_cost)
return 1
"#;

impl QuotaBackend for RedisQuota {
    type Hold = RedisQuotaHold;

    fn try_reserve(
        &self,
        identity_id: i64,
        estimated_cost: u64,
    ) -> impl std::future::Future<Output = Result<Self::Hold, QuotaExhausted>> + Send {
        let mut conn = self.conn.clone();
        let key = format!("{}{}", self.key_prefix, identity_id);
        let conn_for_hold = self.conn.clone();
        let prefix_for_hold = self.key_prefix.clone();

        async move {
            let result: Vec<i64> = redis::Script::new(RESERVE_SCRIPT)
                .key(&key)
                .arg(estimated_cost)
                .invoke_async(&mut conn)
                .await
                .map_err(|_e| QuotaExhausted {
                    remaining: 0,
                    requested: estimated_cost,
                })?;

            if result.first().copied().unwrap_or(-1) < 0 {
                let remaining = result.get(1).copied().unwrap_or(0).max(0) as u64;
                return Err(QuotaExhausted {
                    remaining,
                    requested: estimated_cost,
                });
            }

            Ok(RedisQuotaHold {
                conn: conn_for_hold,
                key_prefix: prefix_for_hold,
                identity_id,
                reserved_amount: estimated_cost,
                settled: AtomicBool::new(false),
            })
        }
    }

    fn balance(
        &self,
        identity_id: i64,
    ) -> impl std::future::Future<Output = Result<QuotaBalance, QuotaError>> + Send {
        let mut conn = self.conn.clone();
        let key = format!("{}{}", self.key_prefix, identity_id);

        async move {
            let (total, used, reserved): (Option<u64>, Option<u64>, Option<u64>) = redis::pipe()
                .hget(&key, "total")
                .hget(&key, "used")
                .hget(&key, "reserved")
                .query_async(&mut conn)
                .await
                .map_err(|e| {
                    QuotaError::Backend(BackendError::from(
                        Box::new(e) as Box<dyn std::error::Error + Send + Sync>
                    ))
                })?;

            Ok(QuotaBalance {
                total: total.unwrap_or(0),
                used: used.unwrap_or(0),
                reserved: reserved.unwrap_or(0),
            })
        }
    }

    fn set_quota(
        &self,
        identity_id: i64,
        total: u64,
    ) -> impl std::future::Future<Output = Result<(), QuotaError>> + Send {
        let mut conn = self.conn.clone();
        let key = format!("{}{}", self.key_prefix, identity_id);

        async move {
            let _: () = redis::cmd("HSET")
                .arg(&key)
                .arg("total")
                .arg(total)
                .query_async(&mut conn)
                .await
                .map_err(|e| {
                    QuotaError::Backend(BackendError::from(
                        Box::new(e) as Box<dyn std::error::Error + Send + Sync>
                    ))
                })?;
            Ok(())
        }
    }
}

/// Redis-backed quota hold. Settle via Lua script; Drop does conservative charge.
pub struct RedisQuotaHold {
    conn: ConnectionManager,
    key_prefix: String,
    identity_id: i64,
    reserved_amount: u64,
    settled: AtomicBool,
}

impl QuotaHold for RedisQuotaHold {
    #[allow(clippy::manual_async_fn)]
    fn settle(
        self,
        actual_cost: u64,
    ) -> impl std::future::Future<Output = Result<(), QuotaError>> + Send {
        async move {
            if !self.settled.swap(true, Ordering::AcqRel) {
                let mut conn = self.conn.clone();
                let key = format!("{}{}", self.key_prefix, self.identity_id);
                let _: i64 = redis::Script::new(SETTLE_SCRIPT)
                    .key(&key)
                    .arg(self.reserved_amount)
                    .arg(actual_cost)
                    .invoke_async(&mut conn)
                    .await
                    .map_err(|e| {
                        QuotaError::Backend(BackendError::from(
                            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
                        ))
                    })?;
            }
            Ok(())
        }
    }
}

impl Drop for RedisQuotaHold {
    fn drop(&mut self) {
        if !self.settled.swap(true, Ordering::AcqRel) {
            // Conservative charge: settle with full reserved amount.
            // Can't do async in Drop, so spawn a task.
            let mut conn = self.conn.clone();
            let key = format!("{}{}", self.key_prefix, self.identity_id);
            let reserved = self.reserved_amount;
            // Best-effort: if tokio runtime is gone, the reserved amount
            // stays in Redis until TTL or manual cleanup.
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let _: Result<i64, _> = redis::Script::new(SETTLE_SCRIPT)
                        .key(&key)
                        .arg(reserved)
                        .arg(reserved) // conservative: charge full amount
                        .invoke_async(&mut conn)
                        .await;
                });
            }
        }
    }
}
