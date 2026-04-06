//! Redis-backed implementations of the backend traits.
//!
//! These implementations use Redis for cross-instance state sharing.
//! Enable with the `redis` feature flag on gproxy-core.
//!
//! Each backend uses atomic Redis operations (INCR, Lua scripts) to
//! ensure correct behavior under concurrent access from multiple instances.

mod rate_limit;
mod quota;
mod affinity;

pub use rate_limit::RedisRateLimit;
pub use quota::{RedisQuota, RedisQuotaHold};
pub use affinity::RedisAffinity;
