use std::time::Instant;

use gproxy_channel_api::{Disposition, ResponseView, StreamCtx, SurfaceRequest};
use gproxy_protocol::SettleMode;

use crate::api::Core;
use crate::boundary::{ExecOutcome, RequestCtx};
use crate::control::{Pricing, Target};
use crate::error::CoreError;
use crate::funnel::{self, FunnelCtx};
use crate::funnel_error;
use crate::host::{Host, UpstreamTransport};
use crate::surface_affinity::Selected;

pub(crate) async fn declared<H: Host>(
    core: &Core<H>,
    selected: &Selected,
    ctx: &RequestCtx,
    started: Instant,
) -> Result<ExecOutcome, CoreError> {
    let (spec, websocket) = match &selected.entry.action {
        gproxy_channel_api::SurfaceAction::Forward(spec) => (spec, false),
        gproxy_channel_api::SurfaceAction::ForwardWebSocket(spec) => (spec, true),
        gproxy_channel_api::SurfaceAction::Synthesize { .. } => {
            return Err(CoreError::Internal(
                "synthesizer reached the forward engine".into(),
            ));
        }
    };
    let surface_request = SurfaceRequest {
        label: spec.label,
        key: None,
        stream: !websocket
            && !matches!(
                selected.entry.affinity,
                gproxy_channel_api::SurfaceAffinity::BodyField { .. }
            ),
        method: ctx.method.clone(),
        upstream_path: crate::surface_template::render(spec.upstream_template, &selected.params)?,
        query: ctx.query.clone(),
        headers: ctx.headers.clone(),
        body: ctx.body.clone(),
        credential: Some(selected.target.credential),
    };
    request(
        core,
        &selected.target,
        surface_request,
        websocket,
        ctx.request_id.clone(),
        started,
        None,
    )
    .await
}

pub(crate) async fn request<H: Host>(
    core: &Core<H>,
    target: &Target,
    request: SurfaceRequest,
    websocket: bool,
    request_id: String,
    started: Instant,
    pricing: Option<Pricing>,
) -> Result<ExecOutcome, CoreError> {
    let channel = core.channels.get(&target.provider.channel).ok_or_else(|| {
        CoreError::Internal(format!(
            "provider references unknown channel `{}`",
            target.provider.channel
        ))
    })?;
    if let Some(key) = request.key
        && (!channel
            .descriptor()
            .supports
            .iter()
            .any(|support| support.target == key)
            || key.operation.spec().settle == SettleMode::OnCompletedStatus)
    {
        return Err(CoreError::Unsupported);
    }
    let credential =
        crate::credential::load_fresh(core.host.as_ref(), channel, target.credential).await?;
    let prepared = channel.prepare_surface(
        &request,
        websocket,
        &target.provider.settings,
        &credential.secret,
    )?;
    if prepared.websocket != websocket {
        return Err(CoreError::Internal(
            "surface websocket preparation disagrees with its table action".into(),
        ));
    }
    if websocket && request.key.is_some() {
        return Err(CoreError::Unsupported);
    }
    let facts = FunnelCtx {
        request_id,
        target: target.clone(),
        source_key: request.key,
        key: request.key,
        settle: request
            .key
            .map(|key| key.operation.spec().settle)
            .unwrap_or(SettleMode::Free),
        pricing,
        started,
        upstream_url: Some(prepared.request.uri().to_string()),
        request_body: prepared.request.body().clone(),
        dedupe_key: None,
        owner_user_id: None,
        resource: None,
        admitted: true,
        surface_label: Some(request.label),
    };
    if websocket {
        return match core.host.transport().open_websocket(prepared.request).await {
            Ok(socket) => Ok(funnel::websocket(core.host.clone(), facts, socket)),
            Err(error) => {
                funnel_error::attempt_transport(core.host.as_ref(), &facts, &error).await;
                Err(error.into())
            }
        };
    }

    let response = match core.host.transport().send(prepared.request).await {
        Ok(response) => response,
        Err(error) => {
            funnel_error::attempt_transport(core.host.as_ref(), &facts, &error).await;
            return Err(error.into());
        }
    };
    if request.stream && response.status().is_success() {
        let disposition = classify(channel, &response, &[]);
        return if let Some(key) = request.key {
            let decoder = channel.stream_decoder(StreamCtx {
                key,
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

    let response = match crate::attempt_body::collect(response).await {
        Ok(response) => response,
        Err(failure) => {
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
    if request.key.is_some() {
        Ok(funnel::buffered(core.host.as_ref(), channel, facts, response, disposition).await)
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

fn classify<B>(
    channel: &dyn gproxy_channel_api::Channel,
    response: &http::Response<B>,
    body: &[u8],
) -> Disposition {
    channel.classify(ResponseView {
        status: response.status(),
        headers: response.headers(),
        body,
    })
}
