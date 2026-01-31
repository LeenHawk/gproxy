use std::future::Future;
use std::io;
use std::time::{Duration, Instant, SystemTime};

use futures_util::StreamExt;
use http::header::RETRY_AFTER;
use http::{HeaderMap, StatusCode};
use tracing::{info, warn};

use gproxy_provider_core::{
    record_upstream, AttemptFailure, DisallowLevel, DisallowMark, DisallowScope, ProxyResponse,
    StreamBody, UpstreamContext, UpstreamPassthroughError, UpstreamRecordMeta,
};

pub fn network_failure(err: wreq::Error, scope: &DisallowScope) -> AttemptFailure {
    AttemptFailure {
        passthrough: UpstreamPassthroughError::service_unavailable(err.to_string()),
        mark: Some(DisallowMark {
            scope: scope.clone(),
            level: DisallowLevel::Transient,
            duration: Some(Duration::from_secs(30)),
            reason: Some("network_error".to_string()),
        }),
    }
}

fn log_upstream_request(
    ctx: &UpstreamContext,
    provider: &str,
    op: &str,
    method: &str,
    path: &str,
    model: Option<&str>,
    is_stream: bool,
) -> Instant {
    match model {
        Some(model) => {
            info!(
                event = "upstream_request",
                trace_id = %ctx.trace_id,
                provider = %provider,
                op = %op,
                method = %method,
                path = %path,
                model = %model,
                is_stream = is_stream
            );
        }
        None => {
            info!(
                event = "upstream_request",
                trace_id = %ctx.trace_id,
                provider = %provider,
                op = %op,
                method = %method,
                path = %path,
                is_stream = is_stream
            );
        }
    }
    Instant::now()
}

fn log_upstream_response_ok(
    ctx: &UpstreamContext,
    provider: &str,
    op: &str,
    status: StatusCode,
    elapsed_ms: u128,
    is_stream: bool,
) {
    info!(
        event = "upstream_response",
        trace_id = %ctx.trace_id,
        provider = %provider,
        op = %op,
        status = %status.as_u16(),
        elapsed_ms = elapsed_ms,
        is_stream = is_stream
    );
}

fn log_upstream_response_err(
    ctx: &UpstreamContext,
    provider: &str,
    op: &str,
    elapsed_ms: u128,
    err: impl std::fmt::Display,
) {
    warn!(
        event = "upstream_response",
        trace_id = %ctx.trace_id,
        provider = %provider,
        op = %op,
        status = "error",
        elapsed_ms = elapsed_ms,
        error = %err
    );
}

#[allow(clippy::too_many_arguments)]
pub async fn send_with_logging<F, Fut>(
    ctx: &UpstreamContext,
    provider: &str,
    op: &str,
    method: &str,
    path: &str,
    model: Option<&str>,
    is_stream: bool,
    scope: &DisallowScope,
    send: F,
) -> Result<wreq::Response, AttemptFailure>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<wreq::Response, wreq::Error>>,
{
    let started_at =
        log_upstream_request(ctx, provider, op, method, path, model, is_stream);
    let response = send().await.map_err(|err| {
        log_upstream_response_err(
            ctx,
            provider,
            op,
            started_at.elapsed().as_millis(),
            &err,
        );
        network_failure(err, scope)
    })?;
    log_upstream_response_ok(
        ctx,
        provider,
        op,
        response.status(),
        started_at.elapsed().as_millis(),
        is_stream,
    );
    Ok(response)
}

pub async fn handle_response(
    response: wreq::Response,
    is_stream: bool,
    scope: DisallowScope,
    ctx: &UpstreamContext,
    record: Option<UpstreamRecordMeta>,
) -> Result<ProxyResponse, AttemptFailure> {
    let status = response.status();
    let headers = response.headers().clone();

    if !status.is_success() {
        let body = response
            .bytes()
            .await
            .map_err(|err| network_failure(err, &scope))?;
        if let Some(record) = record {
            record_upstream(
                &ctx.traffic,
                Some(ctx.trace_id.clone()),
                record,
                status,
                &headers,
                Some(&body),
                false,
            );
        }
        let mark = classify_status(status, &headers, &scope);
        return Err(AttemptFailure {
            passthrough: UpstreamPassthroughError::new(status, headers, body),
            mark,
        });
    }

    if is_stream {
        let stream = response.bytes_stream().map(|item| {
            item.map_err(|err| io::Error::other(err.to_string()))
        });
        Ok(ProxyResponse::Stream {
            status,
            headers,
            body: StreamBody::new("text/event-stream", stream),
        })
    } else {
        let body = response
            .bytes()
            .await
            .map_err(|err| network_failure(err, &scope))?;
        Ok(ProxyResponse::Json { status, headers, body })
    }
}

pub fn classify_status(
    status: StatusCode,
    headers: &HeaderMap,
    scope: &DisallowScope,
) -> Option<DisallowMark> {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Some(DisallowMark {
            scope: scope.clone(),
            level: DisallowLevel::Dead,
            duration: None,
            reason: Some("auth_error".to_string()),
        }),
        StatusCode::TOO_MANY_REQUESTS => {
            let retry_after = retry_after_seconds(headers).unwrap_or(60);
            Some(DisallowMark {
                scope: scope.clone(),
                level: DisallowLevel::Cooldown,
                duration: Some(Duration::from_secs(retry_after)),
                reason: Some("rate_limit".to_string()),
            })
        }
        StatusCode::BAD_GATEWAY
        | StatusCode::SERVICE_UNAVAILABLE
        | StatusCode::GATEWAY_TIMEOUT => Some(DisallowMark {
            scope: scope.clone(),
            level: DisallowLevel::Transient,
            duration: Some(Duration::from_secs(30)),
            reason: Some("upstream_unavailable".to_string()),
        }),
        _ => None,
    }
}

fn retry_after_seconds(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            let value = value.trim();
            if let Ok(seconds) = value.parse::<u64>() {
                return Some(seconds);
            }
            if let Ok(when) = httpdate::parse_http_date(value) {
                return when
                    .duration_since(SystemTime::now())
                    .ok()
                    .map(|duration| duration.as_secs());
            }
            None
        })
}
