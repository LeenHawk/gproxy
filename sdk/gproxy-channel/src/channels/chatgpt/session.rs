//! HTTP client + session management for the ChatGPT web channel.
//!
//! ChatGPT's `/backend-api/f/conversation` endpoint sits behind a Cloudflare
//! WAF that issues a `cf-mitigated: challenge` unless the request carries a
//! `__cf_bm` cookie established by a prior GET to the origin. We therefore
//! keep a single long-lived `wreq::Client` with a cookie jar and "warm it up"
//! once on first use.

use std::sync::{Arc, OnceLock};

use tokio::sync::Mutex;
use wreq::Client;

use crate::response::UpstreamError;

const WARMUP_PATHS: &[&str] = &["/", "/backend-api/me"];
const CHATGPT_ORIGIN: &str = "https://chatgpt.com";

/// Chrome-like desktop User-Agent string. Kept in sync with the
/// `DEFAULT_USER_AGENT` in `prepare_p.rs`.
pub const DEFAULT_USER_AGENT: &str = super::prepare_p::DEFAULT_USER_AGENT;

/// Content of `oai-client-version` header expected by the backend.
pub const OAI_CLIENT_VERSION: &str = super::prepare_p::DEFAULT_BUILD_ID;

/// Process-wide cached client. Built lazily on first use.
static SHARED_CLIENT: OnceLock<Arc<SessionState>> = OnceLock::new();

struct SessionState {
    client: Client,
    warmup: Mutex<bool>,
}

/// Return a process-wide `wreq::Client` impersonating Chrome, with a cookie
/// jar enabled. Subsequent calls return the same instance (so cookies
/// persist across channel requests).
pub fn shared_client() -> Result<Client, UpstreamError> {
    shared_state().map(|s| s.client.clone())
}

fn shared_state() -> Result<Arc<SessionState>, UpstreamError> {
    if let Some(s) = SHARED_CLIENT.get() {
        return Ok(s.clone());
    }
    let client = Client::builder()
        .emulation(wreq_util::Emulation::Chrome136)
        .cookie_store(true)
        .redirect(wreq::redirect::Policy::limited(10))
        .build()
        .map_err(|e| UpstreamError::Channel(format!("build chatgpt client: {e}")))?;
    let state = Arc::new(SessionState {
        client,
        warmup: Mutex::new(false),
    });
    // `get_or_init` wants no-fallible closure; use set_then_get pattern.
    match SHARED_CLIENT.set(state.clone()) {
        Ok(()) => Ok(state),
        Err(_) => Ok(SHARED_CLIENT.get().cloned().expect("just set")),
    }
}

/// Hit the origin once to populate the `__cf_bm` cookie. No-op on subsequent
/// calls in the same process.
pub async fn ensure_warmed(access_token: &str) -> Result<Client, UpstreamError> {
    let state = shared_state()?;
    {
        let mut warmed = state.warmup.lock().await;
        if !*warmed {
            for path in WARMUP_PATHS {
                let url = format!("{CHATGPT_ORIGIN}{path}");
                let _ = state
                    .client
                    .get(&url)
                    .headers(standard_headers(access_token).into())
                    .send()
                    .await;
            }
            *warmed = true;
        }
    }
    Ok(state.client.clone())
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
        add(
            &mut map,
            "sec-ch-ua-platform-version",
            r#""19.0.0""#.into(),
        );
        add(&mut map, "sec-fetch-dest", "empty".into());
        add(&mut map, "sec-fetch-mode", "cors".into());
        add(&mut map, "sec-fetch-site", "same-origin".into());
        map
    }
}
