//! Cross-target admin/portal HTTP dispatcher.
//!
//! This is the sole business implementation for `/admin/*` and `/user/*` on
//! both native axum and wasm edge hosts. Host adapters only translate framework
//! request/response types.
//!
//! Every handler returns PURE DATA ([`Resp`], no `web_sys`), so a native test
//! can assert on `(status, body)` directly. The wasm `edge/mod.rs` converts the
//! returned `Resp` into a `web_sys::Response`.

pub(crate) mod auth;
pub(crate) mod authz;
pub(crate) mod batch;
pub(crate) mod credential_ops;
pub mod crud;
mod host;
pub(crate) mod login_flows;
pub(crate) mod nested;
pub(crate) mod observability;
pub(crate) mod portal;
pub(crate) mod provider_ops;
pub(crate) mod settings;
pub(crate) mod special;
mod update;

use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method, StatusCode, Uri};
use serde::de::DeserializeOwned;

use crate::admin::guard::{RequestMetadata, guard_admin, guard_session};
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::persistence::records::AuditLogInput;

/// Target-independent dispatcher request metadata. The raw body remains a
/// separate [`Bytes`] argument so adapters can pass it without another copy.
#[derive(Clone)]
pub struct Request {
    pub method: Method,
    pub uri: Uri,
    pub headers: HeaderMap,
    source_ip: Option<String>,
}

impl Request {
    pub fn new(method: Method, uri: Uri, headers: HeaderMap) -> Self {
        Self {
            method,
            uri,
            headers,
            source_ip: None,
        }
    }

    pub fn with_source_ip(mut self, source_ip: Option<String>) -> Self {
        self.source_ip = source_ip;
        self
    }

    fn source_ip(&self) -> Option<String> {
        self.source_ip
            .clone()
            .or_else(|| auth::edge_client_ip(&self.headers))
    }
}

impl RequestMetadata for Request {
    fn method(&self) -> &Method {
        &self.method
    }

    fn headers(&self) -> &HeaderMap {
        &self.headers
    }
}

/// Pure HTTP response data converted by native axum or wasm `web_sys` adapters.
#[derive(Debug)]
pub struct Resp {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl Resp {
    /// JSON response with the given status and `content-type: application/json`.
    pub(crate) fn json(status: u16, value: &impl serde::Serialize) -> Result<Resp, ApiError> {
        let body =
            Bytes::from(serde_json::to_vec(value).map_err(|e| ApiError::Internal(e.to_string()))?);
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        Ok(Resp {
            status: StatusCode::from_u16(status).expect("caller passes a valid status"),
            headers,
            body,
        })
    }

    /// Empty `204 No Content` (delete success), matching native CRUD semantics.
    pub(crate) fn no_content() -> Resp {
        Resp {
            status: StatusCode::NO_CONTENT,
            headers: HeaderMap::new(),
            body: Bytes::new(),
        }
    }

    fn method_not_allowed(allow: &'static str) -> Resp {
        let mut headers = HeaderMap::new();
        headers.insert(http::header::ALLOW, HeaderValue::from_static(allow));
        Resp {
            status: StatusCode::METHOD_NOT_ALLOWED,
            headers,
            body: Bytes::new(),
        }
    }

    fn from_error(error: ApiError) -> Resp {
        let extra = error.extra_headers();
        let (status, body) = error.to_parts();
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        for (name, value) in extra {
            headers.append(name, value);
        }
        Resp {
            status,
            headers,
            body: Bytes::from(body),
        }
    }
}

// ── Pure parse helpers (cross-target; the web_sys builders live in edge/http) ──

/// Split a URI path into non-empty segments: `/a/b/c` → `["a", "b", "c"]`.
pub(crate) fn segments(parts: &Request) -> Vec<&str> {
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
/// query is treated as empty.
pub(crate) fn query<T: DeserializeOwned>(parts: &Request) -> Result<T, ApiError> {
    serde_urlencoded::from_str(parts.uri.query().unwrap_or(""))
        .map_err(|e| ApiError::BadRequest(format!("invalid query: {e}")))
}

/// Map a persistence error to a 500 (the cause is logged, not leaked).
pub(crate) fn internal<E: std::fmt::Display>(e: E) -> ApiError {
    ApiError::Internal(e.to_string())
}

/// Write an audit log entry. Direct `await` keeps the behavior available on
/// edge runtimes where spawning is unavailable.
pub(crate) async fn audit(state: &AppState, input: AuditLogInput) {
    let _ = state.persistence.append_audit_log(input).await;
}

// ── Dispatcher ────────────────────────────────────────────────────────────────

/// Route an `/admin/*` or `/user/*` request to its handler, then audit the
/// request when it is a mutation (non-GET) performed by an authenticated user.
/// `login`/`logout` self-audit (login.success/fail), so they're skipped here.
/// Returns `Some(result)` when handled, `None` to fall through (404).
pub async fn dispatch(state: &AppState, parts: &Request, body: &Bytes) -> Option<Resp> {
    if parts.method == Method::HEAD {
        let mut get = parts.clone();
        get.method = Method::GET;
        return dispatch_result(state, &get, body).await.map(|result| {
            let mut response = result.unwrap_or_else(Resp::from_error);
            response.body = Bytes::new();
            response
        });
    }
    dispatch_result(state, parts, body)
        .await
        .map(|result| result.unwrap_or_else(Resp::from_error))
}

async fn dispatch_result(
    state: &AppState,
    parts: &Request,
    body: &Bytes,
) -> Option<Result<Resp, ApiError>> {
    let result = route(state, parts, body).await?;
    audit_mutation(state, parts, &result).await;
    Some(result)
}

/// Audit a mutating request only when its guard authenticated an actor. Logs
/// method + path + response status + actor, never the body. `login`/`logout`
/// self-audit.
async fn audit_mutation(state: &AppState, parts: &Request, result: &Result<Resp, ApiError>) {
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
            source_ip: parts.source_ip(),
        },
    )
    .await;
}

