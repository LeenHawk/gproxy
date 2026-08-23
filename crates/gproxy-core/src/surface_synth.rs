use std::time::Instant;

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
use crate::surface_affinity::Selected;
use crate::surface_invoke::SurfaceCaller;

pub(crate) async fn run<H: Host>(
    core: &Core<H>,
    control: &impl ControlPlane,
    ctx: &RequestCtx,
    plan: &Plan,
    identity: &gproxy_channel_api::CallerIdentity,
    selected: Selected,
    started: Instant,
) -> Result<ExecOutcome, CoreError> {
    let SurfaceAction::Synthesize { handler, upstream } = &selected.entry.action else {
        return Err(CoreError::Internal(
            "forward action reached the synthesizer engine".into(),
        ));
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
        let usage = core.host.surface_usage(identity, &selected.target.provider);
        let provider = ProviderView {
            id: selected.target.provider.id,
            name: &selected.target.provider.name,
            settings: &selected.target.provider.settings,
        };
        handler
            .respond(
                SynthCtx {
                    method: &ctx.method,
                    path: &ctx.path,
                    query: ctx.query.as_deref(),
                    headers: &ctx.headers,
                    body: &ctx.body,
                    params: &selected.params,
                },
                SurfaceServices {
                    invoke: caller.as_ref().map(|caller| caller as &dyn SurfaceInvoke),
                    bindings: core
                        .host
                        .bindings()
                        .expect("surface registration requires a binding store"),
                    identity,
                    provider: &provider,
                    usage: usage.as_ref(),
                },
            )
            .await?
    };
    finish(core, ctx, selected, reply, started).await
}

async fn finish<H: Host>(
    core: &Core<H>,
    request: &RequestCtx,
    selected: Selected,
    reply: gproxy_channel_api::SurfaceReply,
    started: Instant,
) -> Result<ExecOutcome, CoreError> {
    let disposition = if reply.status.is_success() {
        Disposition::Success
    } else {
        Disposition::Terminal
    };
    let ctx = FunnelCtx {
        request_id: request.request_id.clone(),
        target: selected.target,
        key: None,
        settle: SettleMode::Free,
        pricing: None,
        started,
        upstream_url: None,
        request_body: request.body.clone(),
        dedupe_key: None,
        admitted: true,
        surface_label: None,
    };
    Ok(match reply.body {
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
        SurfaceBody::Stream(body) => funnel::free_streaming(
            core.host.clone(),
            ctx,
            reply.status,
            reply.headers,
            body,
            disposition,
        ),
    })
}
