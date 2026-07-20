//! Shared login-flow dispatcher (`/admin/login-flows/*`).
//!
//! Calls the cross-target `crate::admin::login` cache state-machine and
//! the same `ChannelLogin` trait methods through a provider/default resolved
//! upstream client (FetchClient on edge). Authcode/device flows are edge-safe;
//! cookie login uses the native browser-TLS client and returns 501 on edge.

use bytes::Bytes;
use http::Method;

use crate::admin::{guard::guard_admin, invalidate, login};
use crate::api::error::ApiError;
#[cfg(not(target_arch = "wasm32"))]
use crate::api::login::CookieLoginRequest;
use crate::api::login::{
    DevicePollRequest, DeviceStartRequest, DeviceStartResponse, LoginCompleteRequest,
    LoginStartRequest, LoginStartResponse,
};
use crate::app::AppState;
use crate::channel::DevicePoll;
use crate::channel::oauth;
use crate::store::persistence::records::CredentialInput;

use super::{Request, Resp, json_body, segments};

/// Dispatch `/admin/login-flows/*`.
///
/// Returns `Some(result)` when the path is handled here; `None` to fall through.
pub(super) async fn dispatch(
    state: &AppState,
    parts: &Request,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let segs = segments(parts);
    match (&parts.method, segs.as_slice()) {
        // Authcode and device flows are available on both targets.
        (&Method::POST, ["admin", "login-flows", "start"]) => Some(start(state, parts, body).await),
        (&Method::POST, ["admin", "login-flows", "complete"]) => {
            Some(complete(state, parts, body).await)
        }
        (&Method::POST, ["admin", "login-flows", "device", "start"]) => {
            Some(device_start(state, parts, body).await)
        }
        (&Method::POST, ["admin", "login-flows", "device", "poll"]) => {
            Some(device_poll(state, parts, body).await)
        }

        (&Method::POST, ["admin", "login-flows", "cookie"]) => {
            Some(cookie(state, parts, body).await)
        }

        _ => None,
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `POST /admin/login-flows/start`. Resolves the channel's authcode login,
/// mints PKCE + CSRF state, stashes them in the cache, and returns the
/// authorize URL the admin sends the user to.
async fn start(state: &AppState, parts: &Request, body: &Bytes) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let req: LoginStartRequest = json_body(body)?;

    let channel = state
        .channels
        .login_for(&req.channel)
        .ok_or_else(|| ApiError::NotFound("unknown channel".into()))?;

    let (verifier, challenge) = oauth::pkce();
    let state_tok = crate::util::rand::uuid_v4();
    let params = req.params.clone().unwrap_or_else(|| serde_json::json!({}));
    let login_client = state
        .upstream_client_for_provider_id(req.provider_id)
        .map_err(|_| ApiError::BadRequest("login client init failed".into()))?;
    let started = channel
        .authcode_start(
            &login_client,
            &params,
            req.redirect_uri.as_deref().unwrap_or_default(),
            &state_tok,
            &challenge,
        )
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
        .ok_or_else(|| ApiError::BadRequest("channel has no authcode login".into()))?;

    let sid = login::start(
        state.cache.as_ref(),
        req.channel,
        req.provider_id,
        verifier,
        state_tok,
        started.redirect_uri,
        started.extra,
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;

    Resp::json(
        200,
        &LoginStartResponse {
            login_session_id: sid,
            authorize_url: started.authorize_url,
        },
    )
}

/// `POST /admin/login-flows/complete`. Consumes the pending login, verifies the
/// CSRF state, exchanges the callback code for a secret, and persists it as a
/// sealed credential under `provider_id`.
async fn complete(state: &AppState, parts: &Request, body: &Bytes) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let req: LoginCompleteRequest = json_body(body)?;

    let bad = || ApiError::BadRequest("login failed".into());

    // CODE-ONLY flows (e.g. geminicli `codeassist.google.com/authcode`) return a
    // bare authorization code with no callback URL / `state`; callback-URL flows
    // paste the full redirect. Validate the callback up front so a malformed
    // paste doesn't consume the one-shot session.
    let bare_code = req.code.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let callback = if bare_code.is_none() {
        Some(parse_callback(&req.callback_url).ok_or_else(bad)?)
    } else {
        None
    };

    let session = login::take(state.cache.as_ref(), &req.login_session_id)
        .await
        .ok_or_else(bad)?;
    let code = match (bare_code, callback) {
        // Bare code: no `state` to verify — PKCE (the per-session verifier) is the
        // CSRF protection, and the session is one-shot + short-TTL.
        (Some(code), _) => code.to_string(),
        (None, Some((code, cb_state))) => {
            // CSRF: the callback state MUST match the one we issued.
            if cb_state != session.state {
                return Err(bad());
            }
            code
        }
        (None, None) => return Err(bad()),
    };

    let channel = state.channels.login_for(&session.channel).ok_or_else(bad)?;
    let provider_id = authcode_provider_id(session.provider_id, req.provider_id).ok_or_else(bad)?;
    let login_client = state
        .upstream_client_for_provider_id(Some(provider_id))
        .map_err(|_| bad())?;
    let secret = channel
        .authcode_exchange(
            &login_client,
            &code,
            &session.verifier,
            &session.redirect_uri,
            session.extra.as_ref(),
        )
        .await
        .map_err(|_| bad())?;

    let sealed = state.cipher.seal(&secret).map_err(|_| bad())?;
    let name = req.name.or_else(|| label_from_secret(&secret));
    let cred = seal_create(state, provider_id, name, sealed)
        .await
        .map_err(|_| bad())?;
    Resp::json(200, &cred)
}

/// `POST /admin/login-flows/device/start`. Asks the channel's device flow for a
/// code, stashes the device_code server-side, and returns the user-facing code
/// + verification URL the operator visits.
async fn device_start(state: &AppState, parts: &Request, body: &Bytes) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let req: DeviceStartRequest = json_body(body)?;

    let channel = state
        .channels
        .login_for(&req.channel)
        .ok_or_else(|| ApiError::NotFound("unknown channel".into()))?;
    let params = req.params.clone().unwrap_or_else(|| serde_json::json!({}));
    let login_client = state
        .upstream_client_for_provider_id(Some(req.provider_id))
        .map_err(|_| ApiError::BadRequest("device login client init failed".into()))?;
    let init = channel
        .device_start(&login_client, &params)
        .await
        .map_err(|_| ApiError::BadRequest("channel has no device login".into()))?;
    let sid = login::device_start(
        state.cache.as_ref(),
        login::DeviceSession {
            channel: req.channel,
            device_code: init.device_code,
            provider_id: req.provider_id,
            name: req.name,
        },
    )
    .await
    .map_err(|e| ApiError::Internal(e.to_string()))?;
    Resp::json(
        200,
        &DeviceStartResponse {
            login_session_id: sid,
            user_code: init.user_code,
            verification_url: init.verification_url,
            interval_secs: init.interval_secs,
        },
    )
}

