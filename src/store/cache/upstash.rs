//! Edge (wasm32) cache backend backed by Upstash Redis REST API.
//!
//! Issues Redis commands via `POST {url}` with `Authorization: Bearer {token}`.
//! Request body is a JSON array: `["COMMAND", "arg1", ...]`.
//! Response: `{"result": ...}` on success, `{"error": "..."}` on failure.
//!
//! # Byte safety
//! Cache values (arbitrary bytes) are base64-encoded for storage because the
//! Upstash REST API is string-only. `get` base64-decodes the result transparently.
//!
//! # TTL semantics
//! `set` uses `SET key b64value PX <ms>` (TTL) or `SET key b64value` (no TTL).
//! `incr` uses `INCRBY`; if the result equals `delta` (key freshly created),
//! issues `PEXPIRE key <ms>`. This is two REST calls — not atomic — but it is
//! the only option with the REST API. The race window is benign.
//!
//! Compile-checked on wasm32 only; real Upstash round-trips need credentials
//! (see ignored integration tests).

use std::time::Duration;

use js_sys::{Uint8Array, global};
use serde_json::{Value, json};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Headers, Request, RequestInit, Response, WorkerGlobalScope};

use super::b64;
use super::{CacheBackend, CacheError, CounterError, InvalidationHandler};

/// Edge cache backend backed by Upstash Redis REST.
pub struct UpstashCache {
    url: String,
    token: String,
}

fn js_err(e: wasm_bindgen::JsValue) -> String {
    format!("{e:?}")
}

impl UpstashCache {
    pub fn new(url: String, token: String) -> Self {
        Self { url, token }
    }

    async fn cmd(&self, args: &[Value]) -> Option<Value> {
        let body = serde_json::to_string(args).ok()?;
        let js_headers = Headers::new().map_err(js_err).ok()?;
        js_headers
            .append("Content-Type", "application/json")
            .map_err(js_err)
            .ok()?;
        js_headers
            .append("Authorization", &format!("Bearer {}", self.token))
            .map_err(js_err)
            .ok()?;

        let bytes = body.as_bytes();
        let arr = Uint8Array::new_with_length(bytes.len() as u32);
        arr.copy_from(bytes);

        let init = RequestInit::new();
        init.set_method("POST");
        init.set_headers_headers(&js_headers);
        init.set_body_opt_u8_array(Some(&arr));

        let js_req = Request::new_with_str_and_init(&self.url, &init)
            .map_err(js_err)
            .ok()?;
        let scope = global().unchecked_into::<WorkerGlobalScope>();
        let resp_val = JsFuture::from(scope.fetch_with_request(&js_req))
            .await
            .map_err(js_err)
            .ok()?;
        let js_resp: Response = resp_val.unchecked_into();

        let buf_val = JsFuture::from(js_resp.array_buffer().map_err(js_err).ok()?)
            .await
            .map_err(js_err)
            .ok()?;
        let body_bytes = Uint8Array::new(&buf_val).to_vec();

        let parsed: Value = serde_json::from_slice(&body_bytes).ok()?;
        if let Some(err) = parsed.get("error") {
            tracing::error!("upstash error: {err}");
            return None;
        }
        parsed.get("result").cloned()
    }
}

#[async_trait::async_trait(?Send)]
impl CacheBackend for UpstashCache {
    async fn get(&self, key: &str) -> Option<Vec<u8>> {
        // Result is a JSON string; stored as base64.
        let result = self.cmd(&[json!("GET"), json!(key)]).await?;
        b64::decode(result.as_str()?).ok()
    }

    async fn set(
        &self,
        key: &str,
        value: Vec<u8>,
        ttl: Option<Duration>,
    ) -> Result<(), CacheError> {
        let enc = b64::encode(&value);
        let cmd: Vec<Value> = match ttl {
            Some(d) if !d.is_zero() => {
                vec![
                    json!("SET"),
                    json!(key),
                    json!(enc),
                    json!("PX"),
                    json!(d.as_millis() as u64),
                ]
            }
            _ => vec![json!("SET"), json!(key), json!(enc)],
        };
        // `cmd` logs the failure detail and yields None on any error.
        self.cmd(&cmd).await.map(|_| ()).ok_or(CacheError)
    }

    async fn delete(&self, key: &str) {
        let _ = self.cmd(&[json!("DEL"), json!(key)]).await;
    }

    async fn incr(
        &self,
        key: &str,
        delta: i64,
        ttl: Option<Duration>,
    ) -> Result<i64, CounterError> {
        let result = self.cmd(&[json!("INCRBY"), json!(key), json!(delta)]).await;
        let new_val = match result.as_ref().and_then(Value::as_i64) {
            Some(v) => v,
            None => {
                tracing::error!("upstash incrby failed");
                return Err(CounterError);
            }
        };
        // Set TTL only when the key was freshly created (value == delta).
        // This heuristic assumes positive deltas (typical rate-limit counters).
        // A negative delta that coincidentally equals `delta` would spuriously
        // refresh the TTL, but that case is benign for current usage.
        if new_val == delta
            && let Some(d) = ttl
            && !d.is_zero()
        {
            let _ = self
                .cmd(&[json!("PEXPIRE"), json!(key), json!(d.as_millis() as u64)])
                .await;
        }
        Ok(new_val)
    }

    // Edge isolates re-read config frequently; cross-instance pub/sub not needed (§13).
    async fn publish(&self, _channel: &str, _payload: &[u8]) {}

    async fn subscribe(&self, _channel: &str, _handler: InvalidationHandler) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[wasm_bindgen_test::wasm_bindgen_test]
    #[ignore = "requires Upstash creds via GPROXY_TEST_UPSTASH_URL / GPROXY_TEST_UPSTASH_TOKEN"]
    async fn integration_get_set_incr() {
        let url = std::env::var("GPROXY_TEST_UPSTASH_URL").expect("GPROXY_TEST_UPSTASH_URL");
        let token = std::env::var("GPROXY_TEST_UPSTASH_TOKEN").expect("GPROXY_TEST_UPSTASH_TOKEN");
        let cache = UpstashCache::new(url, token);
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
