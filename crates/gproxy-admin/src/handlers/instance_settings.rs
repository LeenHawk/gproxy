use bytes::Bytes;
use gproxy_store::records::{
    DEFAULT_TOKENIZER_VOCAB, DISABLE_LOG_REDACTION, ENABLE_AUTO_UPDATE_CHECK,
    ENABLE_DOWNSTREAM_LOG, ENABLE_DOWNSTREAM_LOG_BODY, ENABLE_TOKENIZER_DOWNLOAD,
    ENABLE_TOKENIZER_VOCABS, ENABLE_UPSTREAM_LOG, ENABLE_UPSTREAM_LOG_BODY, ENABLE_USAGE,
    INSTANCE_NAME, MAX_DATABASE_SIZE_MB, RETENTION_DAYS, SettingInput, SettingRecord,
    TRAFFIC_BLACKLIST, UPDATE_CHANNEL,
};
use http::{Response, StatusCode};
use serde_json::Value;

use crate::dto::{InstanceSettingsDto, UpdateChannelDto};
use crate::handlers::util;
use crate::{AdminError, State, response};

pub(super) async fn get(state: &impl State) -> Result<Response<Bytes>, AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    let mut settings = read(&snapshot.settings)?;
    settings.runtime_status = Some(state.runtime_settings_status(settings.runtime.clone()));
    response::json(StatusCode::OK, &settings)
}

pub(super) async fn update(
    state: &impl State,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let mut request: InstanceSettingsDto = util::parse(body)?;
    if request.instance_name.trim().is_empty() {
        return Err(AdminError::BadRequest(
            "instance name must not be blank".into(),
        ));
    }
    crate::runtime_settings::normalize(&mut request.runtime).map_err(AdminError::BadRequest)?;
    let body_capture = request.enable_downstream_log_body || request.enable_upstream_log_body;
    if body_capture && request.retention_days.is_none() && request.max_database_size_mb.is_none() {
        return Err(AdminError::BadRequest(
            "body capture requires a retention or database size limit".into(),
        ));
    }
    let traffic_blacklist: gproxy_channel_api::TrafficBlacklistConfig = request
        .traffic_blacklist
        .clone()
        .try_into()
        .map_err(AdminError::BadRequest)?;
    request.traffic_blacklist = traffic_blacklist.clone().into();
    request.traffic_blacklist_defaults =
        gproxy_channel_api::TrafficBlacklistConfig::defaults().into();
    let mut writes = vec![
        string(INSTANCE_NAME, Some(request.instance_name.trim())),
        boolean(ENABLE_USAGE, request.enable_usage),
        boolean(ENABLE_TOKENIZER_VOCABS, request.enable_tokenizer_vocabs),
        boolean(ENABLE_TOKENIZER_DOWNLOAD, request.enable_tokenizer_download),
        string(
            DEFAULT_TOKENIZER_VOCAB,
            request.default_tokenizer_vocab.as_deref().map(str::trim),
        ),
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
        string(
            UPDATE_CHANNEL,
            request.update_channel.map(UpdateChannelDto::as_str),
        ),
        boolean(ENABLE_AUTO_UPDATE_CHECK, request.enable_auto_update_check),
        json(TRAFFIC_BLACKLIST, traffic_blacklist),
    ];
    writes.extend(crate::runtime_settings::writes(&request.runtime));
    state.store().set_settings(&writes).await?;
    state.reload().await?;
    request.runtime_status = Some(state.runtime_settings_status(request.runtime.clone()));
    response::json(StatusCode::OK, &request)
}

fn read(values: &[SettingRecord]) -> Result<InstanceSettingsDto, AdminError> {
    Ok(InstanceSettingsDto {
        instance_name: text(values, INSTANCE_NAME).unwrap_or_else(|| "default".into()),
        runtime: crate::runtime_settings::read(values)?,
        runtime_status: None,
        enable_usage: enabled_or(values, ENABLE_USAGE, true),
        enable_tokenizer_vocabs: enabled_or(values, ENABLE_TOKENIZER_VOCABS, true),
        enable_tokenizer_download: enabled(values, ENABLE_TOKENIZER_DOWNLOAD),
        default_tokenizer_vocab: text(values, DEFAULT_TOKENIZER_VOCAB),
        retention_days: positive(values, RETENTION_DAYS),
        max_database_size_mb: positive(values, MAX_DATABASE_SIZE_MB),
        enable_downstream_log: enabled(values, ENABLE_DOWNSTREAM_LOG),
        enable_downstream_log_body: enabled(values, ENABLE_DOWNSTREAM_LOG_BODY),
        enable_upstream_log: enabled(values, ENABLE_UPSTREAM_LOG),
        enable_upstream_log_body: enabled(values, ENABLE_UPSTREAM_LOG_BODY),
        disable_log_redaction: enabled(values, DISABLE_LOG_REDACTION),
        update_channel: text(values, UPDATE_CHANNEL)
            .as_deref()
            .and_then(UpdateChannelDto::from_stored),
        enable_auto_update_check: enabled(values, ENABLE_AUTO_UPDATE_CHECK),
        traffic_blacklist: traffic_blacklist(values).into(),
        traffic_blacklist_defaults: gproxy_channel_api::TrafficBlacklistConfig::defaults().into(),
    })
}

fn boolean(key: &str, value: bool) -> SettingInput {
    SettingInput {
        key: key.into(),
        value: Value::Bool(value),
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

fn json(key: &str, value: impl serde::Serialize) -> SettingInput {
    SettingInput {
        key: key.into(),
        value: serde_json::to_value(value).expect("validated settings serialize"),
    }
}

fn traffic_blacklist(values: &[SettingRecord]) -> gproxy_channel_api::TrafficBlacklistConfig {
    values
        .iter()
        .find(|setting| setting.key == TRAFFIC_BLACKLIST)
        .and_then(|setting| {
            gproxy_channel_api::TrafficBlacklistConfig::from_value(&setting.value).ok()
        })
        .unwrap_or_default()
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

fn positive(values: &[SettingRecord], key: &str) -> Option<u64> {
    values
        .iter()
        .find(|setting| setting.key == key)?
        .value
        .as_u64()
        .filter(|value| *value > 0)
}
