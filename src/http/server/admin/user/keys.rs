//! `/user/keys` CRUD — self-service user-key management.
//!
//! All operations are strictly scoped to the session user's id (`SessionUser.id`
//! from the `require_session` middleware). The user_id is NEVER taken from the
//! request body or path parameters.
//!
//! SECURITY:
//! - `list`  : only returns keys belonging to the session user.
//! - `create`: generates key server-side; returns the bare key ONCE in the
//!   response (`api_key`); subsequent reads never include it.
//! - `update`: ownership-checks via `get_user_key` + `owns` before any write.
//!   Cross-user access returns 404 (no existence leak).
//! - `delete`: same ownership check before deletion.

use axum::Extension;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::admin::invalidate;
use crate::admin::session::SessionUser;
use crate::api::error::ApiError;
use crate::api::user_keys::UserKeyView;
use crate::app::AppState;
use crate::pipeline::auth::key_digest;
use crate::store::persistence::records::{UserKey, UserKeyInput};

// ── ownership helper ──────────────────────────────────────────────────────────

/// Returns true iff `key` belongs to `uid`. The caller must pass
/// the record returned by `get_user_key` — compare the stored `user_id`
/// against the session user id; never trust request-supplied ids.
fn owns(key: &UserKey, uid: i64) -> bool {
    key.user_id == uid
}

// ── internal error mapping ────────────────────────────────────────────────────

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError::Internal(e.to_string())
}

// ── request bodies ────────────────────────────────────────────────────────────

/// Body accepted by `POST /user/keys`. Only `label` is accepted;
/// user_id comes from the session, api_key is server-generated.
#[derive(serde::Deserialize)]
pub struct CreateBody {
    pub label: Option<String>,
    /// Present only to detect caller mistakes — sending this field is a 400.
    #[serde(default)]
    pub api_key: Option<String>,
}

/// Body accepted by `PATCH /user/keys/{id}`.
#[derive(serde::Deserialize)]
pub struct UpdateBody {
    pub label: Option<String>,
    pub enabled: bool,
}

// ── handlers ──────────────────────────────────────────────────────────────────

/// `GET /user/keys` — list all keys belonging to the session user.
/// `api_key` is never included in list responses.
pub async fn list(
    State(state): State<AppState>,
    Extension(u): Extension<SessionUser>,
) -> Result<Json<Vec<UserKeyView>>, ApiError> {
    let keys = state
        .persistence
        .list_user_keys(u.id)
        .await
        .map_err(internal)?;
    Ok(Json(keys.into_iter().map(UserKeyView::from).collect()))
}

/// `POST /user/keys` — create a new key for the session user.
///
/// The bare key is generated server-side and returned **once** in
/// `api_key`. The caller must copy it immediately; subsequent reads
/// return only the `key_prefix`.
pub async fn create(
    State(state): State<AppState>,
    Extension(u): Extension<SessionUser>,
    Json(body): Json<CreateBody>,
) -> Result<Json<UserKeyView>, ApiError> {
    if body.api_key.is_some() {
        return Err(ApiError::BadRequest(
            "api_key is not accepted: keys are generated server-side on create".into(),
        ));
    }

    // Mint the key.
    let bare = crate::util::rand::api_key();
    let digest = key_digest(&bare);
    let sealed = state
        .cipher
        .seal(&serde_json::Value::String(bare.clone()))
        .map_err(internal)?;
    let ciphertext = match &sealed {
        serde_json::Value::String(s) => s.clone(),
        other => serde_json::to_string(other).map_err(internal)?,
    };

    let input = UserKeyInput {
        id: None,
        user_id: u.id, // always from session
        api_key_digest: digest,
        api_key_ciphertext: ciphertext,
        label: body.label,
        enabled: true,
    };
    let key = state
        .persistence
        .upsert_user_key(input)
        .await
        .map_err(internal)?;
    invalidate(&state).await;

    let mut view = UserKeyView::from(key);
    view.api_key = Some(bare); // one-time plaintext
    Ok(Json(view))
}

/// `PATCH /user/keys/{id}` — update label/enabled for a key the session user owns.
///
/// Ownership is checked via `get_user_key` + `owns`; cross-user access returns
/// 404 (no existence leak). The existing digest and ciphertext are reused
/// (key material is immutable — rotate by create + delete).
pub async fn update(
    State(state): State<AppState>,
    Extension(u): Extension<SessionUser>,
    Path(id): Path<i64>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<UserKeyView>, ApiError> {
    let existing = state
        .persistence
        .get_user_key(id)
        .await
        .map_err(internal)?
        .filter(|k| owns(k, u.id))
        .ok_or_else(|| ApiError::NotFound("not found".into()))?;

    let input = UserKeyInput {
        id: Some(id),
        user_id: u.id, // always from session
        api_key_digest: existing.api_key_digest,
        api_key_ciphertext: existing.api_key_ciphertext,
        label: body.label,
        enabled: body.enabled,
    };
    let key = state
        .persistence
        .upsert_user_key(input)
        .await
        .map_err(internal)?;
    invalidate(&state).await;

    Ok(Json(UserKeyView::from(key))) // api_key stays None on updates
}

/// `DELETE /user/keys/{id}` — delete a key the session user owns.
///
/// Ownership is checked before deletion; cross-user access returns 404
/// (no existence leak). Returns 204 on success.
pub async fn delete(
    State(state): State<AppState>,
    Extension(u): Extension<SessionUser>,
    Path(id): Path<i64>,
) -> Result<axum::response::Response, ApiError> {
    // Ownership check first — delete_user_key has no built-in guard.
    let _existing = state
        .persistence
        .get_user_key(id)
        .await
        .map_err(internal)?
        .filter(|k| owns(k, u.id))
        .ok_or_else(|| ApiError::NotFound("not found".into()))?;

    state
        .persistence
        .delete_user_key(id)
        .await
        .map_err(internal)?;
    invalidate(&state).await;

    Ok(StatusCode::NO_CONTENT.into_response())
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::persistence::records::UserKey;

    fn make_key(id: i64, user_id: i64) -> UserKey {
        UserKey {
            id,
            user_id,
            api_key_ciphertext: String::new(),
            api_key_digest: "aabbccdd".to_string(),
            label: None,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn owns_returns_true_for_matching_user() {
        let key = make_key(1, 42);
        assert!(owns(&key, 42));
    }

    #[test]
    fn owns_returns_false_for_different_user() {
        let key = make_key(1, 42);
        assert!(!owns(&key, 99));
    }

    /// Simulate cross-user access: filtering with a wrong uid yields None,
    /// which becomes NotFound — no existence leak.
    #[test]
    fn cross_user_filter_yields_not_found() {
        let key = make_key(10, 1); // owned by user 1
        let result = Some(key).filter(|k| owns(k, 2)); // user 2 attempts access
        assert!(
            result.is_none(),
            "cross-user filter must yield None → NotFound"
        );
    }
}