/// `POST /admin/login-flows/device/poll`. Polls the provider once with the
/// stashed device_code: `pending` keeps the session; `ready` seals + creates
/// the credential and clears the session; `denied`/error clears + 400s.
async fn device_poll(state: &AppState, parts: &Request, body: &Bytes) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let req: DevicePollRequest = json_body(body)?;

    let bad = || ApiError::BadRequest("device login failed".into());
    let session = login::device_peek(state.cache.as_ref(), &req.login_session_id)
        .await
        .ok_or_else(bad)?;
    let channel = state.channels.login_for(&session.channel).ok_or_else(bad)?;
    let login_client = state
        .upstream_client_for_provider_id(Some(session.provider_id))
        .map_err(|_| bad())?;

    match channel
        .device_poll(&login_client, &session.device_code)
        .await
    {
        Ok(DevicePoll::Pending) => Resp::json(200, &serde_json::json!({ "status": "pending" })),
        Ok(DevicePoll::Ready(secret)) => {
            login::device_clear(state.cache.as_ref(), &req.login_session_id).await;
            let sealed = state.cipher.seal(&secret).map_err(|_| bad())?;
            let name = session.name.or_else(|| label_from_secret(&secret));
            let cred = seal_create(state, session.provider_id, name, sealed)
                .await
                .map_err(|_| bad())?;
            Resp::json(
                200,
                &serde_json::json!({ "status": "ready", "credential": cred }),
            )
        }
        Ok(DevicePoll::Denied) | Err(_) => {
            login::device_clear(state.cache.as_ref(), &req.login_session_id).await;
            Err(bad())
        }
    }
}

