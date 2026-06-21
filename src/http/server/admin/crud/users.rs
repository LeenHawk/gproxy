//! Users CRUD — special-cased: reads redact the password hash ([`UserView`]),
//! writes take a PLAINTEXT password ([`UserUpsert`]) that is hashed here. On an
//! update with no password supplied, the existing hash is preserved.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use super::{internal, upsert_err};
use crate::admin::invalidate;
use crate::api::error::ApiError;
use crate::api::users::{UserUpsert, UserView};
use crate::app::AppState;
use crate::store::persistence::records::UserInput;

/// `GET /admin/users` — list users as redacted [`UserView`]s.
pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<UserView>>, ApiError> {
    let users = state.persistence.list_users().await.map_err(internal)?;
    Ok(Json(users.into_iter().map(UserView::from).collect()))
}

/// `GET /admin/users/{id}` — one redacted [`UserView`], or 404.
pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<UserView>, ApiError> {
    match state.persistence.get_user(id).await.map_err(internal)? {
        Some(user) => Ok(Json(UserView::from(user))),
        None => Err(ApiError::NotFound("not found".into())),
    }
}

/// `POST /admin/users` — create or update. The plaintext password is hashed;
/// when omitted on an update the existing hash is kept; the hash is never
/// returned.
pub async fn upsert(
    State(state): State<AppState>,
    Json(body): Json<UserUpsert>,
) -> Result<Json<UserView>, ApiError> {
    // Resolve the password hash to store.
    let password = match (&body.password, body.id) {
        // New plaintext supplied → policy-gate, then hash. `None` is the only
        // way to a password-less user (password login disabled).
        (Some(pw), _) => {
            crate::crypto::password::validate_new(pw).map_err(ApiError::BadRequest)?;
            Some(crate::crypto::password::hash(pw).map_err(internal)?)
        }
        // No password on update → keep the existing hash.
        (None, Some(id)) => state
            .persistence
            .get_user(id)
            .await
            .map_err(internal)?
            .and_then(|u| u.password),
        // No password on create → no login until set.
        (None, None) => None,
    };

    let input = UserInput {
        id: body.id,
        name: body.name,
        org_id: body.org_id,
        team_id: body.team_id,
        password,
        enabled: body.enabled,
        is_admin: body.is_admin,
    };
    let user = state
        .persistence
        .upsert_user(input)
        .await
        .map_err(upsert_err)?;
    invalidate(&state).await;
    Ok(Json(UserView::from(user)))
}

/// `DELETE /admin/users/{id}` — 204 on removal, 404 otherwise.
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<axum::response::Response, ApiError> {
    if state.persistence.delete_user(id).await.map_err(internal)? {
        invalidate(&state).await;
        Ok(StatusCode::NO_CONTENT.into_response())
    } else {
        Err(ApiError::NotFound("not found".into()))
    }
}
