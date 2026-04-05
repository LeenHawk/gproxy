use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::HeaderMap;
use serde::Serialize;

use gproxy_server::AppState;
use gproxy_storage::GlobalSettingsWrite;
use gproxy_storage::repository::SettingsRepository;

use crate::auth::authorize_admin;
use crate::error::{AckResponse, HttpError};

#[derive(Serialize)]
pub struct GlobalSettingsResponse {
    pub host: String,
    pub port: u16,
    pub proxy: Option<String>,
    pub spoof_emulation: String,
    pub update_source: String,
    pub enable_usage: bool,
    pub enable_upstream_log: bool,
    pub enable_upstream_log_body: bool,
    pub enable_downstream_log: bool,
    pub enable_downstream_log_body: bool,
    pub dsn: String,
    pub data_dir: String,
}

pub async fn get_global_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<GlobalSettingsResponse>, HttpError> {
    authorize_admin(&headers, &state)?;
    let config = state.config();
    Ok(Json(GlobalSettingsResponse {
        host: config.host.clone(),
        port: config.port,
        proxy: config.proxy.clone(),
        spoof_emulation: config.spoof_emulation.clone(),
        update_source: config.update_source.clone(),
        enable_usage: config.enable_usage,
        enable_upstream_log: config.enable_upstream_log,
        enable_upstream_log_body: config.enable_upstream_log_body,
        enable_downstream_log: config.enable_downstream_log,
        enable_downstream_log_body: config.enable_downstream_log_body,
        dsn: config.dsn.clone(),
        data_dir: config.data_dir.clone(),
    }))
}

pub async fn upsert_global_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<GlobalSettingsWrite>,
) -> Result<Json<AckResponse>, HttpError> {
    authorize_admin(&headers, &state)?;

    let current = state.config();
    let dsn_changed = payload.dsn != current.dsn;

    if dsn_changed {
        // Intentional design: changing DSN is a cutover to a different database,
        // not an in-place migration of data from the previous one.
        if !payload.data_dir.is_empty() {
            std::fs::create_dir_all(&payload.data_dir)
                .map_err(|e| HttpError::internal(format!("create data dir: {e}")))?;
        }

        let previous_storage = state.storage();
        let previous_config = state.config();
        let previous_engine = state.engine();

        let new_storage = previous_storage.reconnect(&payload.dsn).await?;
        new_storage.sync().await?;
        new_storage
            .upsert_global_settings(payload)
            .await?;

        state.replace_storage(new_storage);
        if let Err(err) = crate::bootstrap::reload_from_db(&state).await {
            state.replace_storage((*previous_storage).clone());
            state.replace_config((*previous_config).clone());
            state.replace_engine_arc(previous_engine);
            return Err(HttpError::internal(format!(
                "reload after DSN change failed: {err}"
            )));
        }
    } else {
        state
            .storage()
            .upsert_global_settings(payload.clone())
            .await?;

        state.replace_config(gproxy_server::GlobalConfig {
            host: payload.host.clone(),
            port: payload.port,
            admin_key: payload.admin_key.clone(),
            proxy: payload.proxy.clone(),
            spoof_emulation: payload.spoof_emulation.clone(),
            update_source: payload.update_source.clone(),
            enable_usage: payload.enable_usage,
            enable_upstream_log: payload.enable_upstream_log,
            enable_upstream_log_body: payload.enable_upstream_log_body,
            enable_downstream_log: payload.enable_downstream_log,
            enable_downstream_log_body: payload.enable_downstream_log_body,
            dsn: payload.dsn.clone(),
            data_dir: payload.data_dir.clone(),
        });

        let new_engine = state.engine().with_settings(
            payload.proxy.as_deref(),
            Some(payload.spoof_emulation.as_str()),
            payload.enable_usage,
            payload.enable_upstream_log,
            payload.enable_upstream_log_body,
        );
        state.replace_engine(new_engine);
    }

    Ok(Json(AckResponse { ok: true, id: None }))
}
