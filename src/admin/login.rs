//! Admin OAuth login orchestration (§14.5): short-TTL PKCE/state in the cache.
//!
//! The interactive authcode flow is two HTTP round-trips (`start` → user visits
//! the authorize URL → `complete` with the callback). Between them the PKCE
//! verifier + CSRF state + redirect_uri must survive server-side but never reach
//! the browser. They live in the cache under a random one-shot id with a 10-min
//! TTL — `take` deletes on read so a callback can't be replayed.
//!
//! cache-only, axum-free → compiles on native and edge.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::store::cache::{CacheBackend, CacheError};

/// Lifetime of a pending login (matches the v1 codex state TTL).
const LOGIN_TTL: Duration = Duration::from_secs(600);

/// Lifetime of a pending device-code login — longer than the authcode TTL since
/// the operator must visit a URL and enter a code (GitHub device codes expire
/// in ~15 min).
const DEVICE_TTL: Duration = Duration::from_secs(900);

/// Server-side state of a pending OAuth login, keyed by an opaque session id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginSession {
    pub channel: String,
    pub verifier: String,
    pub state: String,
    pub redirect_uri: String,
    /// Opaque channel state from `AuthCodeStart::extra` (e.g. registered IdC
    /// client creds), handed back to `authcode_exchange` at `complete`.
    #[serde(default)]
    pub extra: Option<serde_json::Value>,
}

/// Stash a pending login → cache `login:{sid}` for [`LOGIN_TTL`]. Returns the
/// random one-shot session id. A failed stash fails the start: the returned
/// sid would otherwise 400 at `complete` after the operator finished the
/// browser round-trip.
pub async fn start(
    cache: &dyn CacheBackend,
    channel: String,
    verifier: String,
    state: String,
    redirect_uri: String,
    extra: Option<serde_json::Value>,
) -> Result<String, CacheError> {
    let sid = crate::util::rand::uuid_v4();
    let session = LoginSession {
        channel,
        verifier,
        state,
        redirect_uri,
        extra,
    };
    // Serialization of this fixed shape cannot fail.
    let bytes = serde_json::to_vec(&session).map_err(|_| CacheError)?;
    cache.set(&key(&sid), bytes, Some(LOGIN_TTL)).await?;
    Ok(sid)
}

/// Consume a pending login (one-shot: get + delete). `None` if missing/expired.
pub async fn take(cache: &dyn CacheBackend, sid: &str) -> Option<LoginSession> {
    let raw = cache.get(&key(sid)).await?;
    cache.delete(&key(sid)).await;
    serde_json::from_slice(&raw).ok()
}

fn key(sid: &str) -> String {
    format!("login:{sid}")
}

/// Server-side state of a pending device-code login. The `device_code` is the
/// secret the poll endpoint replays to the provider — it never reaches the
/// browser (the operator only sees the user_code + verification URL).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSession {
    pub channel: String,
    pub device_code: String,
    pub provider_id: i64,
    pub name: Option<String>,
}

/// Stash a pending device login → cache `login:{sid}` for [`DEVICE_TTL`].
/// A failed stash fails the start (the poll endpoint could never find it).
pub async fn device_start(
    cache: &dyn CacheBackend,
    session: DeviceSession,
) -> Result<String, CacheError> {
    let sid = crate::util::rand::uuid_v4();
    let bytes = serde_json::to_vec(&session).map_err(|_| CacheError)?;
    cache.set(&key(&sid), bytes, Some(DEVICE_TTL)).await?;
    Ok(sid)
}

/// Peek a pending device login WITHOUT deleting it — the poll endpoint reads it
/// repeatedly while `Pending`, deleting only on a terminal outcome via
/// [`device_clear`].
pub async fn device_peek(cache: &dyn CacheBackend, sid: &str) -> Option<DeviceSession> {
    let raw = cache.get(&key(sid)).await?;
    serde_json::from_slice(&raw).ok()
}

/// Delete a device login session (on Ready/Denied).
pub async fn device_clear(cache: &dyn CacheBackend, sid: &str) {
    cache.delete(&key(sid)).await;
}
