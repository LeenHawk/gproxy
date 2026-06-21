//! Edge (wasm32) cache backend backed by libSQL/Turso via Hrana HTTP.
//!
//! Stores key-value pairs in a `gproxy_kv` table:
//! ```sql
//! CREATE TABLE IF NOT EXISTS gproxy_kv (
//!   k TEXT PRIMARY KEY,
//!   v BLOB,
//!   expires_ms INTEGER
//! )
//! ```
//!
//! # TTL
//! TTL expiry-on-read filters `WHERE expires_ms IS NULL OR expires_ms > <now>`.
//! `now` is obtained from JS via `js_sys::Date::now()` (milliseconds since epoch),
//! mirroring [`crate::util::time`] — wasm32 has no `std::time::Instant`.
//!
//! # `incr` atomicity
//! Uses a single SQL statement with `ON CONFLICT DO UPDATE` to atomically
//! increment the stored integer value. The conflict branch is expiry-aware:
//! a live row is incremented in place (its TTL untouched — Redis INCR +
//! EXPIRE-on-create semantics), while an EXPIRED row restarts at `delta`
//! with the fresh TTL instead of resurrecting the stale count.
//!
//! Compile-checked on wasm32 only; real Turso round-trips need credentials
//! (see ignored integration tests).

use std::time::Duration;

use serde_json::Value;

use crate::store::libsql::{LibsqlClient, arg_blob, arg_integer, arg_null, arg_text};

use super::b64;
use super::{CacheBackend, CacheError, CounterError, InvalidationHandler};

/// Edge cache backend backed by a libSQL/Turso kv table.
pub struct LibsqlCache {
    client: LibsqlClient,
}

impl LibsqlCache {
    /// Create a new cache backend and ensure the kv table exists.
    pub async fn connect(
        url: String,
        token: String,
    ) -> Result<Self, crate::store::libsql::StoreError> {
        let client = LibsqlClient::new(url, token);
        client
            .execute(
                "CREATE TABLE IF NOT EXISTS gproxy_kv \
                 (k TEXT PRIMARY KEY, v BLOB, expires_ms INTEGER)",
                &[],
            )
            .await?;
        Ok(Self { client })
    }

    /// Current time in ms since epoch via JS clock (wasm32 has no Instant).
    fn now_ms() -> i64 {
        js_sys::Date::now() as i64
    }

    fn expiry(ttl: Option<Duration>) -> Value {
        match ttl {
            Some(d) if !d.is_zero() => arg_integer(Self::now_ms() + d.as_millis() as i64),
            _ => arg_null(),
        }
    }
}

#[async_trait::async_trait(?Send)]
impl CacheBackend for LibsqlCache {
    async fn get(&self, key: &str) -> Option<Vec<u8>> {
        let now = Self::now_ms();
        let result = self
            .client
            .execute(
                "SELECT v FROM gproxy_kv \
                 WHERE k = ? AND (expires_ms IS NULL OR expires_ms > ?)",
                &[arg_text(key), arg_integer(now)],
            )
            .await
            .ok()?;
        let cell = result.rows.into_iter().next()?.into_iter().next()?;
        // Hrana: BLOB → {"type":"blob","base64":"..."}, TEXT → {"type":"text","value":"..."}
        hrana_value_to_bytes(&cell)
    }

    async fn set(
        &self,
        key: &str,
        value: Vec<u8>,
        ttl: Option<Duration>,
    ) -> Result<(), CacheError> {
        let expires = Self::expiry(ttl);
        self.client
            .execute(
                "INSERT INTO gproxy_kv(k, v, expires_ms) VALUES(?, ?, ?) \
                 ON CONFLICT(k) DO UPDATE SET v = excluded.v, expires_ms = excluded.expires_ms",
                &[arg_text(key), arg_blob(&value), expires],
            )
            .await
            .map(|_| ())
            .map_err(|e| {
                tracing::error!(key, error = %e, "libsql set failed");
                CacheError
            })
    }

    async fn delete(&self, key: &str) {
        let _ = self
            .client
            .execute("DELETE FROM gproxy_kv WHERE k = ?", &[arg_text(key)])
            .await;
    }

