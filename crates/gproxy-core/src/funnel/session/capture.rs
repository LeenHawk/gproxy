use bytes::Bytes;

use crate::host::{Capture, CaptureSink, Host};

use super::super::FunnelCtx;

pub(super) async fn call<H: Host>(
    host: &H,
    ctx: &FunnelCtx,
    status: http::StatusCode,
    body: Bytes,
) {
    let (provider_id, credential_id) = ctx.capture_attribution();
    host.capture()
        .record(&Capture {
            request_id: ctx.request_id.clone(),
            provider_id,
            credential_id,
            upstream_url: ctx.upstream_url.clone(),
            request_body: ctx.request_body.clone(),
            response_status: Some(status),
            response_body: Some(body),
        })
        .await;
}

pub(super) async fn sideband<H: Host>(
    host: &H,
    ctx: &FunnelCtx,
    attempt: u64,
    url: String,
    request_body: Bytes,
    connected: bool,
) {
    host.capture()
        .record(&Capture {
            request_id: format!("{}:sideband:{attempt}", ctx.request_id),
            provider_id: Some(ctx.target.provider.id),
            credential_id: Some(ctx.target.credential),
            upstream_url: Some(url),
            request_body,
            response_status: connected.then_some(http::StatusCode::SWITCHING_PROTOCOLS),
            response_body: None,
        })
        .await;
}
