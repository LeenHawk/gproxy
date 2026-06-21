//! `/user/quota`, `/user/rate-limits`, `/user/route-permissions` — portal
//! read-only endpoints that surface the effective three-layer authz rules for
//! the authenticated user (§F7a Task 4).
//!
//! **Security invariant:** scope_id values come ONLY from the `SessionUser`
//! extension set by `require_session` — user/team/org ids are never accepted
//! from request parameters or body. The admin `ScopeQuery` is deliberately NOT
//! reused here.
//!
//! **Layer order:** user → team (skipped when `team_id` is `None`) → org.
//!
//! **Wire shape:** each item is `{ "source": "user"|"team"|"org", ...rule fields }`
//! because `Effective<T>` uses `#[serde(flatten)]`.

use axum::Extension;
use axum::Json;
use axum::extract::State;

use crate::admin::session::SessionUser;
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::persistence::records::authz::{Quota, RateLimit, RoutePermission, Scope};

fn internal(e: anyhow::Error) -> ApiError {
    ApiError::Internal(e.to_string())
}

/// A rule wrapper that tags each record with its effective source layer.
///
/// Serialises as `{ "source": "<layer>", <...rule fields flattened> }`.
#[derive(serde::Serialize)]
pub struct Effective<T: serde::Serialize> {
    pub source: &'static str,
    #[serde(flatten)]
    pub rule: T,
}

/// `GET /user/route-permissions` — all route permissions effective for the
/// authenticated user, across user → team → org layers.
pub async fn route_permissions(
    State(state): State<AppState>,
    Extension(u): Extension<SessionUser>,
) -> Result<Json<Vec<Effective<RoutePermission>>>, ApiError> {
    let mut out = Vec::new();
    for p in state
        .persistence
        .list_route_permissions(Scope::User, u.id)
        .await
        .map_err(internal)?
    {
        out.push(Effective {
            source: "user",
            rule: p,
        });
    }
    if let Some(tid) = u.team_id {
        for p in state
            .persistence
            .list_route_permissions(Scope::Team, tid)
            .await
            .map_err(internal)?
        {
            out.push(Effective {
                source: "team",
                rule: p,
            });
        }
    }
    for p in state
        .persistence
        .list_route_permissions(Scope::Org, u.org_id)
        .await
        .map_err(internal)?
    {
        out.push(Effective {
            source: "org",
            rule: p,
        });
    }
    Ok(Json(out))
}

/// `GET /user/rate-limits` — all rate limits effective for the authenticated
/// user, across user → team → org layers.
pub async fn rate_limits(
    State(state): State<AppState>,
    Extension(u): Extension<SessionUser>,
) -> Result<Json<Vec<Effective<RateLimit>>>, ApiError> {
    let mut out = Vec::new();
    for r in state
        .persistence
        .list_rate_limits(Scope::User, u.id)
        .await
        .map_err(internal)?
    {
        out.push(Effective {
            source: "user",
            rule: r,
        });
    }
    if let Some(tid) = u.team_id {
        for r in state
            .persistence
            .list_rate_limits(Scope::Team, tid)
            .await
            .map_err(internal)?
        {
            out.push(Effective {
                source: "team",
                rule: r,
            });
        }
    }
    for r in state
        .persistence
        .list_rate_limits(Scope::Org, u.org_id)
        .await
        .map_err(internal)?
    {
        out.push(Effective {
            source: "org",
            rule: r,
        });
    }
    Ok(Json(out))
}

/// `GET /user/quota` — quota records effective for the authenticated user, at
/// each layer that has one (at most one per layer, at most three total).
///
/// Returns an empty array when no quota exists at any layer — the frontend
/// should render "no quota" in that case.
pub async fn quota(
    State(state): State<AppState>,
    Extension(u): Extension<SessionUser>,
) -> Result<Json<Vec<Effective<Quota>>>, ApiError> {
    let mut out = Vec::new();
    if let Some(q) = state
        .persistence
        .get_quota(Scope::User, u.id)
        .await
        .map_err(internal)?
    {
        out.push(Effective {
            source: "user",
            rule: q,
        });
    }
    if let Some(tid) = u.team_id
        && let Some(q) = state
            .persistence
            .get_quota(Scope::Team, tid)
            .await
            .map_err(internal)?
    {
        out.push(Effective {
            source: "team",
            rule: q,
        });
    }
    if let Some(q) = state
        .persistence
        .get_quota(Scope::Org, u.org_id)
        .await
        .map_err(internal)?
    {
        out.push(Effective {
            source: "org",
            rule: q,
        });
    }
    Ok(Json(out))
}
