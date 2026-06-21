//! Admin control-plane: caller auth + session store + config CRUD
//! invalidation helper.

pub mod csrf;
pub mod guard;
pub mod login;
pub mod session;

use http::HeaderMap;
use http::header::COOKIE;

use crate::app::AppState;

/// Resolve the admin caller behind a request — THE auth check for every
/// admin-gated surface (native `/admin/*` + `/healthz` + `/metrics` via
/// `require_admin`, and the edge fetch arms). Two accepted forms:
/// 1. session cookie (browser/console): revocable, re-checked against
///    persistence per request ([`session::validate`]);
/// 2. API key of an enabled admin user (headless: curl / Prometheus / CI),
///    header forms only — no `?key=` fallback, admin URLs end up in logs.
///    Resolved against the control-plane snapshot exactly like gateway auth
///    (native reloads it on every config mutation; an edge isolate lazily
///    refreshes it via the §7.2 config-version poll, so key changes land
///    within the poll interval).
///
/// SECURITY — revocation lag on edge: because the API-key form resolves against
/// the cached snapshot, **disabling/deleting an admin key only takes effect on
/// an edge isolate at the next config-version poll** — bounded staleness, not a
/// permanent bypass. Native invalidates the snapshot synchronously on every
/// admin mutation, so revocation there is immediate. The session-cookie form
/// (1) is unaffected on both: [`session::validate`] re-reads persistence every
/// request and re-checks `enabled`/`is_admin` live, so a revoked session dies
/// at once. Operators needing instant key revocation on edge should shorten the
/// poll interval or revoke via the cookie/session path.
///
/// An expired/invalid cookie falls through to the key check, not straight 401.
pub async fn authenticate_admin(
    state: &AppState,
    headers: &HeaderMap,
) -> Option<session::AdminUser> {
    if let Some(token) = headers
        .get(COOKIE)
        .and_then(|h| h.to_str().ok())
        .and_then(session::parse_cookie)
        && let Some(admin) =
            session::validate(state.cache.as_ref(), state.persistence.as_ref(), token).await
    {
        return Some(admin);
    }
    let cp = state.cp();
    let identity = crate::pipeline::auth::authenticate(&cp, headers, None).ok()?;
    identity.user.is_admin.then(|| session::AdminUser {
        id: identity.user.id,
        name: identity.user.name.clone(),
    })
}

/// Resolve the portal session identity from the cookie only (user keys are proxy
/// credentials, NOT a portal login). Admits any enabled user.
pub async fn authenticate_session(
    state: &AppState,
    headers: &HeaderMap,
) -> Option<session::SessionUser> {
    let token = headers
        .get(COOKIE)
        .and_then(|h| h.to_str().ok())
        .and_then(session::parse_cookie)?;
    session::validate_session(state.cache.as_ref(), state.persistence.as_ref(), token).await
}

/// After a config mutation: tell peers to reload — version stamp + pub/sub
/// (see [`invalidation::broadcast`](crate::app::invalidation::broadcast)) —
/// and reload locally now (so this instance serves the change immediately).
/// The write is already durable in persistence; a reload failure is logged,
/// not surfaced.
#[cfg(not(target_arch = "wasm32"))]
pub async fn invalidate(state: &crate::app::AppState) {
    crate::app::invalidation::broadcast(state.cache.as_ref(), b"config").await;
    if let Err(e) = state.reload_snapshot().await {
        tracing::warn!(error = %e, "snapshot reload after admin mutation failed");
    }
}

/// Edge has no synchronous pub/sub invalidation; the §7.2 config-version poll
/// (edge.rs) refreshes the snapshot within one interval. No-op here so CRUD
/// cores compile on wasm.
#[cfg(target_arch = "wasm32")]
pub async fn invalidate(_state: &crate::app::AppState) {}
