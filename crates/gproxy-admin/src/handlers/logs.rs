use bytes::Bytes;
use gproxy_store::records::{
    DISABLE_LOG_REDACTION, ENABLE_DOWNSTREAM_LOG, ENABLE_DOWNSTREAM_LOG_BODY, ENABLE_UPSTREAM_LOG,
    ENABLE_UPSTREAM_LOG_BODY, LogQuery, MAX_DATABASE_SIZE_MB, RETENTION_DAYS, SettingInput,
    SettingRecord,
};
use http::request::Parts;
use http::{Response, StatusCode};
use serde_json::Value;

use crate::dto::{
    DownstreamLogDto, LogDetailDto, LogListItemDto, LogPageDto, LogQueryDto, LogSettingsDto,
    LogSettingsUpdateDto, WireLogDto,
};
use crate::handlers::{observability, util};
use crate::{AdminError, State, response};

pub(super) async fn list(state: &impl State, parts: &Parts) -> Result<Response<Bytes>, AdminError> {
    let request = serde_urlencoded::from_str::<LogQueryDto>(parts.uri.query().unwrap_or_default())
        .map_err(|error| AdminError::BadRequest(error.to_string()))?;
    let (start, end) = observability::range(request.start, request.end)?;
    let page = state
        .store()
        .list_logs(&LogQuery {
            start,
            end,
            user_id: request.user_id,
            user_key_id: request.user_key_id,
            provider_id: request.provider_id,
            status: request.status,
            request_id: request.request_id,
            cursor: request.cursor,
            limit: request.limit.unwrap_or(50).clamp(1, 100),
        })
        .await?;
    response::json(
        StatusCode::OK,
        &LogPageDto {
            items: page
                .items
                .into_iter()
                .map(|item| LogListItemDto {
                    id: item.id,
                    request_id: item.request_id,
                    at: item.at,
                    method: item.method,
                    path: item.path,
                    response_status: item.response_status,
                    error_kind: item.error_kind,
                })
                .collect(),
            next_cursor: page.next_cursor,
        },
    )
}

pub(super) async fn detail(
    state: &impl State,
    request_id: &str,
) -> Result<Response<Bytes>, AdminError> {
    let value = state
        .store()
        .log_detail(request_id)
        .await?
        .ok_or(AdminError::NotFound)?;
    let downstream = value.downstream;
    response::json(
        StatusCode::OK,
        &LogDetailDto {
            downstream: DownstreamLogDto {
                id: downstream.id,
                request_id: downstream.input.request_id,
                at: downstream.input.at,
                method: downstream.input.method,
                path: downstream.input.path,
                query: downstream.input.query,
                request_headers: downstream.input.request_headers,
                request_body: text(downstream.input.request_body),
                response_status: downstream.response_status,
                error_kind: downstream.error_kind,
                response_headers: downstream.response_headers,
                response_body: text(downstream.response_body),
            },
            upstream: value
                .upstream
                .into_iter()
                .map(|wire| WireLogDto {
                    id: wire.id,
                    at: wire.input.at,
                    provider_id: wire.input.provider_id,
                    credential_id: wire.input.credential_id,
                    upstream_url: wire.input.upstream_url,
                    request_method: wire.input.request_method,
                    request_headers: wire.input.request_headers,
                    request_body: text(wire.input.request_body),
                    response_status: wire.input.response_status,
                    response_headers: wire.input.response_headers,
                    response_body: text(wire.input.response_body),
                })
                .collect(),
        },
    )
}

pub(super) async fn get_settings(state: &impl State) -> Result<Response<Bytes>, AdminError> {
    let snapshot = state.store().control_snapshot().await?;
    response::json(StatusCode::OK, &settings(&snapshot.settings))
}

pub(super) async fn update_settings(
    state: &impl State,
    body: &Bytes,
) -> Result<Response<Bytes>, AdminError> {
    let request: LogSettingsUpdateDto = util::parse(body)?;
    let snapshot = state.store().control_snapshot().await?;
    let current = settings(&snapshot.settings);
    if (request.enable_downstream_log_body || request.enable_upstream_log_body)
        && !current.body_capture_allowed
    {
        return Err(AdminError::BadRequest(
            "body capture requires positive retention_days or max_database_size_mb".into(),
        ));
    }
    state
        .store()
        .set_settings(&[
            setting(ENABLE_DOWNSTREAM_LOG, request.enable_downstream_log),
            setting(
                ENABLE_DOWNSTREAM_LOG_BODY,
                request.enable_downstream_log_body,
            ),
            setting(ENABLE_UPSTREAM_LOG, request.enable_upstream_log),
            setting(ENABLE_UPSTREAM_LOG_BODY, request.enable_upstream_log_body),
            setting(DISABLE_LOG_REDACTION, request.disable_log_redaction),
        ])
        .await?;
    state.reload().await?;
    response::json(
        StatusCode::OK,
        &LogSettingsDto {
            enable_downstream_log: request.enable_downstream_log,
            enable_downstream_log_body: request.enable_downstream_log_body,
            enable_upstream_log: request.enable_upstream_log,
            enable_upstream_log_body: request.enable_upstream_log_body,
            disable_log_redaction: request.disable_log_redaction,
            ..current
        },
    )
}

fn settings(values: &[SettingRecord]) -> LogSettingsDto {
    let retention_days = positive(values, RETENTION_DAYS);
    let max_database_size_mb = positive(values, MAX_DATABASE_SIZE_MB);
    LogSettingsDto {
        enable_downstream_log: enabled(values, ENABLE_DOWNSTREAM_LOG),
        enable_downstream_log_body: enabled(values, ENABLE_DOWNSTREAM_LOG_BODY),
        enable_upstream_log: enabled(values, ENABLE_UPSTREAM_LOG),
        enable_upstream_log_body: enabled(values, ENABLE_UPSTREAM_LOG_BODY),
        disable_log_redaction: enabled(values, DISABLE_LOG_REDACTION),
        retention_days,
        max_database_size_mb,
        body_capture_allowed: retention_days.is_some() || max_database_size_mb.is_some(),
    }
}

fn setting(key: &str, value: bool) -> SettingInput {
    SettingInput {
        key: key.into(),
        value: Value::Bool(value),
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
        .as_i64()
        .filter(|value| *value > 0)
        .map(|value| value as u64)
}

fn text(value: Option<Vec<u8>>) -> Option<String> {
    value.map(|body| String::from_utf8_lossy(&body).into_owned())
}
