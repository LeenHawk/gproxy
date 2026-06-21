//! `GET /user/me` — returns the session user's identity.

use axum::extract::State;
use axum::{Extension, Json};

use crate::admin::session::SessionUser;
use crate::api::error::ApiError;
use crate::app::AppState;

/// Portal identity shape returned by `GET /user/me`. Org/team are resolved to
/// human names (the portal shows names, not raw ids); `None` when the lookup
/// finds no record (e.g. a since-deleted team).
#[derive(serde::Serialize)]
pub struct UserMe {
    pub id: i64,
    pub name: String,
    pub is_admin: bool,
    pub org_id: i64,
    pub org_name: Option<String>,
    pub team_id: Option<i64>,
    pub team_name: Option<String>,
}

/// Resolve a [`SessionUser`] into the portal identity, looking up org/team names.
pub async fn build_user_me(state: &AppState, u: SessionUser) -> Result<UserMe, ApiError> {
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
    Ok(UserMe {
        id: u.id,
        name: u.name,
        is_admin: u.is_admin,
        org_id: u.org_id,
        org_name,
        team_id: u.team_id,
        team_name,
    })
}

/// `GET /user/me` — reflect the session identity back to the caller.
/// `user_id` always comes from the validated `SessionUser` extension;
/// no request-supplied id is accepted.
pub async fn me(
    State(state): State<AppState>,
    Extension(u): Extension<SessionUser>,
) -> Result<Json<UserMe>, ApiError> {
    Ok(Json(build_user_me(&state, u).await?))
}
