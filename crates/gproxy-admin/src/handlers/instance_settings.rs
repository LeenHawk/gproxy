use bytes::Bytes;
use gproxy_store::records::{
    DISABLE_LOG_REDACTION, ENABLE_DOWNSTREAM_LOG, ENABLE_DOWNSTREAM_LOG_BODY,
    ENABLE_TOKENIZER_DOWNLOAD, ENABLE_UPSTREAM_LOG, ENABLE_UPSTREAM_LOG_BODY, ENABLE_USAGE,
    FILE_UPLOAD_MAX_IN_FLIGHT, INHERIT_SYSTEM_PROXY, INSTANCE_NAME, MAX_DATABASE_SIZE_MB, PROXY,
    RETENTION_DAYS, SPOOF_EMULATION, SettingInput, SettingRecord,
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
    if request.instance_name.trim().is_empty() {
        return Err(AdminError::BadRequest(
            "instance name must not be blank".into(),
        ));
    }
    usize::try_from(request.file_upload_max_in_flight).map_err(|_| {
        AdminError::BadRequest("file upload concurrency exceeds this runtime's limit".into())
    })?;
    let body_capture = request.enable_downstream_log_body || request.enable_upstream_log_body;
    if body_capture && request.retention_days.is_none() && request.max_database_size_mb.is_none() {
        return Err(AdminError::BadRequest(
            "body capture requires a retention or database size limit".into(),
        ));
    }
    state
        .store()
        .set_settings(&[
            string(INSTANCE_NAME, Some(request.instance_name.trim())),
            string(PROXY, request.proxy.as_deref().map(str::trim)),
            boolean(SPOOF_EMULATION, request.spoof_emulation),
            boolean(ENABLE_USAGE, request.enable_usage),
            boolean(ENABLE_TOKENIZER_DOWNLOAD, request.enable_tokenizer_download),
            number(FILE_UPLOAD_MAX_IN_FLIGHT, request.file_upload_max_in_flight),
            boolean(INHERIT_SYSTEM_PROXY, request.inherit_system_proxy),
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
        instance_name: text(values, INSTANCE_NAME).unwrap_or_else(|| "default".into()),
        proxy: text(values, PROXY),
        spoof_emulation: enabled(values, SPOOF_EMULATION),
        enable_usage: enabled_or(values, ENABLE_USAGE, true),
        enable_tokenizer_download: enabled(values, ENABLE_TOKENIZER_DOWNLOAD),
        file_upload_max_in_flight: unsigned(values, FILE_UPLOAD_MAX_IN_FLIGHT).unwrap_or(0),
        inherit_system_proxy: enabled(values, INHERIT_SYSTEM_PROXY),
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

fn number(key: &str, value: u64) -> SettingInput {
    SettingInput {
        key: key.into(),
        value: Value::from(value),
    }
}

fn string(key: &str, value: Option<&str>) -> SettingInput {
    SettingInput {
        key: key.into(),
        value: value
            .filter(|value| !value.is_empty())
            .map(Value::from)
            .unwrap_or(Value::Null),
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

fn enabled_or(values: &[SettingRecord], key: &str, default: bool) -> bool {
    values
        .iter()
        .find(|setting| setting.key == key)
        .and_then(|setting| setting.value.as_bool())
        .unwrap_or(default)
}

fn text(values: &[SettingRecord], key: &str) -> Option<String> {
    values
        .iter()
        .find(|setting| setting.key == key)?
        .value
        .as_str()
        .map(str::to_owned)
        .filter(|value| !value.is_empty())
}

fn unsigned(values: &[SettingRecord], key: &str) -> Option<u64> {
    values
        .iter()
        .find(|setting| setting.key == key)?
        .value
        .as_u64()
}

fn positive(values: &[SettingRecord], key: &str) -> Option<u64> {
    values
        .iter()
        .find(|setting| setting.key == key)?
        .value
        .as_u64()
        .filter(|value| *value > 0)
}