fn authcode_provider_id(start_provider_id: Option<i64>, complete_provider_id: i64) -> Option<i64> {
    match start_provider_id {
        Some(id) if id == complete_provider_id => Some(id),
        Some(_) => None,
        None => Some(complete_provider_id),
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn cookie(state: &AppState, parts: &Request, body: &Bytes) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let req: CookieLoginRequest = json_body(body)?;
    let request_channel = state
        .channels
        .get(&req.channel)
        .ok_or_else(|| ApiError::NotFound("unknown channel".into()))?;
    let channel = state
        .channels
        .login_for(&req.channel)
        .ok_or_else(|| ApiError::NotFound("unknown channel".into()))?;
    let cookie_client = state
        .upstream_client_for_cookie_login(&request_channel, req.provider_id)
        .map_err(|_| ApiError::BadRequest("cookie login client init failed".into()))?;
    let secret = channel
        .cookie_exchange(&cookie_client, &req.cookie)
        .await
        .map_err(|error| {
            tracing::warn!(channel = %req.channel, %error, "cookie login exchange failed");
            ApiError::BadRequest("cookie login failed".into())
        })?;
    let sealed = state
        .cipher
        .seal(&secret)
        .map_err(|_| ApiError::BadRequest("cookie login failed".into()))?;
    let name = req.name.or_else(|| label_from_secret(&secret));
    let credential = seal_create(state, req.provider_id, name, sealed).await?;
    Resp::json(200, &credential)
}

#[cfg(target_arch = "wasm32")]
async fn cookie(_state: &AppState, _parts: &Request, _body: &Bytes) -> Result<Resp, ApiError> {
    Err(ApiError::NotImplemented(
        "cookie login requires the native browser-TLS build; unavailable on edge".into(),
    ))
}

fn label_from_secret(secret: &serde_json::Value) -> Option<String> {
    let email = secret
        .get("user_email")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let tier = secret
        .get("rate_limit_tier")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    Some(match tier {
        Some(tier) => format!("{email} {tier}"),
        None => email.to_string(),
    })
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Seal-then-persist: a pre-sealed secret + target provider/name → a redacted
/// `CredentialView`. `kind="oauth"`, default weight, enabled; cache invalidated.
async fn seal_create(
    state: &AppState,
    provider_id: i64,
    name: Option<String>,
    sealed: serde_json::Value,
) -> Result<crate::api::credentials::CredentialView, ApiError> {
    let input = CredentialInput {
        id: None,
        provider_id,
        name,
        kind: "oauth".into(),
        secret_json: sealed,
        weight: 100,
        rpm_limit: None,
        tpm_limit: None,
        proxy_url: None,
        tls_fingerprint: None,
        enabled: true,
    };
    let cred = state
        .persistence
        .upsert_credential(input)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    invalidate(state).await;
    Ok(crate::api::credentials::CredentialView::from(cred))
}

/// Pull `code` + `state` out of a callback URL's query string. No external URL
/// dep: `http::Uri` splits off the query, then a manual `&`/`=` walk with
/// percent-decoding. Both params are required (replicated from native login.rs).
fn parse_callback(callback_url: &str) -> Option<(String, String)> {
    let uri: http::Uri = callback_url.parse().ok()?;
    let query = uri.query()?;
    let mut code = None;
    let mut state = None;
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=')?;
        match k {
            "code" => code = Some(pct_decode(v)),
            "state" => state = Some(pct_decode(v)),
            _ => {}
        }
    }
    Some((code?, state?))
}

/// Percent-decode a query value (`+` → space, `%XX` → byte). Lossy on invalid
/// UTF-8; malformed `%` escapes are kept verbatim.
fn pct_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    out.push((hi * 16 + lo) as u8);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
