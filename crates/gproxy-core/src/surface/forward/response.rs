use gproxy_channel_api::{ByteStream, Channel, Disposition, ResponseView, StreamCtx};
use gproxy_protocol::OperationKey;

use crate::api::Core;
use crate::boundary::ExecOutcome;
use crate::error::CoreError;
use crate::funnel::error as funnel_error;
use crate::funnel::{self, FunnelCtx};
use crate::host::Host;

pub(super) async fn relay<H: Host>(
    core: &Core<H>,
    channel: &dyn Channel,
    stream: bool,
    key: Option<OperationKey>,
    facts: FunnelCtx,
    response: http::Response<ByteStream>,
) -> Result<ExecOutcome, CoreError> {
    if stream && response.status().is_success() {
        let disposition = classify(channel, &response, &[]);
        crate::funnel::health::response(
            core.host.as_ref(),
            &facts.target,
            facts.credential_version,
            disposition,
            response.status(),
        )
        .await;
        return if let Some(key) = key {
            let decoder = channel.stream_decoder(StreamCtx {
                key,
                framing: facts.target_framing,
                request_body: &facts.request_body,
                response_headers: response.headers(),
            });
            Ok(funnel::streaming(
                core.host.clone(),
                facts,
                response,
                disposition,
                decoder,
            ))
        } else {
            let (parts, body) = response.into_parts();
            Ok(funnel::free_streaming(
                core.host.clone(),
                facts,
                parts.status,
                parts.headers,
                body,
                disposition,
            ))
        };
    }

    let response = match crate::attempt::body::collect(response).await {
        Ok(response) => response,
        Err(failure) => {
            crate::funnel::health::degraded(
                core.host.as_ref(),
                &facts.target,
                facts.credential_version,
                Some(failure.status),
                "upstream response interrupted",
            )
            .await;
            funnel_error::attempt_interrupted(
                core.host.as_ref(),
                &facts,
                failure.status,
                failure.body,
                &failure.error,
            )
            .await;
            return Err(failure.error.into());
        }
    };
    let disposition = classify(channel, &response, response.body());
    crate::funnel::health::response(
        core.host.as_ref(),
        &facts.target,
        facts.credential_version,
        disposition,
        response.status(),
    )
    .await;
    if key.is_some() {
        Ok(funnel::buffered(
            core.host.as_ref(),
            channel,
            facts,
            funnel::BufferedRelay::native(response),
            disposition,
        )
        .await)
    } else {
        let (parts, body) = response.into_parts();
        Ok(funnel::free_buffered(
            core.host.as_ref(),
            facts,
            parts.status,
            parts.headers,
            body,
            disposition,
        )
        .await)
    }
}

fn classify<B>(channel: &dyn Channel, response: &http::Response<B>, body: &[u8]) -> Disposition {
    channel.classify(ResponseView {
        status: response.status(),
        headers: response.headers(),
        body,
    })
}
