use web_time::Instant;

use gproxy_channel_api::{
    Disposition, ProviderView, SurfaceAction, SurfaceBody, SurfaceInvoke, SurfaceServices, SynthCtx,
};
use gproxy_protocol::SettleMode;

use crate::api::Core;
use crate::boundary::{ExecOutcome, RequestCtx};
use crate::control::{ControlPlane, Plan};
use crate::error::CoreError;
use crate::funnel::{self, FunnelCtx};
use crate::host::Host;

use super::affinity::Selected;
use super::invoke::SurfaceCaller;

pub(crate) async fn run<H: Host>(
    core: &Core<H>,
    control: &dyn ControlPlane,
    ctx: &RequestCtx,
    plan: &Plan,
    identity: &gproxy_channel_api::CallerIdentity,
    selected: Selected,
    started: Instant,
) -> Result<ExecOutcome, CoreError> {
    let (handler, upstream, sensitive) = match &selected.entry.action {
        SurfaceAction::Synthesize { handler, upstream } => (*handler, *upstream, false),
        SurfaceAction::PublicSynthesize { handler } => (*handler, false, true),
        _ => {
            return Err(CoreError::Internal(
                "forward action reached the synthesizer engine".into(),
            ));
        }
    };
    let reply = {
        let caller = upstream.then(|| {
            SurfaceCaller::new(
                core,
                control,
                selected.target.clone(),
                identity.clone(),
                plan.clone(),
                ctx.request_id.clone(),
            )
        });
        let usage = core.host.surface_usage(
            identity,
            &selected.target.provider,
            selected.target.credential,
        );
        let provider = ProviderView {
            id: selected.target.provider.id,
            name: &selected.target.provider.name,
            settings: &selected.target.provider.settings,
        };
        let credential = selected.target.credential;
        let mut seen = std::collections::BTreeSet::new();
        let credentials = plan
            .targets
            .iter()
            .filter(|target| target.provider.id == selected.target.provider.id)
            .filter_map(|target| seen.insert(target.credential).then_some(target.credential))
            .collect::<Vec<_>>();
        handler
            .respond(
                SynthCtx {
                    method: &ctx.method,
                    path: &ctx.path,
                    query: ctx.query.as_deref(),
                    headers: &ctx.headers,
                    body: &ctx.body,
                    params: &selected.params,
                    route_name: match &ctx.mode {
                        crate::RoutingMode::Named { name } => Some(name.as_str()),
                        crate::RoutingMode::Scoped { provider } => Some(provider.as_str()),
                        crate::RoutingMode::Aggregated | crate::RoutingMode::Namespace { .. } => {
                            None
                        }
                    },
                },
                SurfaceServices {
                    invoke: caller.as_ref().map(|caller| caller as &dyn SurfaceInvoke),
                    bindings: core
                        .host
                        .bindings()
                        .expect("surface registration requires a binding store"),
                    identity,
                    provider: &provider,
                    credential,
                    credentials: &credentials,
                    usage: usage.as_ref(),
                    oauth: core.host.oauth(),
                },
            )
            .await?
    };
    finish(
        core,
        ctx,
        selected,
        reply,
        identity.user_id,
        started,
        sensitive,
    )
    .await
}

async fn finish<H: Host>(
    core: &Core<H>,
    request: &RequestCtx,
    selected: Selected,
    reply: gproxy_channel_api::SurfaceReply,
    owner_user_id: i64,
    started: Instant,
    sensitive: bool,
) -> Result<ExecOutcome, CoreError> {
    let disposition = if reply.status.is_success() {
        Disposition::Success
    } else {
        Disposition::Terminal
    };
    let ctx = FunnelCtx {
        upstream_started_at_ms: None,
        request_id: request.request_id.clone(),
        target: selected.target,
        credential_version: None,
        source_key: None,
        key: None,
        source_framing: gproxy_protocol::StreamFraming::Sse,
        target_framing: gproxy_protocol::StreamFraming::Sse,
        settle: SettleMode::Free,
        pricing: None,
        pricing_control: None,
        usage_channel: None,
        started,
        upstream_url: None,
        request_method: None,
        request_body: if sensitive {
            bytes::Bytes::new()
        } else {
            request.body.clone()
        },
        request_headers: None,
        client_headers: if sensitive {
            http::HeaderMap::new()
        } else {
            request.headers.clone()
        },
        requested_model: None,
        response_headers: None,
        dedupe_key: None,
        owner_user_id: Some(owner_user_id),
        resource: None,
        admitted: true,
        surface_label: None,
        traffic_policy: None,
        traffic_blacklist: None,
    };
    Ok(match reply.body {
        SurfaceBody::Full(body) if sensitive => {
            funnel::free_uncaptured_buffered(
                core.host.as_ref(),
                ctx,
                reply.status,
                reply.headers,
                body,
                disposition,
            )
            .await
        }
        SurfaceBody::Full(body) => {
            funnel::free_buffered(
                core.host.as_ref(),
                ctx,
                reply.status,
                reply.headers,
                body,
                disposition,
            )
            .await
        }
        SurfaceBody::Stream(_) if sensitive => {
            return Err(CoreError::Internal(
                "sensitive public surfaces cannot stream".into(),
            ));
        }
        SurfaceBody::Stream(body) => {
            funnel::free_streaming(
                core.host.clone(),
                ctx,
                reply.status,
                reply.headers,
                body,
                disposition,
            )
            .await
        }
    })
}
