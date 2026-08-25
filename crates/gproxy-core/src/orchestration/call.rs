use web_time::Instant;

use gproxy_channel_api::{Disposition, PreparedRequest, StepResponse};
use gproxy_protocol::{SettleMode, StreamFraming};

use crate::Shared;
use crate::boundary::ResponseBody;
use crate::control::Target;
use crate::error::CoreError;
use crate::funnel::{self, FunnelCtx};
use crate::host::{Host, UpstreamTransport};

pub(super) async fn run<H: Host>(
    host: Shared<H>,
    target: Target,
    credential_version: Option<u64>,
    request_id: String,
    label: &'static str,
    mut prepared: PreparedRequest,
) -> Result<StepResponse, CoreError> {
    crate::fingerprint::apply_prepared(&mut prepared, &target.provider)?;
    let url = prepared.request.uri().to_string();
    let body = prepared.request.body().clone();
    let facts = FunnelCtx {
        request_id,
        target,
        credential_version,
        source_key: None,
        key: None,
        source_framing: StreamFraming::Sse,
        target_framing: StreamFraming::Sse,
        settle: SettleMode::Free,
        pricing: None,
        started: Instant::now(),
        upstream_url: Some(url),
        request_body: body,
        dedupe_key: None,
        owner_user_id: None,
        resource: None,
        admitted: false,
        surface_label: Some(label),
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
    crate::funnel::health::response(
        host.as_ref(),
        &facts.target,
        facts.credential_version,
        disposition,
        parts.status,
    )
    .await;
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