/// Route an `/admin/*` or `/user/*` request to its handler.
///
/// Returns `Some(result)` when the path is a route we handle; `None` to fall
/// through (caller renders 404). Each handler runs its auth guard first, then
/// the logic, returning `Result<Resp, ApiError>` as pure data.
async fn route(state: &AppState, parts: &Request, body: &Bytes) -> Option<Result<Resp, ApiError>> {
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

    // Batch ops: POST /admin/batch/{entity}.
    if let Some(r) = batch::dispatch(state, parts, body).await {
        return Some(r);
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

    // Live provider operations that are not CRUD (currently upstream models).
    if let Some(r) = provider_ops::dispatch(state, parts).await {
        return Some(r);
    }

    // Live credential operations that call the upstream account API.
    if let Some(r) = credential_ops::dispatch(state, parts, body).await {
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
    //    (`/admin/update/*`). Evaluated after
    //    special (step 6) so the 3-seg credentials arms there win; the
    //    login-flows arms are disjoint from all prior steps.
    if let Some(r) = login_flows::dispatch(state, parts, body).await {
        return Some(r);
    }

    if let Some(r) = host::dispatch(state, parts, body).await {
        return Some(r);
    }

    if let Some(r) = update::dispatch(state, parts, body).await {
        return Some(r);
    }

    // 9. Identity endpoints.
    let r = match (&parts.method, segs.as_slice()) {
        (&Method::GET, ["admin", "me"]) => admin_me(state, parts).await,
        (&Method::GET, ["user", "me"]) => user_me(state, parts).await,
        _ => {
            return allowed_methods(segs.as_slice())
                .map(|allow| Ok(Resp::method_not_allowed(allow)));
        }
    };
    Some(r)
}

fn allowed_methods(segments: &[&str]) -> Option<&'static str> {
    match segments {
        ["admin", "login" | "logout"] => Some("POST"),
        ["admin", "me"] => Some("GET,HEAD"),
        ["admin", "autostart"] => Some("GET,HEAD,PUT"),
        [
            "admin",
            "orgs" | "providers" | "routes" | "aliases" | "price-rules" | "rule-sets"
            | "instance-settings" | "users",
        ] => Some("GET,HEAD,POST"),
        [
            "admin",
            "usage"
            | "usage-summary"
            | "usage-rollups"
            | "credential-statuses"
            | "logs"
            | "audit"
            | "tls-presets",
        ] => Some("GET,HEAD"),
        ["admin", "batch", _] => Some("POST"),
        [
            "admin",
            "orgs" | "providers" | "routes" | "aliases" | "price-rules" | "rule-sets" | "users",
            _,
        ] => Some("GET,HEAD,DELETE"),
        [
            "admin",
            "credentials" | "user-keys" | "teams" | "provider-models" | "route-members" | "rules"
            | "routing-rules" | "provider-rule-sets" | "route-permissions" | "rate-limits"
            | "quotas",
            _,
        ] => Some("DELETE"),
        ["admin", "route-permissions" | "rate-limits" | "quotas"] => Some("GET,HEAD,POST"),
        ["admin", "login-flows", "start" | "complete" | "cookie"] => Some("POST"),
        ["admin", "login-flows", "device", "start" | "poll"] => Some("POST"),
        ["admin", "update", "check" | "status"] => Some("GET,HEAD"),
        ["admin", "update", "apply"] => Some("POST"),
        ["admin", "connectivity", "test"] => Some("POST"),
        ["admin", "orgs", _, "teams"]
        | [
            "admin",
            "providers",
            _,
            "models" | "credentials" | "routing-rules" | "rule-sets",
        ]
        | ["admin", "routes", _, "members"]
        | ["admin", "rule-sets", _, "rules"]
        | ["admin", "users", _, "keys"] => Some("GET,HEAD,POST"),
        ["admin", "providers", _, "upstream-models"]
        | ["admin", "credentials", _, "status" | "secret" | "usage"]
        | ["admin", "logs", _, "downstream" | "upstream"] => Some("GET,HEAD"),
        ["admin", "credentials", _, "rate-limit-reset-credit"]
        | ["admin", "providers", _, "routing-rules", "reset"] => Some("POST"),
        ["admin", "providers", _, "credentials", _] => Some("GET,HEAD"),
        [
            "user",
            "me" | "usage" | "usage-rollups" | "quota" | "rate-limits" | "route-permissions",
        ] => Some("GET,HEAD"),
        ["user", "keys"] => Some("GET,HEAD,POST"),
        ["user", "keys", _] => Some("PATCH,DELETE"),
        ["user", "change-password"] => Some("POST"),
        _ => None,
    }
}

// ── Handlers (auth guard first, then logic) ───────────────────────────────────

/// `GET /admin/me` — the authenticated admin identity.
async fn admin_me(state: &AppState, parts: &Request) -> Result<Resp, ApiError> {
    let u = guard_admin(state, parts).await?;
    Resp::json(
        200,
        &serde_json::json!({ "id": u.id, "name": u.name, "is_admin": true }),
    )
}

/// `GET /user/me` — the portal session identity (admits any enabled user).
/// Org/team ids are resolved to human names for the portal (parity with the
/// native `user::me::me` handler).
async fn user_me(state: &AppState, parts: &Request) -> Result<Resp, ApiError> {
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
