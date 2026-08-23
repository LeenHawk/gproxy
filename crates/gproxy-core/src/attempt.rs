use std::time::Instant;

use bytes::Bytes;
use gproxy_channel_api::{
    Channel, ChannelSupport, Disposition, PrepareCtx, ResponseView, StreamCtx, StreamDecoder,
    TransportError,
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
    stream: bool,
    facts: FunnelCtx,
}

pub(crate) struct Completed {
    channel: &'static str,
    pub facts: FunnelCtx,
    pub disposition: Disposition,
    body: AttemptBody,
}

#[derive(Clone, Copy)]
pub(crate) struct AdmissionCtx {
    pub admitted: bool,
    pub owner_user_id: Option<i64>,
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
        headers: http::HeaderMap,
        body: Bytes,
        error: TransportError,
    },
}

pub(crate) fn support<H: Host>(
    core: &Core<H>,
    target: &Target,
    key: OperationKey,
) -> Result<Option<ChannelSupport>, CoreError> {
    let channel = channel(core, &target.provider.channel)?;
    Ok(channel
        .descriptor()
        .supports
        .iter()
        .find(|support| support.source == key)
        .copied())
}

pub(crate) fn native_support<H: Host>(
    core: &Core<H>,
    target: &Target,
    key: OperationKey,
) -> Result<Option<ChannelSupport>, CoreError> {
    Ok(support(core, target, key)?.filter(|support| support.source == support.target))
}

pub(crate) async fn prepare<H: Host>(
    core: &Core<H>,
    control: &impl ControlPlane,
    target: &Target,
    ctx: &RequestCtx,
    classified: &Classified,
    admission: AdmissionCtx,
    started: Instant,
) -> Result<Prepared, CoreError> {
    let channel = channel(core, &target.provider.channel)?;
    let support = support(core, target, classified.key)?.ok_or(CoreError::Unsupported)?;
    if !admission.admitted && support.source != support.target {
        return Err(CoreError::Unsupported);
    }
    let credential =
        crate::credential::load_fresh(core.host.as_ref(), channel, target.credential).await?;
    let stream = classified.stream
        || support.target.operation == gproxy_protocol::Operation::StreamGenerateContent;
    let mut method = ctx.method.clone();
    let mut path = ctx.path.clone();
    let mut body = ctx.body.clone();
    if support.source != support.target {
        body = gproxy_transform::request(
            support.source,
            support.target,
            body,
            &target.upstream_model,
            stream,
        )
        .map_err(|error| CoreError::Transform(error.to_string()))?;
        (method, path) = gproxy_protocol::request_target(support.target, &target.upstream_model)
            .ok_or_else(|| {
                CoreError::Transform(format!("no request target for {:?}", support.target))
            })?;
    }
    let prepared = channel.prepare(PrepareCtx {
        key: support.target,
        stream,
        method: &method,
        path: &path,
        query: ctx.query.as_deref(),
        headers: &ctx.headers,
        body: &body,
        upstream_model: &target.upstream_model,
        provider_settings: &target.provider.settings,
        secret: &credential.secret,
    })?;
    if prepared.websocket {
        return Err(CoreError::Unsupported);
    }
    Ok(Prepared {
        channel: channel.descriptor().id,
        stream,
        facts: FunnelCtx {
            request_id: ctx.request_id.clone(),
            target: target.clone(),
            source_key: Some(support.source),
            key: Some(support.target),
            settle: support.target.operation.spec().settle,
            pricing: control.pricing(&target.provider, &target.upstream_model),
            started,
            upstream_url: Some(prepared.request.uri().to_string()),
            request_body: prepared.request.body().clone(),
            dedupe_key: classified.dedupe_key(target.provider.id),
            owner_user_id: admission.owner_user_id,
            resource: classified
                .resource()
                .map(|(kind, id)| (kind, id.to_owned())),
            admitted: admission.admitted,
            surface_label: None,
        },
        request: prepared.request,
    })
}

pub(crate) async fn send<H: Host>(
    core: &Core<H>,
    prepared: Prepared,
) -> Result<Completed, Failure> {
    let Prepared {
        channel,
        request,
        stream,
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
    if stream && response.status().is_success() {
        let disposition = classify(channel, &response, &[]);
        let key = facts.key.expect("operation attempt has an upstream key");
        let mut decoder = channel.stream_decoder(StreamCtx {
            key,
            request_body: &facts.request_body,
            response_headers: response.headers(),
        });
        let source = facts
            .source_key
            .expect("operation attempt has a source key");
        if source != key {
            decoder = Some(Box::new(crate::transform_stream::TransformDecoder::new(
                source, key, decoder,
            )));
        }
        return Ok(Completed {
            channel: channel.descriptor().id,
            facts,
            disposition,
            body: AttemptBody::Streaming(response, decoder),
        });
    }
    let response = match crate::attempt_body::collect(response).await {
        Ok(response) => response,
        Err(failure) => {
            return Err(Failure::Interrupted {
                channel: channel.descriptor().id,
                facts,
                status: failure.status,
                headers: failure.headers,
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
