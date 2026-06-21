//! Admin auth endpoints: login (issue session cookie), logout (revoke), me.

use std::time::Duration;

use axum::extract::{FromRequest, Request, State};
use axum::http::header::{COOKIE, SET_COOKIE};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};

use crate::admin::session::{self, AdminUser};
use crate::api::auth::{LoginRequest, LoginResponse, MeResponse};
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::cache::CacheBackend;
use crate::store::persistence::records::{AuditLogInput, User};

use super::{client_ip, peer_ip};

/// Max consecutive failed logins per account / per source IP before lockout.
const MAX_LOGIN_FAILS: i64 = 5;
/// Sliding window the failure count (and lockout) lives in.
const LOGIN_WINDOW: Duration = Duration::from_secs(60);

/// `POST /admin/login`. Verifies credentials and issues a session cookie.
///
/// Brute-force throttled (§ admin hardening): after [`MAX_LOGIN_FAILS`] failures
/// in [`LOGIN_WINDOW`] for an account OR a source IP, further attempts get 429
/// until the window passes; a success resets both counters. Every failure path
/// returns a generic 401 — no user enumeration. Takes the raw `Request` so the
/// `ConnectInfo` peer is available to the trusted-proxy client-IP resolution.
pub async fn login(State(state): State<AppState>, request: Request) -> Response {
    let peer = peer_ip(request.extensions());
    let source_ip = client_ip(request.headers(), peer, &state.config.trusted_proxies);
    let Json(req) = match Json::<LoginRequest>::from_request(request, &()).await {
        Ok(json) => json,
        Err(e) => return e.into_response(),
    };

    let cache = state.cache.as_ref();
    let user_key = format!("loginfail:user:{}", req.username);
    let ip_key = source_ip.as_ref().map(|ip| format!("loginfail:ip:{ip}"));

    // Throttle BEFORE the (deliberately slow) argon2 verify, so a locked-out
    // attacker can't even drive CPU.
    if over_limit(cache, &user_key).await || over_limit_opt(cache, ip_key.as_deref()).await {
        return too_many_requests();
    }

    match verify_admin(&state, &req).await {
        Some(user) => {
            cache.delete(&user_key).await;
            if let Some(k) = &ip_key {
                cache.delete(k).await;
            }
            let token = match session::create(cache, user.id).await {
                Ok(t) => t,
                // Credentials verified but the session never landed in the
                // cache — the cookie would 401 on every use. Surface a 500.
                Err(e) => {
                    tracing::error!(error = %e, "admin session create failed");
                    return ApiError::Internal(e.to_string()).into_response();
                }
            };
            record_audit(
                &state,
                AuditLogInput {
                    actor_id: Some(user.id),
                    actor_name: Some(user.name.clone()),
                    action: "login.success".into(),
                    target: req.username.clone(),
                    status: 200,
                    source_ip,
                },
            );
            let body = LoginResponse {
                user: MeResponse {
                    id: user.id,
                    name: user.name.clone(),
                    is_admin: user.is_admin,
                },
            };
            let cookie = session::set_cookie(
                &token,
                session::cookies_secure(),
                !state.config.cors_origins.is_empty(),
            );
            ([(SET_COOKIE, cookie)], Json(body)).into_response()
        }
        None => {
            let _ = cache.incr(&user_key, 1, Some(LOGIN_WINDOW)).await;
            if let Some(k) = &ip_key {
                let _ = cache.incr(k, 1, Some(LOGIN_WINDOW)).await;
            }
            // Never log the password — only the attempted username.
            record_audit(
                &state,
                AuditLogInput {
                    actor_id: None,
                    actor_name: None,
                    action: "login.fail".into(),
                    target: req.username.clone(),
                    status: 401,
                    source_ip,
                },
            );
            ApiError::Unauthorized.into_response()
        }
    }
}

/// Fire-and-forget audit write so the login response isn't delayed.
fn record_audit(state: &AppState, input: AuditLogInput) {
    let persistence = state.persistence.clone();
    tokio::spawn(async move {
        if let Err(e) = persistence.append_audit_log(input).await {
            tracing::warn!("audit log write failed: {e}");
        }
    });
}

/// Verify an admin password login, returning the user on success. `None` for
/// every failure (no user / not admin / disabled / no hash / wrong password).
async fn verify_admin(state: &AppState, req: &LoginRequest) -> Option<User> {
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

/// `true` if `key`'s failure count is at/over the cap. `incr` by 0 reads the
/// current value (creating it at 0 with the window TTL when absent). A counter
/// backend failure throttles too (fail-closed): a cache outage must not switch
/// brute-force protection off — and with the cache down, sessions can't be
/// issued anyway, so login is already unavailable.
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

/// 429 with a `Retry-After` matching the lockout window.
fn too_many_requests() -> Response {
    (
        axum::http::StatusCode::TOO_MANY_REQUESTS,
        [(
            axum::http::header::RETRY_AFTER,
            LOGIN_WINDOW.as_secs().to_string(),
        )],
        "too many login attempts",
    )
        .into_response()
}

/// `POST /admin/logout`. Revokes the current session (if any) and clears the
/// cookie. Always 204 — idempotent.
pub async fn logout(State(state): State<AppState>, headers: axum::http::HeaderMap) -> Response {
    if let Some(tok) = headers
        .get(COOKIE)
        .and_then(|h| h.to_str().ok())
        .and_then(session::parse_cookie)
    {
        session::revoke(state.cache.as_ref(), tok).await;
    }
    let cookie = session::clear_cookie(
        session::cookies_secure(),
        !state.config.cors_origins.is_empty(),
    );
    ([(SET_COOKIE, cookie)], axum::http::StatusCode::NO_CONTENT).into_response()
}

/// `GET /admin/me`. Runs behind [`super::middleware::require_admin`], so the
/// [`AdminUser`] extension is always present and always an admin.
pub async fn me(Extension(admin): Extension<AdminUser>) -> Json<MeResponse> {
    Json(MeResponse {
        id: admin.id,
        name: admin.name,
        is_admin: true,
    })
}
