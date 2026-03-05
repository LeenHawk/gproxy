use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use gproxy_provider::ChannelId;
use serde::{Deserialize, Serialize};

use crate::AppState;

use super::{Ack, DeleteById, HttpError, authorize_admin, resolve_provider_channel_by_id};

pub(super) async fn query_credentials(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<gproxy_storage::CredentialQuery>,
) -> Result<Json<Vec<gproxy_storage::CredentialQueryRow>>, HttpError> {
    authorize_admin(&headers, &state)?;
    let storage = state.load_storage();
    Ok(Json(
        gproxy_admin::query_credentials(&storage, query).await?,
    ))
}

pub(super) async fn upsert_credential(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(mut payload): Json<UpsertCredentialPayload>,
) -> Result<Json<UpsertCredentialAck>, HttpError> {
    authorize_admin(&headers, &state)?;
    let Some(channel) = resolve_provider_channel_by_id(&state, payload.provider_id).await? else {
        return Err(HttpError::new(
            StatusCode::NOT_FOUND,
            format!("not found: provider {}", payload.provider_id),
        ));
    };
    let mut credential =
        serde_json::from_str::<gproxy_provider::ChannelCredential>(payload.secret_json.as_str())
            .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, err.to_string()))?;

    maybe_detect_and_fill_project_id(&state, &channel, &mut credential).await?;
    payload.secret_json = serde_json::to_string(&credential)
        .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, err.to_string()))?;

    let id = if let Some(id) = payload.id {
        let rows = state
            .load_storage()
            .list_credentials(&gproxy_storage::CredentialQuery {
                provider_id: gproxy_storage::Scope::Eq(payload.provider_id),
                kind: gproxy_storage::Scope::All,
                enabled: gproxy_storage::Scope::All,
                limit: Some(1000),
            })
            .await
            .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
        if !rows.iter().any(|row| row.id == id) {
            return Err(HttpError::new(
                StatusCode::NOT_FOUND,
                format!("not found: credential {id}"),
            ));
        }
        gproxy_admin::upsert_credential(
            &state.storage_writes,
            gproxy_storage::CredentialWrite {
                id,
                provider_id: payload.provider_id,
                name: payload.name.clone(),
                kind: payload.kind.clone(),
                settings_json: payload.settings_json.clone(),
                secret_json: payload.secret_json.clone(),
                enabled: payload.enabled,
            },
        )
        .await?;
        id
    } else {
        state
            .load_storage()
            .create_credential(
                payload.provider_id,
                payload.name.as_deref(),
                payload.kind.as_str(),
                payload.settings_json.as_deref(),
                payload.secret_json.as_str(),
                payload.enabled,
            )
            .await
            .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
    };

    state.upsert_provider_credential_in_memory(
        &channel,
        gproxy_provider::CredentialRef {
            id,
            label: payload.name.clone(),
            credential,
        },
    );
    Ok(Json(UpsertCredentialAck { ok: true, id }))
}

pub(super) async fn maybe_detect_and_fill_project_id(
    state: &AppState,
    channel: &ChannelId,
    credential: &mut gproxy_provider::ChannelCredential,
) -> Result<(), HttpError> {
    let settings = if let Some(provider) = state.config.load().providers.get(channel) {
        provider.settings.clone()
    } else {
        gproxy_provider::parse_provider_settings_json_for_channel(channel, "{}")
            .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, err.to_string()))?
    };

    let http = state.load_http();
    gproxy_provider::ensure_project_id_for_credential(
        http.as_ref(),
        channel,
        &settings,
        credential,
    )
    .await
    .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, err.to_string()))?;

    Ok(())
}

pub(super) async fn delete_credential(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DeleteById>,
) -> Result<Json<Ack>, HttpError> {
    authorize_admin(&headers, &state)?;
    let storage = state.load_storage();
    let rows = storage
        .list_credentials(&gproxy_storage::CredentialQuery {
            provider_id: gproxy_storage::Scope::All,
            kind: gproxy_storage::Scope::All,
            enabled: gproxy_storage::Scope::All,
            limit: None,
        })
        .await
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    if let Some(credential_row) = rows.into_iter().find(|row| row.id == payload.id)
        && let Some(channel) =
            resolve_provider_channel_by_id(&state, credential_row.provider_id).await?
    {
        let _ = state.delete_provider_credential_in_memory(&channel, payload.id);
    }
    gproxy_admin::delete_credential(&state.storage_writes, payload.id).await?;
    Ok(Json(Ack { ok: true }))
}

#[derive(Debug, Deserialize)]
pub(super) struct UpsertCredentialPayload {
    #[serde(default)]
    pub id: Option<i64>,
    pub provider_id: i64,
    pub name: Option<String>,
    pub kind: String,
    pub settings_json: Option<String>,
    pub secret_json: String,
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct UpsertCredentialAck {
    pub ok: bool,
    pub id: i64,
}
