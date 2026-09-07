use web_time::Instant;

use gproxy_channel_api::{Channel, Disposition, PreparedRequest, StepResponse};
use gproxy_protocol::{SettleMode, StreamFraming};

use crate::Shared;
use crate::boundary::ResponseBody;
use crate::control::Target;
use crate::error::CoreError;
use crate::funnel::{self, FunnelCtx};
use crate::host::{Host, UpstreamTransport};

pub(super) async fn run<H: Host>(
    host: Shared<H>,
    channel: Option<&dyn Channel>,
    target: Target,
    credential_version: Option<u64>,
    request_id: String,
    label: &'static str,
    mut prepared: PreparedRequest,
) -> Result<StepResponse, CoreError> {
    host.admit_credential(
        &request_id,
        &target,
        prepared.request.body(),
        SettleMode::Free,
    )
    .await?;
    crate::fingerprint::apply_prepared(&mut prepared, &target.provider)?;
    let url = prepared.request.uri().to_string();
    let method = prepared.request.method().clone();
    let headers = prepared.request.headers().clone();
    let body = prepared.request.body().clone();
    let mut facts = FunnelCtx {
        upstream_started_at_ms: Some(crate::quota::now_ms()),
        request_id,
        target,
        credential_version,
        source_key: None,
        key: None,
        source_framing: StreamFraming::Sse,
        target_framing: StreamFraming::Sse,
        settle: SettleMode::Free,
        pricing: None,
        pricing_control: None,
        usage_channel: None,
        started: Instant::now(),
        upstream_url: Some(url),
        request_method: Some(method),
        request_body: body,
        request_headers: Some(headers),
        client_headers: Default::default(),
        requested_model: None,
        response_headers: None,
        dedupe_key: None,
        owner_user_id: None,
        resource: None,
        admitted: false,
        surface_label: Some(label),
        traffic_policy: None,
        traffic_blacklist: None,
    };
    let response = match host.transport().send(prepared.request).await {
        Ok(response) => response,
        Err(error) => {
            crate::funnel::health::degraded(
                host.as_ref(),
                &facts.target,
                facts.credential_version,
                None,
                "upstream transport failed",
            )
            .await;
            funnel::error::terminal_transport(host.as_ref(), &facts, &error).await;
            return Err(error.into());
        }
    };
    facts.response_headers = Some(response.headers().clone());
    let response = match crate::attempt::body::collect(response).await {
        Ok(response) => response,
        Err(failure) => {
            crate::funnel::health::degraded(
                host.as_ref(),
                &facts.target,
                facts.credential_version,
                Some(failure.status),
                "upstream response interrupted",
            )
            .await;
            let outcome = funnel::free_buffered(
                host.as_ref(),
                facts,
                failure.status,
                failure.headers,
                failure.body,
                Disposition::Terminal,
            )
            .await;
            drop(outcome);
            return Err(failure.error.into());
        }
    };
    let (parts, body) = response.into_parts();
    let disposition = if parts.status.is_success() {
        Disposition::Success
    } else {
        Disposition::Terminal
    };
    // Cleanup calls run detached without a channel handle; they also carry
    // no credential version, so health recording is a no-op for them anyway.
    if let Some(channel) = channel {
        crate::funnel::health::response(
            host.as_ref(),
            channel,
            &facts,
            disposition,
            parts.status,
            &parts.headers,
        )
        .await;
    }
    let outcome = funnel::free_buffered(
        host.as_ref(),
        facts,
        parts.status,
        parts.headers,
        body,
        disposition,
    )
    .await;
    let ResponseBody::Full(body) = outcome.body else {
        return Err(CoreError::Internal(
            "orchestration side call was not buffered".into(),
        ));
    };
    Ok(StepResponse {
        status: outcome.status,
        headers: outcome.headers,
        body,
    })
}
