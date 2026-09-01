pub(crate) mod redaction;
mod stream;

use gproxy_store::records::{RequestLogCompletion, RequestLogInput, SettingRecord};

use crate::host::AppHost;

pub(crate) use gproxy_store::records::{
    DISABLE_LOG_REDACTION, ENABLE_DOWNSTREAM_LOG, ENABLE_DOWNSTREAM_LOG_BODY, ENABLE_UPSTREAM_LOG,
    ENABLE_UPSTREAM_LOG_BODY,
};

#[derive(Clone)]
pub(crate) struct DownstreamCapture {
    request_id: String,
    redact: bool,
    body: bool,
}

pub(crate) struct Policy {
    pub upstream: bool,
    pub upstream_body: bool,
    pub redact: bool,
}

impl Policy {
    pub(crate) fn read(settings: &[SettingRecord]) -> Self {
        Self {
            upstream: enabled(settings, ENABLE_UPSTREAM_LOG),
            upstream_body: enabled(settings, ENABLE_UPSTREAM_LOG_BODY),
            redact: !enabled(settings, DISABLE_LOG_REDACTION),
        }
    }
}

pub(crate) async fn begin(
    host: &AppHost,
    request: &gproxy_core::RequestCtx,
) -> Option<DownstreamCapture> {
    let settings = &host.services.control.current().settings;
    if !enabled(settings, ENABLE_DOWNSTREAM_LOG) {
        return None;
    }
    let redact = sensitive_path(&request.path) || !enabled(settings, DISABLE_LOG_REDACTION);
    let body = enabled(settings, ENABLE_DOWNSTREAM_LOG_BODY);
    let input = RequestLogInput {
        request_id: request.request_id.clone(),
        at: unix_now(),
        method: request.method.to_string(),
        path: request.path.clone(),
        query: request
            .query
            .as_deref()
            .map(|query| redaction::query_string(query, redact)),
        request_headers: Some(redaction::headers_json(&request.headers, redact)),
        request_body: body.then(|| redaction::body_bytes(&request.body, redact)),
    };
    if let Err(error) = host.services.store.begin_request_log(&input).await {
        tracing::error!(request_id = %request.request_id, error = %error, "begin request capture failed");
        return None;
    }
    Some(DownstreamCapture {
        request_id: request.request_id.clone(),
        redact,
        body,
    })
}

fn sensitive_path(path: &str) -> bool {
    path.starts_with("/oauth/")
        || path.starts_with("/api/accounts/deviceauth/")
        || path == "/codex/device"
        || matches!(path, "/portal/api/login" | "/portal/api/password")
}

pub(crate) async fn finish(
    host: &AppHost,
    capture: DownstreamCapture,
    result: &mut Result<gproxy_core::ExecOutcome, gproxy_core::CoreError>,
) {
    let (status, headers, body) = match result {
        Ok(outcome) => {
            let body = match &outcome.body {
                gproxy_core::ResponseBody::Full(body) if capture.body => {
                    Some(redaction::body_bytes(body, capture.redact))
                }
                _ => None,
            };
            (outcome.status, outcome.headers.clone(), body)
        }
        Err(error) => {
            let headers = http::HeaderMap::from_iter([(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/json"),
            )]);
            let body = capture.body.then(|| {
                redaction::body_bytes(error.body_json().to_string().as_bytes(), capture.redact)
            });
            (error.status(), headers, body)
        }
    };
    let completion = RequestLogCompletion {
        request_id: capture.request_id.clone(),
        response_status: status.as_u16(),
        error_kind: None,
        response_headers: Some(redaction::headers_json(&headers, capture.redact)),
        response_body: body,
    };
    if let Err(error) = host.services.store.finish_request_log(&completion).await {
        tracing::error!(request_id = %capture.request_id, error = %error, "finish request capture failed");
        return;
    }
    if capture.body
        && let Ok(outcome) = result
    {
        stream::wrap(host.clone(), capture, &mut outcome.body);
    }
}

pub(crate) async fn backfill(host: &AppHost, capture: DownstreamCapture, body: &[u8]) {
    let body = redaction::body_bytes(body, capture.redact);
    if let Err(error) = host
        .services
        .store
        .update_request_log_response(&capture.request_id, body)
        .await
    {
        tracing::error!(request_id = %capture.request_id, error = %error, "backfill request capture failed");
    }
}

fn enabled(settings: &[SettingRecord], key: &str) -> bool {
    settings
        .iter()
        .any(|setting| setting.key == key && setting.value.as_bool() == Some(true))
}

fn unix_now() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system time is after Unix epoch")
        .as_secs() as i64
}
