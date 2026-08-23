use std::time::Instant;

use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use gproxy_channel_api::{Channel, PrepareCtx, ResponseView, TransportError};
use gproxy_protocol::{
    Affinity, OperationKey, SettleMode, StreamDetect, match_ingress, streaming_sibling,
};

use crate::api::Core;
use crate::boundary::RequestCtx;
use crate::control::{ControlPlane, Target};
use crate::error::CoreError;
use crate::funnel::{self, FunnelCtx};
use crate::host::{Host, UpstreamTransport};

pub(crate) async fn run<H: Host>(
    core: &Core<H>,
    control: &impl ControlPlane,
    target: &Target,
    ctx: RequestCtx,
) -> Result<crate::boundary::ExecOutcome, CoreError> {
    let matched = match_ingress(&ctx.method, &ctx.path).ok_or(CoreError::Unsupported)?;
    if matched.upgrade {
        return Err(CoreError::Unsupported);
    }
    let stream = detects_stream(matched.stream, &ctx.body);
    let operation = if stream {
        streaming_sibling(matched.operation).unwrap_or(matched.operation)
    } else {
        matched.operation
    };
    let key = OperationKey {
        operation,
        kind: matched.kind,
    };
    let spec = operation.spec();
    let dedupe_key = match (spec.settle, spec.affinity, matched.params.first()) {
        (SettleMode::OnCompletedStatus, Affinity::Resource(kind), Some((_, id))) => {
            Some(format!("gproxy:settle:{}:{kind}:{id}", target.provider.id))
        }
        _ => None,
    };
    let channel = core.channels.get(&target.provider.channel).ok_or_else(|| {
        CoreError::Internal(format!(
            "provider references unknown channel `{}`",
            target.provider.channel
        ))
    })?;
    if !channel.descriptor().supports.contains(&key) {
        return Err(CoreError::Unsupported);
    }

    let credential =
        crate::credential::load_fresh(core.host.as_ref(), channel, target.credential).await?;
    let prepared = channel.prepare(PrepareCtx {
        key,
        stream,
        method: &ctx.method,
        path: &ctx.path,
        query: ctx.query.as_deref(),
        headers: &ctx.headers,
        body: &ctx.body,
        upstream_model: &target.upstream_model,
        provider_settings: &target.provider.settings,
        secret: &credential.secret,
    })?;
    if prepared.websocket {
        return Err(CoreError::Unsupported);
    }

    let facts = FunnelCtx {
        request_id: ctx.request_id,
        target: target.clone(),
        key,
        settle: spec.settle,
        pricing: control.pricing(&target.provider, &target.upstream_model),
        started: Instant::now(),
        upstream_url: prepared.request.uri().to_string(),
        request_body: prepared.request.body().clone(),
        dedupe_key,
    };
    let response = match core.host.transport().send(prepared.request).await {
        Ok(response) => response,
        Err(error) => {
            funnel::transport_failed(core.host.as_ref(), &facts, &error).await;
            return Err(error.into());
        }
    };
    if stream && response.status().is_success() {
        let disposition = classify(channel, &response, &[]);
        let decoder = channel.stream_decoder(key);
        return Ok(funnel::streaming(
            core.host.clone(),
            facts,
            response,
            disposition,
            decoder,
        ));
    }

    match collect(response).await {
        Ok(response) => {
            let disposition = classify(channel, &response, response.body());
            Ok(funnel::buffered(core.host.as_ref(), channel, facts, response, disposition).await)
        }
        Err(failure) => {
            funnel::interrupted(
                core.host.as_ref(),
                channel,
                facts,
                failure.status,
                failure.body,
            )
            .await;
            Err(failure.error.into())
        }
    }
}

fn detects_stream(detect: StreamDetect, body: &[u8]) -> bool {
    match detect {
        StreamDetect::Never => false,
        StreamDetect::Always => true,
        StreamDetect::BodyFlag(field) => serde_json::from_slice::<serde_json::Value>(body)
            .ok()
            .and_then(|body| body.get(field)?.as_bool())
            .unwrap_or(false),
    }
}

fn classify<B>(
    channel: &dyn Channel,
    response: &http::Response<B>,
    body: &[u8],
) -> gproxy_channel_api::Disposition {
    channel.classify(ResponseView {
        status: response.status(),
        headers: response.headers(),
        body,
    })
}

struct BodyFailure {
    status: http::StatusCode,
    body: Bytes,
    error: TransportError,
}

async fn collect(
    response: http::Response<crate::boundary::ByteStream>,
) -> Result<http::Response<Bytes>, BodyFailure> {
    let (parts, mut stream) = response.into_parts();
    let mut body = BytesMut::new();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(chunk) => body.extend_from_slice(&chunk),
            Err(error) => {
                return Err(BodyFailure {
                    status: parts.status,
                    body: body.freeze(),
                    error,
                });
            }
        }
    }
    Ok(http::Response::from_parts(parts, body.freeze()))
}
