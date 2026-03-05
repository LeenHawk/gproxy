use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use gproxy_provider::{ChannelId, parse_credential_pick_mode_from_provider_settings_value};
use serde::{Deserialize, Serialize};

use crate::AppState;

use super::{Ack, DeleteById, HttpError, authorize_admin};

pub(super) async fn resolve_provider_channel_by_id(
    state: &AppState,
    id: i64,
) -> Result<Option<ChannelId>, HttpError> {
    let storage = state.load_storage();
    let rows = storage
        .list_providers(&gproxy_storage::ProviderQuery {
            channel: gproxy_storage::Scope::All,
            name: gproxy_storage::Scope::All,
            enabled: gproxy_storage::Scope::All,
            limit: None,
        })
        .await
        .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    Ok(rows
        .into_iter()
        .find(|row| row.id == id)
        .map(|row| ChannelId::parse(row.channel.as_str())))
}

pub(super) async fn query_providers(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(query): Json<gproxy_storage::ProviderQuery>,
) -> Result<Json<Vec<gproxy_storage::ProviderQueryRow>>, HttpError> {
    authorize_admin(&headers, &state)?;
    let storage = state.load_storage();
    Ok(Json(gproxy_admin::query_providers(&storage, query).await?))
}

pub(super) async fn upsert_provider(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<UpsertProviderPayload>,
) -> Result<Json<UpsertEntityAck>, HttpError> {
    authorize_admin(&headers, &state)?;
    let channel = ChannelId::parse(payload.channel.as_str());
    let settings = gproxy_provider::parse_provider_settings_json_for_channel(
        &channel,
        payload.settings_json.as_str(),
    )
    .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    let settings_value = serde_json::from_str::<serde_json::Value>(payload.settings_json.as_str())
        .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    let credential_pick_mode =
        parse_credential_pick_mode_from_provider_settings_value(&settings_value);
    let dispatch = serde_json::from_str::<gproxy_provider::ProviderDispatchTable>(
        payload.dispatch_json.as_str(),
    )
    .map_err(|err| HttpError::new(StatusCode::BAD_REQUEST, err.to_string()))?;
    let id = if let Some(id) = payload.id {
        if resolve_provider_channel_by_id(&state, id).await?.is_none() {
            return Err(HttpError::new(
                StatusCode::NOT_FOUND,
                format!("not found: provider {id}"),
            ));
        }
        gproxy_admin::upsert_provider(
            &state.storage_writes,
            gproxy_storage::ProviderWrite {
                id,
                name: payload.name.clone(),
                channel: payload.channel.clone(),
                settings_json: payload.settings_json.clone(),
                dispatch_json: payload.dispatch_json.clone(),
                enabled: payload.enabled,
            },
        )
        .await?;
        id
    } else {
        state
            .load_storage()
            .create_provider(
                payload.name.as_str(),
                payload.channel.as_str(),
                payload.settings_json.as_str(),
                payload.dispatch_json.as_str(),
                payload.enabled,
            )
            .await
            .map_err(|err| HttpError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?
    };
    state.upsert_provider_in_memory(
        channel,
        settings,
        dispatch,
        credential_pick_mode,
        payload.enabled,
    );
    Ok(Json(UpsertEntityAck { ok: true, id }))
}

pub(super) async fn delete_provider(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<DeleteById>,
) -> Result<Json<Ack>, HttpError> {
    authorize_admin(&headers, &state)?;
    if let Some(channel) = resolve_provider_channel_by_id(&state, payload.id).await? {
        state.delete_provider_in_memory(&channel);
    }
    gproxy_admin::delete_provider(&state.storage_writes, payload.id).await?;
    Ok(Json(Ack { ok: true }))
}

#[derive(Debug, Deserialize)]
pub(super) struct UpsertProviderPayload {
    #[serde(default)]
    pub id: Option<i64>,
    pub name: String,
    pub channel: String,
    pub settings_json: String,
    pub dispatch_json: String,
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct UpsertEntityAck {
    pub ok: bool,
    pub id: i64,
}
