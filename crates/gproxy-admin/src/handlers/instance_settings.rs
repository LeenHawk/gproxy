use bytes::Bytes;
use gproxy_store::records::{
    DISABLE_LOG_REDACTION, ENABLE_DOWNSTREAM_LOG, ENABLE_DOWNSTREAM_LOG_BODY, ENABLE_UPSTREAM_LOG,
    ENABLE_UPSTREAM_LOG_BODY, MAX_DATABASE_SIZE_MB, RETENTION_DAYS, SettingInput, SettingRecord,
};
use http::{Response, StatusCode};
use serde_json::Value;

use crate::dto::InstanceSettingsDto;
use crate::handlers::util;
use crate::{AdminError, State, response};

pub(super) async fn get(state: &impl State) -> Result<Response<Bytes>, AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    response::json(StatusCode::OK, &read(&snapshot.settings))
}

pub(super) async fn update(
    state: &impl State,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let request: InstanceSettingsDto = util::parse(body)?;
    let body_capture = request.enable_downstream_log_body || request.enable_upstream_log_body;
    if body_capture && request.retention_days.is_none() && request.max_database_size_mb.is_none() {
        return Err(AdminError::BadRequest(
            "body capture requires a retention or database size limit".into(),
        ));
    }
    state
        .store()
        .set_settings(&[
            optional(RETENTION_DAYS, request.retention_days),
            optional(MAX_DATABASE_SIZE_MB, request.max_database_size_mb),
            boolean(ENABLE_DOWNSTREAM_LOG, request.enable_downstream_log),
            boolean(
                ENABLE_DOWNSTREAM_LOG_BODY,
                request.enable_downstream_log_body,
            ),
            boolean(ENABLE_UPSTREAM_LOG, request.enable_upstream_log),
            boolean(ENABLE_UPSTREAM_LOG_BODY, request.enable_upstream_log_body),
            boolean(DISABLE_LOG_REDACTION, request.disable_log_redaction),
        ])
        .await?;
    state.reload().await?;
    response::json(StatusCode::OK, &request)
}

fn read(values: &[SettingRecord]) -> InstanceSettingsDto {
    InstanceSettingsDto {
        retention_days: positive(values, RETENTION_DAYS),
        max_database_size_mb: positive(values, MAX_DATABASE_SIZE_MB),
        enable_downstream_log: enabled(values, ENABLE_DOWNSTREAM_LOG),
        enable_downstream_log_body: enabled(values, ENABLE_DOWNSTREAM_LOG_BODY),
        enable_upstream_log: enabled(values, ENABLE_UPSTREAM_LOG),
        enable_upstream_log_body: enabled(values, ENABLE_UPSTREAM_LOG_BODY),
        disable_log_redaction: enabled(values, DISABLE_LOG_REDACTION),
    }
}

fn boolean(key: &str, value: bool) -> SettingInput {
    SettingInput {
        key: key.into(),
        value: Value::Bool(value),
    }
}

fn optional(key: &str, value: Option<u64>) -> SettingInput {
    SettingInput {
        key: key.into(),
        value: value.map(Value::from).unwrap_or(Value::Null),
    }
}

fn enabled(values: &[SettingRecord], key: &str) -> bool {
    values
        .iter()
        .any(|setting| setting.key == key && setting.value.as_bool() == Some(true))
}

fn positive(values: &[SettingRecord], key: &str) -> Option<u64> {
    values
        .iter()
        .find(|setting| setting.key == key)?
        .value
        .as_u64()
        .filter(|value| *value > 0)
}
