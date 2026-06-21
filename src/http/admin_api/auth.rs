//! Edge-dispatcher login / logout handlers.
//!
//! Replicates `src/http/server/admin/auth.rs` (login:31, logout:170) without
//! axum machinery: pure `(AppState, &Parts, &Bytes) → Result<Resp, ApiError>`,
//! testable on native and wasm.
//!
//! Key security properties (must stay identical to native):
//! - Throttle BEFORE the (slow) argon2 verify — fail-closed on cache error.
//! - Failure path: incr counters + audit; NEVER log the attempted password.
//! - Success path: clear counters + `session::create` + audit + Set-Cookie.
//! - `source_ip`: from XFF/X-Real-IP only (edge has no peer socket).

use std::time::Duration;

use bytes::Bytes;
use http::request::Parts;
use http::{HeaderMap, HeaderValue};

use crate::admin::session;
use crate::api::auth::{LoginRequest, LoginResponse, MeResponse};
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::cache::CacheBackend;
use crate::store::persistence::records::AuditLogInput;

use super::{Resp, internal, json_body};

/// Max consecutive failed logins per account / per source IP before lockout.
/// Same values as `src/http/server/admin/auth.rs` — redefined here because
/// that module is native-only (axum). Keep in sync if auth.rs changes.
const MAX_LOGIN_FAILS: i64 = 5;
/// Sliding window the failure count (and lockout) lives in.
const LOGIN_WINDOW: Duration = Duration::from_secs(60);

// ── Public handlers ───────────────────────────────────────────────────────────

/// `POST /admin/login` — public (no guard). Verifies credentials, issues a
/// session cookie. Brute-force throttled per account and per source IP.
pub(crate) async fn login(state: &AppState, parts: &Parts, body: &Bytes) -> Result<Resp, ApiError> {
    let source_ip = edge_client_ip(&parts.headers);
    let req: LoginRequest = json_body(body)?;

    let cache = state.cache.as_ref();
    let user_key = format!("loginfail:user:{}", req.username);
    let ip_key = source_ip.as_ref().map(|ip| format!("loginfail:ip:{ip}"));

    // Throttle BEFORE the (deliberately slow) argon2 verify.
    if over_limit(cache, &user_key).await || over_limit_opt(cache, ip_key.as_deref()).await {
        return Err(too_many_requests());
    }

    match verify_user(state, &req).await {
        Some(user) => {
            // Success: clear fail counters, create session, audit, return cookie.
            cache.delete(&user_key).await;
            if let Some(k) = &ip_key {
                cache.delete(k).await;
            }
            let token = session::create(cache, user.id)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?;
            // Edge: direct await (no tokio::spawn — edge isolate has no runtime).
            super::audit(
                state,
                AuditLogInput {
                    actor_id: Some(user.id),
                    actor_name: Some(user.name.clone()),
                    action: "login.success".into(),
                    target: req.username.clone(),
                    status: 200,
                    source_ip,
                },
            )
            .await;
            let cookie = session::set_cookie(
                &token,
                session::cookies_secure(),
                !state.config.cors_origins.is_empty(),
            );
            let cookie_val = HeaderValue::from_str(&cookie).map_err(internal)?;
            let body = serde_json::to_vec(&LoginResponse {
                user: MeResponse {
                    id: user.id,
                    name: user.name,
                    is_admin: user.is_admin,
                },
            })
            .map_err(|e| ApiError::Internal(e.to_string()))?;
            Ok(Resp {
                status: http::StatusCode::OK,
                headers: vec![
                    (
                        http::header::CONTENT_TYPE,
                        HeaderValue::from_static("application/json"),
                    ),
                    (http::header::SET_COOKIE, cookie_val),
                ],
                body,
            })
        }
        None => {
            // Failure: incr counters, audit (never log the password).
            let _ = cache.incr(&user_key, 1, Some(LOGIN_WINDOW)).await;
            if let Some(k) = &ip_key {
                let _ = cache.incr(k, 1, Some(LOGIN_WINDOW)).await;
            }
            super::audit(
                state,
                AuditLogInput {
                    actor_id: None,
                    actor_name: None,
                    action: "login.fail".into(),
                    target: req.username.clone(),
                    status: 401,
                    source_ip,
                },
            )
            .await;
            Err(ApiError::Unauthorized)
        }
    }
}

