use std::time::Instant;

use bytes::Bytes;
use gproxy_channel_api::{Channel, Disposition, NormalizedUsage, ResponseShapeCtx, StreamDecoder};
use gproxy_protocol::{OperationKey, SettleMode, StreamFraming};

use crate::Shared;
use crate::boundary::{ExecOutcome, ResponseBody};
use crate::control::{Pricing, Target};
use crate::host::Host;
use crate::usage::Ended;

pub(crate) mod error;
mod settlement;
mod socket;
mod stream;

use self::socket::FunnelSocket;
use self::stream::FunnelStream;

#[derive(Debug)]
pub(crate) struct Settled(());

pub(crate) struct FunnelCtx {
    pub request_id: String,
    pub target: Target,
    /// Caller-facing operation key; differs from `key` when a pair transforms.
    pub source_key: Option<OperationKey>,
    /// Channel-native upstream operation key used for usage extraction.
    pub key: Option<OperationKey>,
    pub source_framing: StreamFraming,
    pub target_framing: StreamFraming,
    pub settle: SettleMode,
    pub pricing: Option<Pricing>,
    pub started: Instant,
    pub upstream_url: Option<String>,
    pub request_body: Bytes,
    pub dedupe_key: Option<String>,
    pub owner_user_id: Option<i64>,
    pub resource: Option<(&'static str, String)>,
    pub admitted: bool,
    pub surface_label: Option<&'static str>,
}

pub(crate) struct BufferedRelay {
    pub response: http::Response<Bytes>,
    pub usage: Option<NormalizedUsage>,
    pub capture_body: Option<Bytes>,
    pub outward_ready: bool,
}

impl BufferedRelay {
    pub(crate) fn native(response: http::Response<Bytes>) -> Self {
        Self {
            response,
            usage: None,
            capture_body: None,
            outward_ready: false,
        }
    }
}

pub(crate) async fn buffered<H: Host>(
    host: &H,
    channel: &dyn Channel,
    ctx: FunnelCtx,
    relay: BufferedRelay,
    disposition: Disposition,
) -> ExecOutcome {
    let BufferedRelay {
        response,
        usage: usage_override,
        capture_body,
        outward_ready,
    } = relay;
    let (parts, body) = response.into_parts();
    let (record_usage, extracted) = if usage_override.is_some() {
        (matches!(ctx.settle, SettleMode::OnResponse), None)
    } else {
        settlement::usage(channel, &ctx, &parts.headers, &body)
    };
    let usage = usage_override.or(extracted);
    crate::execution::resource::observe(
        host,
        &ctx,
        parts.status,
        &parts.headers,
        capture_body.as_deref().unwrap_or(&body),
    )
    .await;
    let upstream_status = parts.status;
    let upstream_headers = parts.headers;
    let (status, headers, outward, disposition) = if outward_ready {
        (upstream_status, upstream_headers, body.clone(), disposition)
    } else {
        let shaped = ctx.key.map_or_else(
            || Ok(body.clone()),
            |key| {
                channel.shape_response(ResponseShapeCtx {
                    key,
                    status: upstream_status,
                    headers: &upstream_headers,
                    body: &body,
                })
            },
        );
        transform_buffered(&ctx, upstream_status, upstream_headers, shaped, disposition)
    };
    settlement::complete(
        host,
        &ctx,
        settlement::Completion {
            status: Some(upstream_status),
            response_body: Some(capture_body.unwrap_or_else(|| body.clone())),
            estimated_output_chars: None,
            record_usage,
            usage,
            ended: Ended::Complete,
        },
    )
    .await;
    ExecOutcome {
        status,
        headers,
        body: ResponseBody::Full(outward),
        disposition,
        _settled: Settled(()),
    }
}

fn transform_buffered(
    ctx: &FunnelCtx,
    status: http::StatusCode,
    mut headers: http::HeaderMap,
    shaped: Result<Bytes, gproxy_channel_api::ChannelError>,
    disposition: Disposition,
) -> (http::StatusCode, http::HeaderMap, Bytes, Disposition) {
    let body = match shaped {
        Ok(body) => body,
        Err(error) => return transform_error(crate::error::CoreError::Channel(error)),
    };
    let (Some(source), Some(target)) = (ctx.source_key, ctx.key) else {
        return (status, headers, body, disposition);
    };
    if source == target || !status.is_success() {
        return (status, headers, body, disposition);
    }
    match gproxy_transform::response(source, target, body) {
        Ok(body) => {
            headers.remove(http::header::CONTENT_LENGTH);
            (status, headers, body, disposition)
        }
        Err(error) => transform_error(crate::error::CoreError::Transform(error.to_string())),
    }
}

fn transform_error(
    error: crate::error::CoreError,
) -> (http::StatusCode, http::HeaderMap, Bytes, Disposition) {
    let headers = http::HeaderMap::from_iter([(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    )]);
    (
        error.status(),
        headers,
        Bytes::from(error.body_json().to_string()),
        Disposition::Terminal,
    )
}

pub(crate) fn streaming<H: Host>(
    host: Shared<H>,
    ctx: FunnelCtx,
    response: http::Response<crate::boundary::ByteStream>,
    disposition: Disposition,
    decoder: Option<Box<dyn StreamDecoder>>,
) -> ExecOutcome {
    let (mut parts, body) = response.into_parts();
    if ctx.source_key != ctx.key {
        frame_headers(&mut parts.headers, ctx.source_framing);
    }
    let body = FunnelStream::new(body, decoder, host, ctx, parts.status);
    ExecOutcome {
        status: parts.status,
        headers: parts.headers,
        body: ResponseBody::Stream(Box::pin(body)),
        disposition,
        _settled: Settled(()),
    }
}

pub(crate) async fn free_buffered<H: Host>(
    host: &H,
    ctx: FunnelCtx,
    status: http::StatusCode,
    headers: http::HeaderMap,
    body: Bytes,
    disposition: Disposition,
) -> ExecOutcome {
    settlement::complete(
        host,
        &ctx,
        settlement::Completion {
            status: Some(status),
            response_body: Some(body.clone()),
            estimated_output_chars: None,
            record_usage: false,
            usage: None,
            ended: Ended::Complete,
        },
    )
    .await;
    ExecOutcome {
        status,
        headers,
        body: ResponseBody::Full(body),
        disposition,
        _settled: Settled(()),
    }
}

pub(crate) fn free_streaming<H: Host>(
    host: Shared<H>,
    ctx: FunnelCtx,
    status: http::StatusCode,
    headers: http::HeaderMap,
    body: crate::boundary::ByteStream,
    disposition: Disposition,
) -> ExecOutcome {
    let body = FunnelStream::new(body, None, host, ctx, status);
    ExecOutcome {
        status,
        headers,
        body: ResponseBody::Stream(Box::pin(body)),
        disposition,
        _settled: Settled(()),
    }
}

fn frame_headers(headers: &mut http::HeaderMap, framing: StreamFraming) {
    let content_type = match framing {
        StreamFraming::Sse => Some("text/event-stream"),
        StreamFraming::JsonArray => Some("application/json"),
        StreamFraming::WebSocket => None,
    };
    if let Some(content_type) = content_type {
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static(content_type),
        );
        headers.remove(http::header::CONTENT_LENGTH);
        if framing == StreamFraming::Sse {
            headers.insert(
                http::header::CACHE_CONTROL,
                http::HeaderValue::from_static("no-cache"),
            );
        }
    }
}

