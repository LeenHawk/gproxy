//! Cross-target admin/portal HTTP dispatcher.
//!
//! This module is the architecture-proving core for serving `/admin/*` and
//! `/user/*` on the edge (wasm) worker. It is gated `cfg(any(wasm32, test))`:
//! it compiles into the wasm edge build (which calls [`dispatch`] from
//! `edge/mod.rs::fetch`) AND into native **test** builds (so the dispatcher can
//! be driven by native integration tests), but is skipped in native release
//! builds to avoid dead code there — the native server has its own axum router.
//!
//! Every handler returns PURE DATA ([`Resp`], no `web_sys`), so a native test
//! can assert on `(status, body)` directly. The wasm `edge/mod.rs` converts the
//! returned `Resp` into a `web_sys::Response`.

pub(crate) mod auth;
pub(crate) mod authz;
pub mod crud;
pub(crate) mod login_flows;
pub(crate) mod nested;
pub(crate) mod observability;
pub(crate) mod portal;
pub(crate) mod settings;
pub(crate) mod special;

use bytes::Bytes;
use http::request::Parts;
use http::{HeaderName, HeaderValue, Method, StatusCode};
use serde::de::DeserializeOwned;

use crate::admin::guard::{guard_admin, guard_session};
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::persistence::records::AuditLogInput;

/// Pure HTTP response data (no `web_sys`) so the dispatcher is natively
/// testable. The wasm edge converts this into a `web_sys::Response`.
#[derive(Debug)]
pub struct Resp {
    pub status: StatusCode,
    pub headers: Vec<(HeaderName, HeaderValue)>,
    pub body: Vec<u8>,
}

impl Resp {
    /// JSON response with the given status and `content-type: application/json`.
    pub(crate) fn json(status: u16, value: &impl serde::Serialize) -> Result<Resp, ApiError> {
        let body = serde_json::to_vec(value).map_err(|e| ApiError::Internal(e.to_string()))?;
        Ok(Resp {
            status: StatusCode::from_u16(status).expect("caller passes a valid status"),
            headers: vec![(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            )],
            body,
        })
    }

    /// Empty `204 No Content` (delete success), matching native CRUD semantics.
    pub(crate) fn no_content() -> Resp {
        Resp {
            status: StatusCode::NO_CONTENT,
            headers: vec![],
            body: vec![],
        }
    }
}

// ── Pure parse helpers (cross-target; the web_sys builders live in edge/http) ──

/// Split a URI path into non-empty segments: `/a/b/c` → `["a", "b", "c"]`.
pub(crate) fn segments(parts: &Parts) -> Vec<&str> {
    parts
        .uri
        .path()
        .split('/')
        .filter(|s| !s.is_empty())
        .collect()
}

/// Parse a path segment as `i64`, mapping failures to [`ApiError::BadRequest`].
pub(crate) fn parse_i64(seg: &str) -> Result<i64, ApiError> {
    seg.parse::<i64>()
        .map_err(|_| ApiError::BadRequest(format!("invalid id: {seg}")))
}

/// Deserialize a JSON request body, mapping errors to [`ApiError::BadRequest`].
pub(crate) fn json_body<T: DeserializeOwned>(body: &Bytes) -> Result<T, ApiError> {
    serde_json::from_slice(body)
        .map_err(|e| ApiError::BadRequest(format!("invalid JSON body: {e}")))
}

/// Deserialize URL-encoded query params from `parts.uri.query()`. An absent
/// query is treated as empty. Now cross-target: `serde_urlencoded` is in the
/// top-level `[dependencies]` (not wasm-gated), enabling this helper in both
/// edge and native test builds.
#[allow(dead_code)]
pub(crate) fn query<T: DeserializeOwned>(parts: &Parts) -> Result<T, ApiError> {
    serde_urlencoded::from_str(parts.uri.query().unwrap_or(""))
        .map_err(|e| ApiError::BadRequest(format!("invalid query: {e}")))
}