/// `POST /admin/logout` — public (no guard). Revokes the session (if any) and
/// clears the cookie. Always 204; idempotent.
pub(crate) async fn logout(state: &AppState, parts: &Parts) -> Result<Resp, ApiError> {
    let cache = state.cache.as_ref();
    if let Some(tok) = parts
        .headers
        .get(http::header::COOKIE)
        .and_then(|h| h.to_str().ok())
        .and_then(session::parse_cookie)
    {
        session::revoke(cache, tok).await;
    }
    let cookie = session::clear_cookie(
        session::cookies_secure(),
        !state.config.cors_origins.is_empty(),
    );
    let cookie_val = HeaderValue::from_str(&cookie).map_err(internal)?;
    Ok(Resp {
        status: http::StatusCode::NO_CONTENT,
        headers: vec![(http::header::SET_COOKIE, cookie_val)],
        body: vec![],
    })
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Verify credentials: get_user_by_name → enabled → password::verify.
/// Returns `None` for every failure (no user enumeration).
/// NOTE: no is_admin check — login admits any enabled user (matches F7a native).
async fn verify_user(
    state: &AppState,
    req: &LoginRequest,
) -> Option<crate::store::persistence::records::User> {
    let user = state
        .persistence
        .get_user_by_name(&req.username)
        .await
        .ok()??;
    if !user.enabled {
        return None;
    }
    let hash = user.password.as_deref()?;
    crate::crypto::password::verify(&req.password, hash).then_some(user)
}

/// `true` if `key`'s failure count is at/over the cap. Fail-closed on error.
async fn over_limit(cache: &dyn CacheBackend, key: &str) -> bool {
    match cache.incr(key, 0, Some(LOGIN_WINDOW)).await {
        Ok(n) => n >= MAX_LOGIN_FAILS,
        Err(_) => true,
    }
}

async fn over_limit_opt(cache: &dyn CacheBackend, key: Option<&str>) -> bool {
    match key {
        Some(k) => over_limit(cache, k).await,
        None => false,
    }
}

/// Build a 429 `ApiError` with the `Retry-After` header.
fn too_many_requests() -> ApiError {
    ApiError::TooManyRequests(LOGIN_WINDOW.as_secs().to_string())
}

/// Resolve the client IP on edge where there is no peer socket.
/// Resolve the client IP for the login throttle / audit. Prefers a
/// platform-STAMPED header (unspoofable on that platform) over the
/// client-controllable `x-forwarded-for`, in order: `cf-connecting-ip`
/// (Cloudflare Workers — authoritative), then `x-real-ip` (commonly set by the
/// edge platform / reverse proxy), then the `x-forwarded-for` rightmost
/// non-loopback hop (least trusted — a client can forge it, so it is only a
/// best-effort fallback when the stamped headers are absent).
///
/// The per-USER throttle (`loginfail:user:{name}`) is the primary brute-force
/// cap and is unaffected by IP forging; this is defense-in-depth.
pub(crate) fn edge_client_ip(headers: &HeaderMap) -> Option<String> {
    fn single(headers: &HeaderMap, name: &str) -> Option<String> {
        headers
            .get(name)
            .and_then(|h| h.to_str().ok())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    }
    // 1-2: platform-stamped headers, trusted first.
    if let Some(ip) = single(headers, "cf-connecting-ip").or_else(|| single(headers, "x-real-ip")) {
        return Some(ip);
    }
    // 3: client-controllable XFF — rightmost non-loopback hop.
    headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|h| h.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .find(|s| {
            s.parse::<std::net::IpAddr>()
                .map(|ip| !ip.is_loopback())
                .unwrap_or(true) // non-IP: treat as client-supplied
        })
        .map(str::to_owned)
}
