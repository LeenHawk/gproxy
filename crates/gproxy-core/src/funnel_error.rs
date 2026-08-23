use bytes::Bytes;
use gproxy_channel_api::{Disposition, TransportError};
use gproxy_protocol::OperationKey;

use crate::boundary::RequestCtx;
use crate::control::Target;
use crate::error::CoreError;
use crate::funnel::FunnelCtx;
use crate::host::{Capture, CaptureSink, Host};

pub(crate) async fn terminal_transport<H: Host>(host: &H, ctx: &FunnelCtx, error: &TransportError) {
    capture_transport(host, ctx).await;
    tracing::info!(
        request_id = %ctx.request_id,
        provider_id = ctx.target.provider.id,
        credential_id = ctx.target.credential.0,
        operation = ?ctx.key.map(|key| key.operation),
        surface = ctx.surface_label.unwrap_or(""),
        error_kind = transport_error_kind(error),
        "request.completed"
    );
}

pub(crate) async fn attempt_transport<H: Host>(host: &H, ctx: &FunnelCtx, error: &TransportError) {
    capture_transport(host, ctx).await;
    tracing::info!(
        request_id = %ctx.request_id,
        provider_id = ctx.target.provider.id,
        credential_id = ctx.target.credential.0,
        operation = ?ctx.key.map(|key| key.operation),
        surface = ctx.surface_label.unwrap_or(""),
        error_kind = transport_error_kind(error),
        "attempt.completed"
    );
}

pub(crate) async fn attempt_response<H: Host>(
    host: &H,
    ctx: &FunnelCtx,
    status: http::StatusCode,
    body: Option<Bytes>,
    disposition: Disposition,
) {
    capture_response(host, ctx, status, body).await;
    tracing::info!(
        request_id = %ctx.request_id,
        provider_id = ctx.target.provider.id,
        credential_id = ctx.target.credential.0,
        operation = ?ctx.key.map(|key| key.operation),
        surface = ctx.surface_label.unwrap_or(""),
        status = status.as_u16(),
        disposition = ?disposition,
        "attempt.completed"
    );
}

pub(crate) async fn attempt_interrupted<H: Host>(
    host: &H,
    ctx: &FunnelCtx,
    status: http::StatusCode,
    body: Bytes,
    error: &TransportError,
) {
    capture_response(host, ctx, status, Some(body)).await;
    tracing::info!(
        request_id = %ctx.request_id,
        provider_id = ctx.target.provider.id,
        credential_id = ctx.target.credential.0,
        operation = ?ctx.key.map(|key| key.operation),
        surface = ctx.surface_label.unwrap_or(""),
        status = status.as_u16(),
        error_kind = transport_error_kind(error),
        "attempt.completed"
    );
}

pub(crate) fn pre_send(ctx: &RequestCtx, target: &Target, key: OperationKey, reason: &'static str) {
    tracing::info!(
        request_id = %ctx.request_id,
        provider_id = target.provider.id,
        credential_id = target.credential.0,
        operation = ?key.operation,
        reason,
        "attempt.rejected"
    );
}

pub(crate) fn request_failed(ctx: &RequestCtx, key: Option<OperationKey>, error: &CoreError) {
    request_failed_surface(ctx, key, None, error);
}

pub(crate) fn request_failed_surface(
    ctx: &RequestCtx,
    key: Option<OperationKey>,
    surface: Option<&'static str>,
    error: &CoreError,
) {
    tracing::info!(
        request_id = %ctx.request_id,
        operation = ?key.map(|key| key.operation),
        surface = surface.unwrap_or(""),
        error_kind = core_error_kind(error),
        "request.completed"
    );
}

pub(crate) fn request_transport_failed(
    ctx: &RequestCtx,
    key: Option<OperationKey>,
    surface: Option<&'static str>,
    error: &TransportError,
) {
    tracing::info!(
        request_id = %ctx.request_id,
        operation = ?key.map(|key| key.operation),
        surface = surface.unwrap_or(""),
        error_kind = transport_error_kind(error),
        "request.completed"
    );
}

async fn capture_transport<H: Host>(host: &H, ctx: &FunnelCtx) {
    host.capture()
        .record(&Capture {
            request_id: ctx.request_id.clone(),
            upstream_url: ctx.upstream_url.clone(),
            request_body: ctx.request_body.clone(),
            response_status: None,
            response_body: None,
        })
        .await;
}

async fn capture_response<H: Host>(
    host: &H,
    ctx: &FunnelCtx,
    status: http::StatusCode,
    body: Option<Bytes>,
) {
    host.capture()
        .record(&Capture {
            request_id: ctx.request_id.clone(),
            upstream_url: ctx.upstream_url.clone(),
            request_body: ctx.request_body.clone(),
            response_status: Some(status),
            response_body: body,
        })
        .await;
}

pub(crate) fn transport_error_kind(error: &TransportError) -> &'static str {
    match error {
        TransportError::Connect(_) => "connect",
        TransportError::Timeout => "timeout",
        TransportError::Interrupted(_) => "interrupted",
    }
}

fn core_error_kind(error: &CoreError) -> &'static str {
    match error {
        CoreError::Unauthorized => "unauthorized",
        CoreError::Forbidden(_) => "forbidden",
        CoreError::UnknownRoute(_) => "unknown_route",
        CoreError::UnknownProvider(_) => "unknown_provider",
        CoreError::Unsupported => "unsupported",
        CoreError::RateLimited { .. } => "rate_limited",
        CoreError::QuotaExceeded => "quota_exceeded",
        CoreError::NoCredentials => "no_credentials",
        CoreError::Transform(_) => "transform",
        CoreError::UpstreamExhausted(_) => "upstream_exhausted",
        CoreError::Transport(error) => transport_error_kind(error),
        CoreError::Store(_) => "store",
        CoreError::Channel(_) => "channel",
        CoreError::SurfaceState(_) => "surface_state",
        CoreError::Internal(_) => "internal",
    }
}
