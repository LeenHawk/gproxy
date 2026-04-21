//! HTTP header helpers + turn context cache for the ChatGPT web channel.
//!
//! ChatGPT's `/backend-api/f/conversation` endpoint sits behind a Cloudflare
//! WAF that issues a `cf-mitigated: challenge` unless the request carries a
//! `__cf_bm` cookie established by a prior GET to the origin.
//!
//! This module:
//! * provides [`standard_headers`] — the common request header set,
//! * provides [`warmup`] — populates `__cf_bm` on a caller-supplied
//!   `wreq::Client` (which MUST have `cookie_store(true)`),
//! * caches per-turn credential context in [`stash_turn`] / [`take_turn`]
//!   keyed by `x-oai-turn-trace-id` — so `normalize_response` (which does
//!   not receive the credential) can still reach back for image download.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use std::sync::OnceLock;
use wreq::Client;

use crate::response::UpstreamError;

const WARMUP_PATHS: &[&str] = &["/", "/backend-api/me"];
const CHATGPT_ORIGIN: &str = "https://chatgpt.com";
/// Re-warm the CF session at most this often. The `__cf_bm` cookie's TTL
/// is 30 minutes; we stay comfortably inside that.
const WARMUP_TTL: Duration = Duration::from_secs(25 * 60);

/// Chrome-like desktop User-Agent string. Kept in sync with the
/// `DEFAULT_USER_AGENT` in `prepare_p.rs`.
pub const DEFAULT_USER_AGENT: &str = super::prepare_p::DEFAULT_USER_AGENT;

/// Content of `oai-client-version` header expected by the backend.
pub const OAI_CLIENT_VERSION: &str = super::prepare_p::DEFAULT_BUILD_ID;

/// Perform a best-effort CF warmup on `client` for the given access token.
///
/// Fires two cheap GETs (`/`, `/backend-api/me`) to let Cloudflare set a
/// `__cf_bm` cookie on this client. Safe to call repeatedly — returns
/// immediately if a recent warmup is still fresh.
///
/// The caller's `client` **MUST** have been built with `cookie_store(true)`,
/// otherwise the set cookies will not persist into subsequent requests.
pub async fn warmup(client: &Client, access_token: &str) -> Result<(), UpstreamError> {
    static LAST_WARMUP: Mutex<Option<Instant>> = Mutex::new(None);
    {
        let guard = LAST_WARMUP.lock().unwrap();
        if let Some(t) = *guard
            && t.elapsed() < WARMUP_TTL
        {
            return Ok(());
        }
    }
    for path in WARMUP_PATHS {
        let url = format!("{CHATGPT_ORIGIN}{path}");
        let req = client
            .get(&url)
            .headers(standard_headers(access_token).into());
        if let Err(e) = req.send().await {
            tracing::warn!(error = %e, "chatgpt warmup GET {path} failed");
        }
    }
    *LAST_WARMUP.lock().unwrap() = Some(Instant::now());
    Ok(())
}

/// Per-turn credential/client context stashed by `prepare_request` for
/// later pickup by `normalize_response` (which has no way to access the
/// credential or HTTP client otherwise). Keyed by `x-oai-turn-trace-id`.
#[derive(Clone)]
pub struct TurnContext {
    pub access_token: String,
    pub chat_req_token: String,
    pub device_id: String,
    pub client: Client,
}

fn turn_cache() -> &'static DashMap<String, TurnContext> {
    static CACHE: OnceLock<DashMap<String, TurnContext>> = OnceLock::new();
    CACHE.get_or_init(DashMap::new)
}

pub fn stash_turn(trace_id: String, ctx: TurnContext) {
    // Prune anything older than 60s on each insert to keep the map small.
    let cache = turn_cache();
    cache.insert(trace_id, ctx);
    if cache.len() > 1024 {
        let stale: Vec<String> = cache.iter().map(|e| e.key().clone()).take(256).collect();
        for k in stale {
            cache.remove(&k);
        }
    }
}

pub fn take_turn(trace_id: &str) -> Option<TurnContext> {
    turn_cache().remove(trace_id).map(|(_, v)| v)
}