pub(crate) fn websocket<H: Host>(
    host: Shared<H>,
    ctx: FunnelCtx,
    socket: Box<dyn gproxy_channel_api::WsDuplex>,
) -> ExecOutcome {
    ExecOutcome {
        status: http::StatusCode::SWITCHING_PROTOCOLS,
        headers: http::HeaderMap::new(),
        body: ResponseBody::WebSocket(Box::new(FunnelSocket::new(host, ctx, socket))),
        disposition: Disposition::Success,
        _settled: Settled(()),
    }
}

pub(crate) async fn interrupted<H: Host>(
    host: &H,
    channel: &dyn Channel,
    ctx: FunnelCtx,
    status: http::StatusCode,
    headers: http::HeaderMap,
    body: Bytes,
) {
    let (record_usage, usage) = settlement::usage(channel, &ctx, &headers, &body);
    settlement::complete(
        host,
        &ctx,
        settlement::Completion {
            status: Some(status),
            response_body: Some(body),
            estimated_output_chars: None,
            record_usage,
            usage,
            ended: Ended::Interrupted,
        },
    )
    .await;
}

pub(crate) async fn complete_stream<H: Host>(
    host: Shared<H>,
    ctx: FunnelCtx,
    status: http::StatusCode,
    usage: Option<NormalizedUsage>,
    estimated_output_chars: Option<u64>,
    ended: Ended,
) {
    let record_usage = matches!(ctx.settle, SettleMode::OnResponse);
    settlement::complete(
        host.as_ref(),
        &ctx,
        settlement::Completion {
            status: Some(status),
            response_body: None,
            estimated_output_chars,
            record_usage,
            usage,
            ended,
        },
    )
    .await;
}
