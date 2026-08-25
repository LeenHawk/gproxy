use bytes::Bytes;
use gproxy_store::records::SettingInput;
use http::{Response, StatusCode};

use crate::dto::PortalSettingsDto;
use crate::handlers::util;
use crate::portal::{RECENT_REQUESTS_SETTING, recent_requests_enabled};
use crate::{AdminError, State, response};

pub(super) async fn get(state: &impl State) -> Result<Response<Bytes>, AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    response::json(
        StatusCode::OK,
        &PortalSettingsDto {
            recent_requests_enabled: recent_requests_enabled(&snapshot.settings),
        },
    )
}

pub(super) async fn update(
    state: &impl State,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let request: PortalSettingsDto = util::parse(body)?;
    state
        .store()
        .set_setting(&SettingInput {
            key: RECENT_REQUESTS_SETTING.into(),
            value: serde_json::Value::Bool(request.recent_requests_enabled),
        })
        .await?;
    state.reload().await?;
    response::json(StatusCode::OK, &request)
}