/// Shared cookie-enabled `wreq::Client` for fallback HTTP calls made
/// outside the engine's request loop (file downloads etc.). Built lazily
/// on first use with Chrome emulation + cookie jar.
pub fn shared_fallback_client() -> Result<Client, UpstreamError> {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    if let Some(c) = CLIENT.get() {
        return Ok(c.clone());
    }
    let built = Client::builder()
        .emulation(wreq_util::Emulation::Chrome136)
        .cookie_store(true)
        .redirect(wreq::redirect::Policy::limited(10))
        .build()
        .map_err(|e| UpstreamError::Channel(format!("chatgpt fallback client: {e}")))?;
    let _ = CLIENT.set(built);
    Ok(CLIENT.get().cloned().expect("just set"))
}

/// Warmup the shared fallback client specifically. Tracks its own
/// timestamp so that warmups for the engine's http_client do not mask
/// the fallback needing one.
pub async fn warmup_fallback(client: &Client, access_token: &str) -> Result<(), UpstreamError> {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    {
        let guard = LAST.lock().unwrap();
        if let Some(t) = *guard
            && t.elapsed() < WARMUP_TTL
        {
            return Ok(());
        }
    }
    for path in WARMUP_PATHS {
        let url = format!("{CHATGPT_ORIGIN}{path}");
        let req = client
            .get(&url)
            .headers(standard_headers(access_token).into());
        if let Err(e) = req.send().await {
            tracing::warn!(error = %e, "chatgpt fallback warmup GET {path} failed");
        }
    }
    *LAST.lock().unwrap() = Some(Instant::now());
    Ok(())
}

/// Common request headers (non-sentinel) used for every backend-api call.
pub fn standard_headers(access_token: &str) -> StandardHeaders {
    StandardHeaders {
        access_token: access_token.to_string(),
    }
}

/// Builder-style helper for the recurring "chatgpt web" request header set.
/// The fields are populated once and then flattened into a [`http::HeaderMap`]
/// when attached to a request.
#[derive(Clone)]
pub struct StandardHeaders {
    access_token: String,
}

impl From<StandardHeaders> for http::HeaderMap {
    fn from(s: StandardHeaders) -> http::HeaderMap {
        let mut map = http::HeaderMap::new();
        let add = |map: &mut http::HeaderMap, name: &'static str, value: String| {
            let n = http::HeaderName::from_static(name);
            if let Ok(v) = http::HeaderValue::from_str(&value) {
                map.insert(n, v);
            }
        };
        add(&mut map, "accept", "*/*".into());
        add(
            &mut map,
            "accept-language",
            "en-US,en;q=0.9,zh-CN;q=0.8,zh;q=0.7".into(),
        );
        add(&mut map, "content-type", "application/json".into());
        add(&mut map, "origin", CHATGPT_ORIGIN.into());
        add(&mut map, "referer", format!("{CHATGPT_ORIGIN}/"));
        add(
            &mut map,
            "authorization",
            format!("Bearer {}", s.access_token),
        );
        add(&mut map, "oai-client-version", OAI_CLIENT_VERSION.into());
        add(&mut map, "oai-language", "en-US".into());
        add(
            &mut map,
            "sec-ch-ua",
            r#""Microsoft Edge";v="147", "Chromium";v="147", "Not_A Brand";v="24""#.into(),
        );
        add(&mut map, "sec-ch-ua-arch", r#""x86""#.into());
        add(&mut map, "sec-ch-ua-bitness", r#""64""#.into());
        add(
            &mut map,
            "sec-ch-ua-full-version",
            r#""147.0.3912.72""#.into(),
        );
        add(
            &mut map,
            "sec-ch-ua-full-version-list",
            r#""Microsoft Edge";v="147.0.3912.72", "Chromium";v="147.0.7727.102""#.into(),
        );
        add(&mut map, "sec-ch-ua-mobile", "?0".into());
        add(&mut map, "sec-ch-ua-model", r#""""#.into());
        add(&mut map, "sec-ch-ua-platform", r#""Windows""#.into());
        add(&mut map, "sec-ch-ua-platform-version", r#""19.0.0""#.into());
        add(&mut map, "sec-fetch-dest", "empty".into());
        add(&mut map, "sec-fetch-mode", "cors".into());
        add(&mut map, "sec-fetch-site", "same-origin".into());
        map
    }
}
