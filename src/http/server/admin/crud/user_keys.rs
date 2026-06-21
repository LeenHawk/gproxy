//! User-keys CRUD — special-cased: the bare key is GENERATED server-side on
//! create (digest derived, ciphertext sealed, the key itself returned ONCE in
//! the response); updates touch only label/enabled. Caller-supplied key
//! material is rejected here — the import path is its sole entrance. Reads
//! expose only a short `key_prefix` ([`UserKeyView`]).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use super::{internal, upsert_err};
use crate::admin::invalidate;
use crate::api::error::ApiError;
use crate::api::user_keys::{UserKeyUpsert, UserKeyView};
use crate::app::AppState;
use crate::pipeline::auth::key_digest;
use crate::store::persistence::records::UserKeyInput;

/// `GET /admin/users/{user_id}/keys` — redacted list.
pub async fn list(
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
) -> Result<Json<Vec<UserKeyView>>, ApiError> {
    let keys = state
        .persistence
        .list_user_keys(user_id)
        .await
        .map_err(internal)?;
    Ok(Json(keys.into_iter().map(UserKeyView::from).collect()))
}

/// `POST /admin/users/{user_id}/keys` — create or update. On create the key is
/// generated server-side (CSPRNG) and the bare value is returned ONCE in the
/// response; on update the stored digest + ciphertext are kept (key material
/// is immutable — rotate by create + delete). A caller-supplied `api_key` is
/// rejected outright.
pub async fn upsert(
    State(state): State<AppState>,
    Path(user_id): Path<i64>,
    Json(body): Json<UserKeyUpsert>,
) -> Result<Json<UserKeyView>, ApiError> {
    if body.api_key.is_some() {
        return Err(ApiError::BadRequest(
            "api_key is not accepted: keys are generated server-side on create \
             (external key material is import-only)"
                .into(),
        ));
    }

    // Resolve the (digest, ciphertext) pair to store; `bare` only on create.
    let (digest, ciphertext, bare) = match body.id {
        // Create → mint the key here.
        None => {
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
            (digest, ciphertext, Some(bare))
        }
        // Update → keep the existing digest + ciphertext.
        Some(id) => {
            let existing = state
                .persistence
                .get_user_key(id)
                .await
                .map_err(internal)?
                .filter(|k| k.user_id == user_id)
                .ok_or_else(|| ApiError::NotFound("not found".into()))?;
            (existing.api_key_digest, existing.api_key_ciphertext, None)
        }
    };

    let input = UserKeyInput {
        id: body.id,
        user_id,
        api_key_digest: digest,
        api_key_ciphertext: ciphertext,
        label: body.label,
        enabled: body.enabled,
    };
    let key = state
        .persistence
        .upsert_user_key(input)
        .await
        .map_err(upsert_err)?;
    invalidate(&state).await;
    let mut view = UserKeyView::from(key);
    view.api_key = bare;
    Ok(Json(view))
}

/// `DELETE /admin/user-keys/{id}` — 204 on removal, 404 otherwise.
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<axum::response::Response, ApiError> {
    if state
        .persistence
        .delete_user_key(id)
        .await
        .map_err(internal)?
    {
        invalidate(&state).await;
        Ok(StatusCode::NO_CONTENT.into_response())
    } else {
        Err(ApiError::NotFound("not found".into()))
    }
}
