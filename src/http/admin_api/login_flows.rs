//! Shared login-flow dispatcher (`/admin/login-flows/*`).
//!
//! Calls the cross-target `crate::admin::login` cache state-machine and
//! the same `ChannelLogin` trait methods through a provider/default resolved
//! upstream client (FetchClient on edge). Authcode/device flows are edge-safe;
//! cookie login uses the native browser-TLS client and returns 501 on edge.

use bytes::Bytes;
use http::Method;

use super::login_callback::parse_callback;
use super::login_support::{audit_sequence, provider_settings, seal_create};
use super::login_telemetry::{channel_error_kind, warn_flow_failure};
use super::{Request, Resp, json_body, segments};
use crate::admin::{guard::guard_admin, login};
use crate::api::error::ApiError;
#[cfg(not(target_arch = "wasm32"))]
use crate::api::login::CookieLoginRequest;
use crate::api::login::{
    DevicePollRequest, DeviceStartRequest, DeviceStartResponse, LoginCompleteRequest,
    LoginStartRequest, LoginStartResponse,
};
use crate::app::AppState;
#[cfg(not(target_arch = "wasm32"))]
use crate::channel::CookieExchangeCtx;
use crate::channel::oauth;
use crate::channel::{
    AuthCodeExchangeCtx, AuthCodeStartCtx, ChannelError, DevicePoll, DevicePollCtx, DeviceStartCtx,
};

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
    let provider_settings = provider_settings(state, req.provider_id, &req.channel)
        .map_err(|_| ApiError::BadRequest("login provider does not match channel".into()))?;
    let login_client = state
        .upstream_client_for_provider_id(req.provider_id)
        .map_err(|_| ApiError::BadRequest("login client init failed".into()))?;
    let audit = audit_sequence(state, &req.channel, req.provider_id, None);
    let audit_request_id = audit.request_id().to_owned();
    let login_client = audit.wrap_client(login_client);
    let started = channel
        .authcode_start(
            &login_client,
            AuthCodeStartCtx {
                provider_settings: &provider_settings,
                params: &params,
                redirect_uri: req.redirect_uri.as_deref().unwrap_or_default(),
                state: &state_tok,
                pkce_challenge: &challenge,
            },
        )
        .await;
    let error = started.as_ref().err().map(ToString::to_string);
    audit.persist(error.as_deref()).await;
    let started = started
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
        .ok_or_else(|| ApiError::BadRequest("channel has no authcode login".into()))?;

    let sid = login::start_session(
        state.cache.as_ref(),
        login::LoginSession {
            channel: req.channel,
            provider_id: req.provider_id,
            verifier,
            state: state_tok,
            redirect_uri: started.redirect_uri,
            extra: started.extra,
            audit_request_id: Some(audit_request_id),
        },
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

/// `POST /admin/login-flows/complete`. Verifies the pending login and CSRF state,
/// exchanges the callback code, then consumes the session and persists the
/// secret as a sealed credential under `provider_id`.
async fn complete(state: &AppState, parts: &Request, body: &Bytes) -> Result<Resp, ApiError> {
    guard_admin(state, parts).await?;
    let req: LoginCompleteRequest = json_body(body)?;

    let bad = || ApiError::BadRequest("login failed".into());

    // CODE-ONLY flows (e.g. geminicli `codeassist.google.com/authcode`) return a
    // bare authorization code with no callback URL / `state`; callback-URL flows
    // paste the full redirect. Validate the callback before reading the session.
    let bare_code = req.code.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let callback = if bare_code.is_none() {
        Some(parse_callback(&req.callback_url).ok_or_else(bad)?)
    } else {
        None
    };

    let Some(session) = login::peek(state.cache.as_ref(), &req.login_session_id).await else {
        tracing::warn!("authcode login session is missing or expired");
        return Err(bad());
    };
    let code = match (bare_code, callback) {
        // Bare code: no `state` to verify — PKCE (the per-session verifier) and
        // the short-lived server-side session provide the CSRF protection.
        (Some(code), _) => code.to_string(),
        (None, Some((code, cb_state))) => {
            // CSRF: the callback state MUST match the one we issued.
            if cb_state != session.state {
                tracing::warn!(channel = %session.channel, "authcode login callback state mismatch");
                return Err(bad());
            }
            code
        }
        (None, None) => return Err(bad()),
    };

    let channel = state.channels.login_for(&session.channel).ok_or_else(bad)?;
    let Some(provider_id) = authcode_provider_id(session.provider_id, req.provider_id) else {
        tracing::warn!(channel = %session.channel, "authcode login provider mismatch");
        return Err(bad());
    };
    let login_client = state
        .upstream_client_for_provider_id(Some(provider_id))
        .map_err(|_| {
            tracing::warn!(
                channel = %session.channel,
                provider_id,
                "authcode login client initialization failed"
            );
            bad()
        })?;
    let provider_settings =
        provider_settings(state, Some(provider_id), &session.channel).map_err(|_| bad())?;
    let audit = audit_sequence(
        state,
        &session.channel,
        Some(provider_id),
        session.audit_request_id.as_deref(),
    );
    let login_client = audit.wrap_client(login_client);
    let secret = channel
        .authcode_exchange(
            &login_client,
            AuthCodeExchangeCtx {
                provider_settings: &provider_settings,
                code: &code,
                verifier: &session.verifier,
                redirect_uri: &session.redirect_uri,
                extra: session.extra.as_ref(),
            },
        )
        .await;
    let error = secret.as_ref().err().map(ToString::to_string);
    audit.persist(error.as_deref()).await;
    let secret = secret.map_err(|error| {
        let error_kind = match &error {
            ChannelError::MissingSetting(_) => "missing_setting",
            ChannelError::InvalidCredential(_) => "invalid_credential",
            ChannelError::Unsupported(_) => "unsupported",
            ChannelError::Build(_) => "request_or_upstream",
            ChannelError::Transient(_) => "transient",
        };
        tracing::warn!(
            channel = %session.channel,
            provider_id,
            error_kind,
            "authcode login exchange failed"
        );
        bad()
    })?;

    // The authorization code is consumed upstream at this point; prevent replay
    // even if local sealing or persistence subsequently fails.
    login::clear(state.cache.as_ref(), &req.login_session_id).await;

    let sealed = state.cipher.seal(&secret).map_err(|_| {
        warn_flow_failure(&session.channel, provider_id, "seal", 0, "crypto");
        bad()
    })?;
    let name = req
        .name
        .or_else(|| crate::credentials::label::auto_label("oauth", &secret));
    let cred = seal_create(state, provider_id, &session.channel, name, sealed)
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
    let provider_settings = provider_settings(state, Some(req.provider_id), &req.channel)
        .map_err(|_| ApiError::BadRequest("device login provider does not match channel".into()))?;
    let login_client = state
        .upstream_client_for_provider_id(Some(req.provider_id))
        .map_err(|_| ApiError::BadRequest("device login client init failed".into()))?;
    let audit = audit_sequence(state, &req.channel, Some(req.provider_id), None);
    let audit_request_id = audit.request_id().to_owned();
    let login_client = audit.wrap_client(login_client);
    let init = channel
        .device_start(
            &login_client,
            DeviceStartCtx {
                provider_settings: &provider_settings,
                params: &params,
            },
        )
        .await;
    let error = init.as_ref().err().map(ToString::to_string);
    audit.persist(error.as_deref()).await;
    let init = init.map_err(|error| {
        warn_flow_failure(
            &req.channel,
            req.provider_id,
            "device_start",
            0,
            channel_error_kind(&error),
        );
        ApiError::BadRequest("channel has no device login".into())
    })?;
    let sid = login::device_start(
        state.cache.as_ref(),
        login::DeviceSession {
            channel: req.channel,
            device_code: init.device_code,
            provider_id: req.provider_id,
            name: req.name,
            audit_request_id: Some(audit_request_id),
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
    let provider_settings =
        provider_settings(state, Some(session.provider_id), &session.channel).map_err(|_| bad())?;

    let audit = audit_sequence(
        state,
        &session.channel,
        Some(session.provider_id),
        session.audit_request_id.as_deref(),
    );
    let login_client = audit.wrap_client(login_client);
    let poll = channel
        .device_poll(
            &login_client,
            DevicePollCtx {
                provider_settings: &provider_settings,
                device_code: &session.device_code,
            },
        )
        .await;
    let error = poll.as_ref().err().map(ToString::to_string);
    audit.persist(error.as_deref()).await;
    match poll {
        Ok(DevicePoll::Pending) => Resp::json(200, &serde_json::json!({ "status": "pending" })),
        Ok(DevicePoll::Ready(secret)) => {
            login::device_clear(state.cache.as_ref(), &req.login_session_id).await;
            let sealed = state.cipher.seal(&secret).map_err(|_| {
                warn_flow_failure(
                    &session.channel,
                    session.provider_id,
                    "device_poll.seal",
                    0,
                    "crypto",
                );
                bad()
            })?;
            let name = session
                .name
                .or_else(|| crate::credentials::label::auto_label("oauth", &secret));
            let cred = seal_create(state, session.provider_id, &session.channel, name, sealed)
                .await
                .map_err(|_| bad())?;
            Resp::json(
                200,
                &serde_json::json!({ "status": "ready", "credential": cred }),
            )
        }
        Ok(DevicePoll::Denied) => {
            login::device_clear(state.cache.as_ref(), &req.login_session_id).await;
            Err(bad())
        }
        Err(error) => {
            warn_flow_failure(
                &session.channel,
                session.provider_id,
                "device_poll",
                0,
                channel_error_kind(&error),
            );
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
    let provider_settings = provider_settings(state, Some(req.provider_id), &req.channel)
        .map_err(|_| ApiError::BadRequest("cookie login provider does not match channel".into()))?;
    let audit = audit_sequence(state, &req.channel, Some(req.provider_id), None);
    let cookie_client = audit.wrap_client(cookie_client);
    let secret = channel
        .cookie_exchange(
            &cookie_client,
            CookieExchangeCtx {
                provider_settings: &provider_settings,
                cookie: &req.cookie,
            },
        )
        .await;
    let error = secret.as_ref().err().map(ToString::to_string);
    audit.persist(error.as_deref()).await;
    let secret = secret.map_err(|error| {
        warn_flow_failure(
            &req.channel,
            req.provider_id,
            "cookie_exchange",
            0,
            channel_error_kind(&error),
        );
        ApiError::BadRequest("cookie login failed".into())
    })?;
    let sealed = state.cipher.seal(&secret).map_err(|_| {
        warn_flow_failure(&req.channel, req.provider_id, "seal", 0, "crypto");
        ApiError::BadRequest("cookie login failed".into())
    })?;
    let name = req
        .name
        .or_else(|| crate::credentials::label::auto_label("oauth", &secret));
    let credential = seal_create(state, req.provider_id, &req.channel, name, sealed).await?;
    Resp::json(200, &credential)
}

#[cfg(target_arch = "wasm32")]
async fn cookie(_state: &AppState, _parts: &Request, _body: &Bytes) -> Result<Resp, ApiError> {
    Err(ApiError::NotImplemented(
        "cookie login requires the native browser-TLS build; unavailable on edge".into(),
    ))
}
