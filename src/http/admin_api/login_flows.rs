//! Edge-safe login-flow dispatcher (`/admin/login-flows/*`).
//!
//! Replicates the native `src/http/server/admin/login.rs` handler logic,
//! calling the same cross-target `crate::admin::login` cache state-machine and
//! the same `ChannelLogin` trait methods through `&state.upstream` (FetchClient
//! on edge). All four authcode/device flows are edge-safe. The cookie-login
//! endpoint requires the native `upstream-wreq` browser-TLS client and is
//! degraded to 501 on edge.
//!
//! Also serves the explicit 501 degradation arms for self-update endpoints and
//! the `credentials/{id}/usage` read (both native-only features).

use bytes::Bytes;
use http::Method;
use http::request::Parts;

use crate::admin::{guard::guard_admin, invalidate, login};
use crate::api::error::ApiError;
use crate::api::login::{
    DevicePollRequest, DeviceStartRequest, DeviceStartResponse, LoginCompleteRequest,
    LoginStartRequest, LoginStartResponse,
};
use crate::app::AppState;
use crate::channel::DevicePoll;
use crate::channel::oauth;
use crate::store::persistence::records::CredentialInput;

use super::{Resp, json_body, segments};

/// Dispatch `/admin/login-flows/*` and the explicit 501 degradation arms for
/// `/admin/update/*` and `/admin/credentials/{id}/usage`.
///
/// Returns `Some(result)` when the path is handled here; `None` to fall through.
pub(super) async fn dispatch(
    state: &AppState,
    parts: &Parts,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let segs = segments(parts);
    match (&parts.method, segs.as_slice()) {
        // ── edge-safe login-flows (guard_admin: native in protected router) ──
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

        // ── 501 degradations ─────────────────────────────────────────────────

        // cookie-login uses WreqClient::browser() (native upstream-wreq only).
        (&Method::POST, ["admin", "login-flows", "cookie"]) => Some(Err(ApiError::NotImplemented(
            "cookie login requires the native browser-TLS build; unavailable on edge".to_string(),
        ))),

        // self-update endpoints (selfupdate module, native-only).
        (_, ["admin", "update", "check" | "status" | "apply"]) => Some(Err(
            ApiError::NotImplemented("self-update is unavailable on edge".to_string()),
        )),

        // credential live-usage (B6.2 follow-up, fetch_usage needs upstream-wreq).
        (_, ["admin", "credentials", _id, "usage"]) => Some(Err(ApiError::NotImplemented(
            "live credential usage is unavailable on edge".to_string(),
        ))),

        _ => None,
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `POST /admin/login-flows/start`. Resolves the channel's authcode login,
/// mints PKCE + CSRF state, stashes them in the cache, and returns the
/// authorize URL the admin sends the user to.
async fn start(state: &AppState, parts: &Parts, body: &Bytes) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let req: LoginStartRequest = json_body(body)?;

    let channel = state
        .channels
        .login_for(&req.channel)
        .ok_or_else(|| ApiError::NotFound("unknown channel".into()))?;

    let (verifier, challenge) = oauth::pkce();
    let state_tok = crate::util::rand::uuid_v4();
    let params = req.params.clone().unwrap_or_else(|| serde_json::json!({}));
    let started = channel
        .authcode_start(
            &state.upstream,
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
async fn complete(state: &AppState, parts: &Parts, body: &Bytes) -> Result<Resp, ApiError> {
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
    let secret = channel
        .authcode_exchange(
            &state.upstream,
            &code,
            &session.verifier,
            &session.redirect_uri,
            session.extra.as_ref(),
        )
        .await
        .map_err(|_| bad())?;

    let sealed = state.cipher.seal(&secret).map_err(|_| bad())?;
    let cred = seal_create(state, req.provider_id, req.name, sealed)
        .await
        .map_err(|_| bad())?;
    Resp::json(200, &cred)
}

/// `POST /admin/login-flows/device/start`. Asks the channel's device flow for a
/// code, stashes the device_code server-side, and returns the user-facing code
/// + verification URL the operator visits.
async fn device_start(state: &AppState, parts: &Parts, body: &Bytes) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let req: DeviceStartRequest = json_body(body)?;

    let channel = state
        .channels
        .login_for(&req.channel)
        .ok_or_else(|| ApiError::NotFound("unknown channel".into()))?;
    let params = req.params.clone().unwrap_or_else(|| serde_json::json!({}));
    let init = channel
        .device_start(&state.upstream, &params)
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
async fn device_poll(state: &AppState, parts: &Parts, body: &Bytes) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let req: DevicePollRequest = json_body(body)?;

    let bad = || ApiError::BadRequest("device login failed".into());
    let session = login::device_peek(state.cache.as_ref(), &req.login_session_id)
        .await
        .ok_or_else(bad)?;
    let channel = state.channels.login_for(&session.channel).ok_or_else(bad)?;

    match channel
        .device_poll(&state.upstream, &session.device_code)
        .await
    {
        Ok(DevicePoll::Pending) => Resp::json(200, &serde_json::json!({ "status": "pending" })),
        Ok(DevicePoll::Ready(secret)) => {
            login::device_clear(state.cache.as_ref(), &req.login_session_id).await;
            let sealed = state.cipher.seal(&secret).map_err(|_| bad())?;
            let cred = seal_create(state, session.provider_id, session.name, sealed)
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
