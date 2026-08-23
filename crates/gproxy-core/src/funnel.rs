use std::time::Instant;

use bytes::Bytes;
use gproxy_channel_api::{Channel, Disposition, NormalizedUsage, StreamDecoder};
use gproxy_protocol::{OperationKey, SettleMode};

use crate::Shared;
use crate::boundary::{ExecOutcome, ResponseBody};
use crate::control::{Pricing, Target};
use crate::funnel_socket::FunnelSocket;
use crate::funnel_stream::FunnelStream;
use crate::host::Host;
use crate::usage::Ended;

#[derive(Debug)]
pub(crate) struct Settled(());

pub(crate) struct FunnelCtx {
    pub request_id: String,
    pub target: Target,
    /// Caller-facing operation key; differs from `key` when a pair transforms.
    pub source_key: Option<OperationKey>,
    /// Channel-native upstream operation key used for usage extraction.
    pub key: Option<OperationKey>,
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

pub(crate) async fn buffered<H: Host>(
    host: &H,
    channel: &dyn Channel,
    ctx: FunnelCtx,
    response: http::Response<Bytes>,
    disposition: Disposition,
) -> ExecOutcome {
    let (parts, body) = response.into_parts();
    let (record_usage, usage) = crate::settlement::usage(channel, &ctx, &parts.headers, &body);
    crate::resource::observe(host, &ctx, parts.status, &body).await;
    let upstream_status = parts.status;
    let upstream_headers = parts.headers;
    let (status, headers, outward, disposition) =
        transform_buffered(&ctx, upstream_status, upstream_headers, &body, disposition);
    crate::settlement::complete(
        host,
        &ctx,
        Some(upstream_status),
        Some(body.clone()),
        record_usage,
        usage,
        Ended::Complete,
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
    body: &Bytes,
    disposition: Disposition,
) -> (http::StatusCode, http::HeaderMap, Bytes, Disposition) {
    let (Some(source), Some(target)) = (ctx.source_key, ctx.key) else {
        return (status, headers, body.clone(), disposition);
    };
    if source == target || !status.is_success() {
        return (status, headers, body.clone(), disposition);
    }
    match gproxy_transform::response(source, target, body.clone()) {
        Ok(body) => {
            headers.remove(http::header::CONTENT_LENGTH);
            (status, headers, body, disposition)
        }
        Err(error) => {
            let error = crate::error::CoreError::Transform(error.to_string());
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
    }
}

pub(crate) fn streaming<H: Host>(
    host: Shared<H>,
    ctx: FunnelCtx,
    response: http::Response<crate::boundary::ByteStream>,
    disposition: Disposition,
    decoder: Option<Box<dyn StreamDecoder>>,
) -> ExecOutcome {
    let (parts, body) = response.into_parts();
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
    crate::settlement::complete(
        host,
        &ctx,
        Some(status),
        Some(body.clone()),
        false,
        None,
        Ended::Complete,
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
    let (record_usage, usage) = crate::settlement::usage(channel, &ctx, &headers, &body);
    crate::settlement::complete(
        host,
        &ctx,
        Some(status),
        Some(body),
        record_usage,
        usage,
        Ended::Interrupted,
    )
    .await;
}

pub(crate) async fn complete_stream<H: Host>(
    host: Shared<H>,
    ctx: FunnelCtx,
    status: http::StatusCode,
    usage: Option<NormalizedUsage>,
    ended: Ended,
) {
    let record_usage = matches!(ctx.settle, SettleMode::OnResponse);
    crate::settlement::complete(
        host.as_ref(),
        &ctx,
        Some(status),
        None,
        record_usage,
        usage,
        ended,
    )
    .await;
}
