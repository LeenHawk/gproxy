mod in_process;
mod libsql;
#[cfg(not(target_arch = "wasm32"))]
mod redis;
mod upstash;

pub use in_process::InProcessCache;
pub use libsql::LibsqlCache;
#[cfg(not(target_arch = "wasm32"))]
pub use redis::RedisCache;
pub use upstash::UpstashCache;

fn error(backend: &'static str, operation: &'static str) -> gproxy_core::error::StoreError {
    gproxy_core::error::StoreError(format!("{backend} cache {operation} failed"))
}

fn ttl_millis(ttl: Option<std::time::Duration>) -> i64 {
    ttl.map(|value| i64::try_from(value.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}
