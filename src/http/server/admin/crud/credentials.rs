//! Credentials CRUD — special-cased: writes SEAL a plaintext `secret_json`
//! ([`CredentialUpsert`]); reads REDACT the sealed secret ([`CredentialView`]).
//! On an update with no `secret_json`, the stored ciphertext is preserved.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use super::{internal, upsert_err};
use crate::admin::invalidate;
use crate::api::credentials::{CredentialUpsert, CredentialView};
use crate::api::error::ApiError;
use crate::app::AppState;
use crate::store::persistence::records::CredentialInput;

/// `GET /admin/providers/{provider_id}/credentials` — redacted list.
pub async fn list(
    State(state): State<AppState>,
    Path(provider_id): Path<i64>,
) -> Result<Json<Vec<CredentialView>>, ApiError> {
    let creds = state
        .persistence
        .list_credentials(provider_id)
        .await
        .map_err(internal)?;
    Ok(Json(creds.into_iter().map(CredentialView::from).collect()))
}

/// `GET /admin/providers/{provider_id}/credentials/{id}` — one redacted view,
/// or 404. Scoped to the provider so a credential id can only be read in its
/// own provider's context.
pub async fn get(
    State(state): State<AppState>,
    Path((provider_id, id)): Path<(i64, i64)>,
) -> Result<Json<CredentialView>, ApiError> {
    match state
        .persistence
        .get_credential(id)
        .await
        .map_err(internal)?
    {
        Some(c) if c.provider_id == provider_id => Ok(Json(CredentialView::from(c))),
        _ => Err(ApiError::NotFound("not found".into())),
    }
}

/// `POST /admin/providers/{provider_id}/credentials` — create or update. The
/// plaintext `secret_json` is sealed; when omitted on an update the stored
/// ciphertext is kept; on create it is required (400 otherwise). The redacted
/// view is returned.
pub async fn upsert(
    State(state): State<AppState>,
    Path(provider_id): Path<i64>,
    Json(body): Json<CredentialUpsert>,
) -> Result<Json<CredentialView>, ApiError> {
    // Resolve the sealed secret to store.
    let secret_json = match (&body.secret_json, body.id) {
        // New plaintext supplied → seal it.
        (Some(plain), _) => state.cipher.seal(plain).map_err(internal)?,
        // No secret on update → keep the existing (already sealed) value.
        (None, Some(id)) => {
            let existing = state
                .persistence
                .get_credential(id)
                .await
                .map_err(internal)?
                .filter(|c| c.provider_id == provider_id)
                .ok_or_else(|| ApiError::NotFound("not found".into()))?;
            existing.secret_json
        }
        // No secret on create → reject.
        (None, None) => {
            return Err(ApiError::BadRequest(
                "secret_json required on create".into(),
            ));
        }
    };

    let input = CredentialInput {
        id: body.id,
        provider_id,
        name: body.label,
        kind: body.kind,
        secret_json,
        weight: body.weight,
        rpm_limit: body.rpm_limit,
        tpm_limit: body.tpm_limit,
        proxy_url: body.proxy_url,
        tls_fingerprint: body.tls_fingerprint,
        enabled: body.enabled,
    };
    let cred = state
        .persistence
        .upsert_credential(input)
        .await
        .map_err(upsert_err)?;
    invalidate(&state).await;
    Ok(Json(CredentialView::from(cred)))
}

/// `GET /admin/credentials/{id}/secret` — unseal and return the plaintext
/// secret so the edit form can pre-fill it.
pub async fn reveal(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cred = state
        .persistence
        .get_credential(id)
        .await
        .map_err(internal)?
        .ok_or_else(|| ApiError::NotFound("not found".into()))?;
    let plain = state.cipher.open(&cred.secret_json).map_err(internal)?;
    Ok(Json(plain))
}

/// `DELETE /admin/credentials/{id}` — 204 on removal, 404 otherwise.
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<axum::response::Response, ApiError> {
    if state
        .persistence
        .delete_credential(id)
        .await
        .map_err(internal)?
    {
        invalidate(&state).await;
        Ok(StatusCode::NO_CONTENT.into_response())
    } else {
        Err(ApiError::NotFound("not found".into()))
    }
}
