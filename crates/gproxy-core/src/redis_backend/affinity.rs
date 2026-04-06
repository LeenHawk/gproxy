//! Redis-backed affinity bindings using SET with EX (TTL).

use std::time::Duration;

use redis::AsyncCommands;
use redis::aio::ConnectionManager;

use gproxy_sdk::provider::backend::traits::AffinityBackend;
use gproxy_sdk::provider::backend::types::BackendError;

/// Redis-backed affinity backend for multi-instance deployments.
///
/// Stores bindings as simple key-value pairs with TTL expiration.
/// `SET {prefix}{key} {credential_id} EX {ttl_secs}`
#[derive(Clone)]
pub struct RedisAffinity {
    conn: ConnectionManager,
    key_prefix: String,
}

impl RedisAffinity {
    /// Create a new Redis affinity backend.
    pub fn new(conn: ConnectionManager) -> Self {
        Self {
            conn,
            key_prefix: "gproxy:aff:".to_string(),
        }
    }
}

impl AffinityBackend for RedisAffinity {
    fn get_binding(
        &self,
        key: &str,
    ) -> impl std::future::Future<Output = Option<String>> + Send {
        let mut conn = self.conn.clone();
        let redis_key = format!("{}{}", self.key_prefix, key);

        async move {
            conn.get::<_, Option<String>>(&redis_key)
                .await
                .ok()
                .flatten()
        }
    }

    fn set_binding(
        &self,
        key: &str,
        credential_id: &str,
        ttl: Duration,
    ) -> impl std::future::Future<Output = Result<(), BackendError>> + Send {
        let mut conn = self.conn.clone();
        let redis_key = format!("{}{}", self.key_prefix, key);
        let value = credential_id.to_string();
        let ttl_secs = ttl.as_secs().max(1);

        async move {
            let _: () = redis::cmd("SET")
                .arg(&redis_key)
                .arg(&value)
                .arg("EX")
                .arg(ttl_secs)
                .query_async(&mut conn)
                .await
                .map_err(|e| BackendError::from(
                    Box::new(e) as Box<dyn std::error::Error + Send + Sync>
                ))?;
            Ok(())
        }
    }

    fn remove_binding(
        &self,
        key: &str,
    ) -> impl std::future::Future<Output = Result<(), BackendError>> + Send {
        let mut conn = self.conn.clone();
        let redis_key = format!("{}{}", self.key_prefix, key);

        async move {
            let _: () = conn.del(&redis_key)
                .await
                .map_err(|e| BackendError::from(
                    Box::new(e) as Box<dyn std::error::Error + Send + Sync>
                ))?;
            Ok(())
        }
    }
}
