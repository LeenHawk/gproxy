use std::time::Instant;

use bytes::Bytes;
use gproxy_channel_api::{Channel, Disposition, NormalizedUsage, StreamDecoder};
use gproxy_protocol::{OperationKey, SettleMode};

use crate::Shared;
use crate::boundary::{ExecOutcome, ResponseBody};
use crate::control::{Pricing, Target};
use crate::funnel_stream::FunnelStream;
use crate::host::{Capture, CaptureSink, Host};
use crate::usage::Ended;

#[derive(Debug)]
pub(crate) struct Settled(());

pub(crate) struct FunnelCtx {
    pub request_id: String,
    pub target: Target,
    pub key: OperationKey,
    pub settle: SettleMode,
    pub pricing: Option<Pricing>,
    pub started: Instant,
    pub upstream_url: String,
    pub request_body: Bytes,
    pub dedupe_key: Option<String>,
}

pub(crate) async fn buffered<H: Host>(
    host: &H,
    channel: &dyn Channel,
    ctx: FunnelCtx,
    response: http::Response<Bytes>,
    disposition: Disposition,
) -> ExecOutcome {
    let (parts, body) = response.into_parts();
    let (record_usage, usage) = crate::settlement::usage(channel, &ctx, &body);
    crate::settlement::complete(
        host,
        &ctx,
        Some(parts.status),
        Some(body.clone()),
        record_usage,
        usage,
        Ended::Complete,
    )
    .await;
    ExecOutcome {
        status: parts.status,
        headers: parts.headers,
        body: ResponseBody::Full(body),
        disposition,
        _settled: Settled(()),
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

pub(crate) async fn interrupted<H: Host>(
    host: &H,
    channel: &dyn Channel,
    ctx: FunnelCtx,
    status: http::StatusCode,
    body: Bytes,
) {
    let (record_usage, usage) = crate::settlement::usage(channel, &ctx, &body);
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

pub(crate) async fn transport_failed<H: Host>(
    host: &H,
    ctx: &FunnelCtx,
    error: &gproxy_channel_api::TransportError,
) {
    host.capture()
        .record(&Capture {
            request_id: ctx.request_id.clone(),
            upstream_url: ctx.upstream_url.clone(),
            request_body: ctx.request_body.clone(),
            response_status: None,
            response_body: None,
        })
        .await;
    tracing::info!(
        request_id = %ctx.request_id,
        provider_id = ctx.target.provider.id,
        credential_id = ctx.target.credential.0,
        operation = ?ctx.key.operation,
        error_kind = transport_error_kind(error),
        "request.completed"
    );
}

fn transport_error_kind(error: &gproxy_channel_api::TransportError) -> &'static str {
    match error {
        gproxy_channel_api::TransportError::Connect(_) => "connect",
        gproxy_channel_api::TransportError::Timeout => "timeout",
        gproxy_channel_api::TransportError::Interrupted(_) => "interrupted",
    }
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
