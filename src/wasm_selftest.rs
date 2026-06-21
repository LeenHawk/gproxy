//! wasm32 self-test export, exercised by the Deno harness (`selftest.ts`).
//!
//! Drives the real edge storage backends (libSQL/Turso over Hrana HTTP and
//! Upstash Redis REST) against live endpoints, proving the actual web-sys
//! fetch plumbing + request building + response parsing works end-to-end —
//! not just the wire protocol. Never panics: every step's outcome (ok/err or
//! assertion result) is captured into a human-readable summary string.
//!
//! Compiled only on wasm32; not part of the production binary.

use wasm_bindgen::prelude::*;

use crate::store::cache::{CacheBackend, LibsqlCache, UpstashCache};
use crate::store::persistence::{LibsqlPersistence, PersistenceBackend};

/// Run the edge storage self-test against live Turso + Upstash endpoints.
///
/// Returns a multi-line summary, one line per step (e.g. `libsql.health: OK`,
/// `upstash.incr: 6`, or `libsql.get: ERR <msg>`).
#[wasm_bindgen]
pub async fn storage_selftest(
    turso_url: String,
    turso_token: String,
    upstash_url: String,
    upstash_token: String,
) -> String {
    let mut out: Vec<String> = Vec::new();

    libsql_persistence(&mut out, turso_url.clone(), turso_token.clone()).await;
    libsql_cache(&mut out, turso_url, turso_token).await;
    upstash_cache(&mut out, upstash_url, upstash_token).await;

    out.join("\n")
}

/// LibsqlPersistence: connect (ensures schema) then health() → record ok/err.
async fn libsql_persistence(out: &mut Vec<String>, url: String, token: String) {
    let backend = match LibsqlPersistence::connect(url, token).await {
        Ok(b) => b,
        Err(e) => {
            out.push(format!("libsql.connect: ERR {e}"));
            return;
        }
    };
    match backend.health().await {
        Ok(()) => out.push("libsql.health: OK".into()),
        Err(e) => out.push(format!("libsql.health: ERR {e}")),
    }
}

/// LibsqlCache: connect → set/get/incr/delete/get round-trip.
async fn libsql_cache(out: &mut Vec<String>, url: String, token: String) {
    let cache = match LibsqlCache::connect(url, token).await {
        Ok(c) => {
            out.push("libsql.connect: OK".into());
            c
        }
        Err(e) => {
            out.push(format!("libsql.connect: ERR {e}"));
            return;
        }
    };
    cache_roundtrip(out, &cache, "libsql", "gproxy_st_k", "gproxy_st_c").await;
}

/// UpstashCache: construct (sync) → set/get/incr/delete/get round-trip.
async fn upstash_cache(out: &mut Vec<String>, url: String, token: String) {
    let cache = UpstashCache::new(url, token);
    out.push("upstash.construct: OK".into());
    cache_roundtrip(out, &cache, "upstash", "gproxy_st_uk", "gproxy_st_uc").await;
}

/// Shared CacheBackend exercise: set/get(==hello)/incr(+3)/delete/get(==None).
///
/// `value_key` and `counter_key` are distinct because `incr` and `set` must
/// not share a key (see backend docs).
async fn cache_roundtrip(
    out: &mut Vec<String>,
    cache: &dyn CacheBackend,
    label: &str,
    value_key: &str,
    counter_key: &str,
) {
    // Start clean so a stale counter from a prior run doesn't skew incr.
    cache.delete(counter_key).await;

    match cache.set(value_key, b"hello".to_vec(), None).await {
        Ok(()) => out.push(format!("{label}.set: OK")),
        Err(e) => out.push(format!("{label}.set: ERR {e}")),
    }

    match cache.get(value_key).await {
        Some(v) if v == b"hello" => {
            out.push(format!("{label}.get: {}", String::from_utf8_lossy(&v)))
        }
        Some(v) => out.push(format!(
            "{label}.get: ERR expected 'hello' got {:?}",
            String::from_utf8_lossy(&v)
        )),
        None => out.push(format!("{label}.get: ERR expected 'hello' got None")),
    }

    match cache.incr(counter_key, 3, None).await {
        Ok(3) => out.push(format!("{label}.incr: 3")),
        Ok(n) => out.push(format!("{label}.incr: {n} (expected 3 on fresh key)")),
        Err(e) => out.push(format!("{label}.incr: ERR {e}")),
    }

    cache.delete(value_key).await;
    out.push(format!("{label}.delete: OK"));

    match cache.get(value_key).await {
        None => out.push(format!("{label}.get_after_delete: None (OK)")),
        Some(v) => out.push(format!(
            "{label}.get_after_delete: ERR expected None got {:?}",
            String::from_utf8_lossy(&v)
        )),
    }

    // Clean up the counter we created.
    cache.delete(counter_key).await;
}