/// Map a persistence error to a 500 (the cause is logged, not leaked).
pub(crate) fn internal<E: std::fmt::Display>(e: E) -> ApiError {
    ApiError::Internal(e.to_string())
}

/// Write an audit log entry. Direct `await` (no `tokio::spawn`) so this works
/// on edge (wasm) where spawning is unavailable. Native login/logout go through
/// their own `record_audit` (spawn-based); this helper is edge/test only.
pub(crate) async fn audit(state: &AppState, input: AuditLogInput) {
    let _ = state.persistence.append_audit_log(input).await;
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

/// Route an `/admin/*` or `/user/*` request to its handler, then audit the
/// request when it is a mutation (non-GET) performed by an authenticated user —
/// mirroring the native audit middleware, which records every non-GET admin
/// request. `login`/`logout` self-audit (login.success/fail), so they're skipped
/// here. Returns `Some(result)` when handled, `None` to fall through (404).
pub async fn dispatch(
    state: &AppState,
    parts: &Parts,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let result = route(state, parts, body).await?;
    audit_mutation(state, parts, &result).await;
    Some(result)
}

/// Audit a mutating edge request (parity with the native `audit` middleware,
/// which is inner to `require_admin`/`require_session` — so only authenticated
/// requests are recorded; a 401/403 from the guard is not). Logs method + path +
/// response status + actor, never the body. `login`/`logout` self-audit.
async fn audit_mutation(state: &AppState, parts: &Parts, result: &Result<Resp, ApiError>) {
    if parts.method == Method::GET {
        return;
    }
    let segs = segments(parts);
    if matches!(segs.as_slice(), ["admin", "login"] | ["admin", "logout"]) {
        return; // self-audited
    }
    // Resolve the actor the same way the handler's guard did. If unauthenticated,
    // the handler already returned 401/403 and native wouldn't have audited it.
    let actor = if segs.first() == Some(&"user") {
        crate::admin::authenticate_session(state, &parts.headers)
            .await
            .map(|u| (u.id, u.name))
    } else {
        crate::admin::authenticate_admin(state, &parts.headers)
            .await
            .map(|u| (u.id, u.name))
    };
    let Some((actor_id, actor_name)) = actor else {
        return;
    };
    let status = match result {
        Ok(r) => r.status.as_u16() as i64,
        Err(e) => e.status().as_u16() as i64,
    };
    audit(
        state,
        AuditLogInput {
            actor_id: Some(actor_id),
            actor_name: Some(actor_name),
            action: parts.method.as_str().to_owned(),
            target: parts.uri.path().to_owned(),
            status,
            source_ip: auth::edge_client_ip(&parts.headers),
        },
    )
    .await;
}

/// Route an `/admin/*` or `/user/*` request to its handler.
///
/// Returns `Some(result)` when the path is a route we handle; `None` to fall
/// through (caller renders 404). Each handler runs its auth guard first, then
/// the logic, returning `Result<Resp, ApiError>` as pure data.
async fn route(state: &AppState, parts: &Parts, body: &Bytes) -> Option<Result<Resp, ApiError>> {
    // 0. Public auth endpoints (login/logout): no guard, no CSRF required.
    //    Must come BEFORE the guarded arms so a cookie-less login POST is not
    //    refused by guard_admin.
    let segs = segments(parts);
    match (&parts.method, segs.as_slice()) {
        (&Method::POST, ["admin", "login"]) => {
            return Some(auth::login(state, parts, body).await);
        }
        (&Method::POST, ["admin", "logout"]) => {
            return Some(auth::logout(state, parts).await);
        }
        _ => {}
    }

    // Provider create seeds the channel's default routing rules; channel is
    // immutable on update. Must resolve BEFORE the generic CRUD providers upsert.
    if let (&Method::POST, ["admin", "providers"]) = (&parts.method, segs.as_slice()) {
        return Some(crud::create_provider_seeded(state, parts, body).await);
    }
    // Reset a provider's routing rules to the channel defaults.
    if let (&Method::POST, ["admin", "providers", pid, "routing-rules", "reset"]) =
        (&parts.method, segs.as_slice())
    {
        return Some(crud::reset_routing(state, parts, pid).await);
    }

    // 1. Try standard CRUD entities (providers/routes/aliases/rule-sets/orgs).
    if let Some(r) = crud::dispatch(state, parts, body).await {
        return Some(r);
    }

    // 2. Try nested CRUD entities (teams/models/members/rules/routing-rules/provider-rule-sets).
    if let Some(r) = nested::dispatch(state, parts, body).await {
        return Some(r);
    }

    // 3. Instance settings (no per-id routes).
    if let Some(r) = settings::dispatch(state, parts, body).await {
        return Some(r);
    }

    // 4. Authz-scoped entities (route-permissions / rate-limits / quotas).
    if let Some(r) = authz::dispatch(state, parts, body).await {
        return Some(r);
    }

    // 5. Read-only observability (usage / rollups / audit / logs / cred-status).
    if let Some(r) = observability::dispatch(state, parts, body).await {
        return Some(r);
    }

    // 6. Special admin CRUD (user-keys / users / credentials) with server-side
    //    crypto: key gen + seal, password hash, secret seal, redaction.
    //    Must come BEFORE the identity arm (step 7) and AFTER nested (step 2)
    //    so the 4-seg `users/{uid}/keys` arm is evaluated before `users/{id}`.
    if let Some(r) = special::dispatch(state, parts, body).await {
        return Some(r);
    }

    // 7. Portal `/user/*` endpoints (session-scoped). Evaluated BEFORE the identity
    //    arm below so these explicit arms win over the catch-all. Disjoint from
    //    `/user/me` which is handled in step 8.
    if let Some(r) = portal::dispatch(state, parts, body).await {
        return Some(r);
    }

    // 8. Login-flows (`/admin/login-flows/*`) and explicit 501 degradations
    //    (`/admin/update/*`, `/admin/credentials/{id}/usage`). Evaluated after
    //    special (step 6) so the 3-seg credentials arms there win; the
    //    login-flows arms are disjoint from all prior steps.
    if let Some(r) = login_flows::dispatch(state, parts, body).await {
        return Some(r);
    }

    // 9. Identity endpoints.
    let r = match (&parts.method, segs.as_slice()) {
        (&Method::GET, ["admin", "me"]) => admin_me(state, parts).await,
        (&Method::GET, ["user", "me"]) => user_me(state, parts).await,
        _ => return None,
    };
    Some(r)
}

// ── Handlers (auth guard first, then logic) ───────────────────────────────────

/// `GET /admin/me` — the authenticated admin identity.
async fn admin_me(state: &AppState, parts: &Parts) -> Result<Resp, ApiError> {
    let u = guard_admin(state, parts).await?;
    Resp::json(
        200,
        &serde_json::json!({ "id": u.id, "name": u.name, "is_admin": true }),
    )
}

/// `GET /user/me` — the portal session identity (admits any enabled user).
/// Org/team ids are resolved to human names for the portal (parity with the
/// native `user::me::me` handler).
async fn user_me(state: &AppState, parts: &Parts) -> Result<Resp, ApiError> {
    let u = guard_session(state, parts).await?;
    let org_name = state
        .persistence
        .get_org(u.org_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .map(|o| o.name);
    let team_name = match u.team_id {
        Some(tid) => state
            .persistence
            .get_team(tid)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .map(|t| t.name),
        None => None,
    };
    Resp::json(
        200,
        &serde_json::json!({
            "id": u.id,
            "name": u.name,
            "is_admin": u.is_admin,
            "org_id": u.org_id,
            "org_name": org_name,
            "team_id": u.team_id,
            "team_name": team_name,
        }),
    )
}

#[cfg(test)]
mod tests;
