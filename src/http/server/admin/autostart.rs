//! Native host automatic-start settings.

use axum::Json;
use axum::extract::State;
use serde::Deserialize;

use crate::api::error::ApiError;
use crate::app::AppState;
use crate::autostart::AutoStartStatus;

#[derive(Debug, Deserialize)]
pub struct SetAutoStart {
    enabled: bool,
}

pub async fn status(State(state): State<AppState>) -> Json<AutoStartStatus> {
    Json(state.autostart.status())
}

pub async fn set(
    State(state): State<AppState>,
    Json(input): Json<SetAutoStart>,
) -> Result<Json<AutoStartStatus>, ApiError> {
    let current = state.autostart.status();
    // A sensitive-config process may still remove an installer-created entry;
    // all other unsupported transitions are a user-visible capability error.
    if !current.supported && !current.enabled {
        return Err(ApiError::BadRequest(
            current
                .detail
                .unwrap_or_else(|| "automatic startup is unavailable".into()),
        ));
    }
    if !current.supported && input.enabled {
        return Err(ApiError::BadRequest(
            current
                .detail
                .unwrap_or_else(|| "automatic startup cannot be enabled".into()),
        ));
    }
    state
        .autostart
        .set_enabled(input.enabled)
        .map(Json)
        .map_err(|error| ApiError::Internal(error.to_string()))
}
