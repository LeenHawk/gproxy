use gproxy_channel_api::PreparedRequest;

use crate::host::{Capture, CaptureSink, Host, UpstreamTransport};

use super::super::FunnelCtx;

pub(super) async fn send<H: Host>(host: &H, ctx: &FunnelCtx, prepared: PreparedRequest) {
    let request = prepared.request;
    let url = request.uri().to_string();
    let method = request.method().clone();
    let headers = request.headers().clone();
    let request_body = request.body().clone();
    let response = host.transport().send(request).await;
    let (status, response_headers, response_body, error) = match response {
        Ok(response) => match crate::attempt::body::collect(response).await {
            Ok(response) => {
                let (parts, body) = response.into_parts();
                let error = (!parts.status.is_success())
                    .then(|| format!("Realtime hangup returned {}", parts.status));
                (Some(parts.status), Some(parts.headers), Some(body), error)
            }
            Err(failure) => (
                Some(failure.status),
                Some(failure.headers),
                Some(failure.body),
                Some(failure.error.to_string()),
            ),
        },
        Err(error) => (None, None, None, Some(error.to_string())),
    };
    host.capture()
        .record(&Capture {
            request_id: format!("{}:sideband:hangup", ctx.request_id),
            provider_id: Some(ctx.target.provider.id),
            credential_id: Some(ctx.target.credential),
            upstream_url: Some(url),
            request_method: Some(method),
            request_headers: Some(headers),
            request_body,
            response_status: status,
            response_headers,
            response_body,
        })
        .await;
    if let Some(error) = error {
        tracing::error!(request_id = %ctx.request_id, error, "Realtime hangup failed");
    }
}
