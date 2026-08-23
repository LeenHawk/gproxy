use std::time::Instant;

use bytes::Bytes;
use gproxy_channel_api::{
    Channel, Disposition, PrepareCtx, ResponseView, StreamDecoder, TransportError,
};
use gproxy_protocol::OperationKey;

use crate::api::Core;
use crate::boundary::{ByteStream, ExecOutcome, RequestCtx};
use crate::control::{ControlPlane, Target};
use crate::error::CoreError;
use crate::funnel::{self, FunnelCtx};
use crate::host::{Host, UpstreamTransport};
use crate::request::Classified;

pub(crate) struct Prepared {
    channel: &'static str,
    request: http::Request<Bytes>,
    facts: FunnelCtx,
}

pub(crate) struct Completed {
    channel: &'static str,
    pub facts: FunnelCtx,
    pub disposition: Disposition,
    body: AttemptBody,
}

enum AttemptBody {
    Buffered(http::Response<Bytes>),
    Streaming(http::Response<ByteStream>, Option<Box<dyn StreamDecoder>>),
}

pub(crate) enum Failure {
    Transport {
        facts: FunnelCtx,
        error: TransportError,
    },
    Interrupted {
        channel: &'static str,
        facts: FunnelCtx,
        status: http::StatusCode,
        body: Bytes,
        error: TransportError,
    },
}

pub(crate) fn supports<H: Host>(
    core: &Core<H>,
    target: &Target,
    key: OperationKey,
) -> Result<bool, CoreError> {
    let channel = channel(core, &target.provider.channel)?;
    Ok(channel.descriptor().supports.contains(&key))
}

pub(crate) async fn prepare<H: Host>(
    core: &Core<H>,
    control: &impl ControlPlane,
    target: &Target,
    ctx: &RequestCtx,
    classified: &Classified,
    admitted: bool,
    started: Instant,
) -> Result<Prepared, CoreError> {
    let channel = channel(core, &target.provider.channel)?;
    let credential =
        crate::credential::load_fresh(core.host.as_ref(), channel, target.credential).await?;
    let prepared = channel.prepare(PrepareCtx {
        key: classified.key,
        stream: classified.stream,
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
    Ok(Prepared {
        channel: channel.descriptor().id,
        facts: FunnelCtx {
            request_id: ctx.request_id.clone(),
            target: target.clone(),
            key: classified.key,
            settle: classified.settle,
            pricing: control.pricing(&target.provider, &target.upstream_model),
            started,
            upstream_url: prepared.request.uri().to_string(),
            request_body: prepared.request.body().clone(),
            dedupe_key: classified.dedupe_key(target.provider.id),
            admitted,
        },
        request: prepared.request,
    })
}

pub(crate) async fn send<H: Host>(
    core: &Core<H>,
    prepared: Prepared,
    classified: &Classified,
) -> Result<Completed, Failure> {
    let Prepared {
        channel,
        request,
        facts,
    } = prepared;
    let response = match core.host.transport().send(request).await {
        Ok(response) => response,
        Err(error) => return Err(Failure::Transport { facts, error }),
    };
    let channel = core
        .channels
        .get(channel)
        .expect("prepared attempt channel remains registered");
    if classified.stream && response.status().is_success() {
        let disposition = classify(channel, &response, &[]);
        return Ok(Completed {
            channel: channel.descriptor().id,
            facts,
            disposition,
            body: AttemptBody::Streaming(response, channel.stream_decoder(classified.key)),
        });
    }
    let response = match crate::attempt_body::collect(response).await {
        Ok(response) => response,
        Err(failure) => {
            return Err(Failure::Interrupted {
                channel: channel.descriptor().id,
                facts,
                status: failure.status,
                body: failure.body,
                error: failure.error,
            });
        }
    };
    let disposition = classify(channel, &response, response.body());
    Ok(Completed {
        channel: channel.descriptor().id,
        facts,
        disposition,
        body: AttemptBody::Buffered(response),
    })
}

pub(crate) async fn finish<H: Host>(core: &Core<H>, completed: Completed) -> ExecOutcome {
    let channel = core
        .channels
        .get(completed.channel)
        .expect("completed attempt channel remains registered");
    match completed.body {
        AttemptBody::Buffered(response) => {
            funnel::buffered(
                core.host.as_ref(),
                channel,
                completed.facts,
                response,
                completed.disposition,
            )
            .await
        }
        AttemptBody::Streaming(response, decoder) => funnel::streaming(
            core.host.clone(),
            completed.facts,
            response,
            completed.disposition,
            decoder,
        ),
    }
}

pub(crate) fn discard(completed: Completed) -> (FunnelCtx, http::StatusCode, Option<Bytes>) {
    match completed.body {
        AttemptBody::Buffered(response) => {
            let (parts, body) = response.into_parts();
            (completed.facts, parts.status, Some(body))
        }
        AttemptBody::Streaming(response, _) => (completed.facts, response.status(), None),
    }
}

fn channel<'a, H: Host>(core: &'a Core<H>, id: &str) -> Result<&'a dyn Channel, CoreError> {
    core.channels
        .get(id)
        .ok_or_else(|| CoreError::Internal(format!("provider references unknown channel `{id}`")))
}

fn classify<B>(channel: &dyn Channel, response: &http::Response<B>, body: &[u8]) -> Disposition {
    channel.classify(ResponseView {
        status: response.status(),
        headers: response.headers(),
        body,
    })
}