    async fn incr(
        &self,
        key: &str,
        delta: i64,
        ttl: Option<Duration>,
    ) -> Result<i64, CounterError> {
        // NOTE: `incr` treats the stored value as an integer counter via
        // `CAST(v AS INTEGER)`.  Do NOT mix `set` (arbitrary bytes) and `incr`
        // on the same key — SQLite's CAST of a binary blob to INTEGER yields a
        // wrong value (unlike Redis, which errors).  Use distinct keys for byte
        // blobs and integer counters.
        //
        // An EXPIRED conflicting row is a dead counter: restart it at `delta`
        // with the fresh TTL instead of resurrecting the stale value (whose
        // `expires_ms` would otherwise stay in the past forever, so the
        // counter would keep accumulating while `get` already reports it gone).
        let now = Self::now_ms();
        let expires = Self::expiry(ttl);
        let result = self
            .client
            .execute(
                "INSERT INTO gproxy_kv(k, v, expires_ms) \
                 VALUES(?, CAST(? AS BLOB), ?) \
                 ON CONFLICT(k) DO UPDATE SET \
                   v = CASE WHEN gproxy_kv.expires_ms IS NOT NULL AND gproxy_kv.expires_ms <= ? \
                            THEN excluded.v \
                            ELSE CAST(CAST(gproxy_kv.v AS INTEGER) + ? AS BLOB) END, \
                   expires_ms = CASE WHEN gproxy_kv.expires_ms IS NOT NULL AND gproxy_kv.expires_ms <= ? \
                            THEN excluded.expires_ms \
                            ELSE gproxy_kv.expires_ms END \
                 RETURNING CAST(v AS INTEGER) AS val",
                &[
                    arg_text(key),
                    arg_integer(delta),
                    expires,
                    arg_integer(now),
                    arg_integer(delta),
                    arg_integer(now),
                ],
            )
            .await;
        match result {
            Ok(qr) => qr
                .rows
                .into_iter()
                .next()
                .and_then(|r| r.into_iter().next())
                .and_then(|v| hrana_value_to_i64(&v))
                .ok_or_else(|| {
                    tracing::error!("libsql incr returned no readable counter value");
                    CounterError
                }),
            Err(e) => {
                tracing::error!("libsql incr failed: {e}");
                Err(CounterError)
            }
        }
    }

    // Edge isolates re-read config frequently; cross-instance pub/sub not needed (§13).
    async fn publish(&self, _channel: &str, _payload: &[u8]) {}

    async fn subscribe(&self, _channel: &str, _handler: InvalidationHandler) {}
}

fn hrana_value_to_bytes(v: &Value) -> Option<Vec<u8>> {
    match v.get("type")?.as_str()? {
        "blob" => b64::decode(v.get("base64")?.as_str()?).ok(),
        "text" => Some(v.get("value")?.as_str()?.as_bytes().to_vec()),
        _ => None,
    }
}

fn hrana_value_to_i64(v: &Value) -> Option<i64> {
    match v.get("type")?.as_str()? {
        "integer" | "text" => v.get("value")?.as_str()?.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[wasm_bindgen_test::wasm_bindgen_test]
    #[ignore = "requires live Turso creds via GPROXY_TEST_TURSO_URL / GPROXY_TEST_TURSO_TOKEN"]
    async fn integration_get_set_incr() {
        let url = std::env::var("GPROXY_TEST_TURSO_URL").expect("GPROXY_TEST_TURSO_URL");
        let token = std::env::var("GPROXY_TEST_TURSO_TOKEN").expect("GPROXY_TEST_TURSO_TOKEN");
        let cache = LibsqlCache::connect(url, token).await.expect("connect");
        cache.set("k", b"hello".to_vec(), None).await.expect("set");
        assert_eq!(cache.get("k").await, Some(b"hello".to_vec()));
        cache.delete("k").await;
        assert_eq!(cache.get("k").await, None);
        cache.delete("ctr").await;
        assert_eq!(cache.incr("ctr", 1, None).await, Ok(1));
        assert_eq!(cache.incr("ctr", 4, None).await, Ok(5));
        cache.delete("ctr").await;
    }
}
