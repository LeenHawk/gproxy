use std::time::Instant;

use gproxy_channel_api::{Disposition, SurfaceAction};

use crate::api::Core;
use crate::boundary::{ExecOutcome, RequestCtx};
use crate::control::{ControlPlane, Plan};
use crate::error::CoreError;
use crate::funnel_error;
use crate::host::Host;
use crate::surface_affinity::Selected;

pub(crate) async fn dispatch<H: Host>(
    core: &Core<H>,
    control: &impl ControlPlane,
    ctx: &RequestCtx,
    planned: Option<&Plan>,
) -> Option<Result<ExecOutcome, CoreError>> {
    let matches = crate::surface_affinity::table_matches(core, ctx);
    if matches.is_empty() {
        return None;
    }
    Some(run(core, control, ctx, planned, matches).await)
}

async fn run<H: Host>(
    core: &Core<H>,
    control: &impl ControlPlane,
    ctx: &RequestCtx,
    planned: Option<&Plan>,
    matches: Vec<crate::surface_affinity::TableMatch>,
) -> Result<ExecOutcome, CoreError> {
    let started = Instant::now();
    let matched_label = matches
        .first()
        .and_then(|matched| action_label(&matched.entry.action));
    let identity = match core.host.authenticate(ctx).await {
        Ok(identity) => identity,
        Err(error) => return reject(ctx, matched_label, error),
    };
    let plan = match planned {
        Some(plan) => plan.clone(),
        None => match control.resolve(None, &ctx.mode) {
            Ok(plan) => plan,
            Err(error) => return reject(ctx, matched_label, error),
        },
    };
    if let Err(error) = core.host.admit(&identity, ctx, None, &plan).await {
        return reject(ctx, matched_label, error);
    }
    let mut selected =
        match crate::surface_affinity::select(core, ctx, &identity, &plan, matches).await {
            Ok(selected) => selected,
            Err(error) => {
                core.host.finish_admission(&ctx.request_id, None).await;
                funnel_error::request_failed_surface(ctx, None, matched_label, &error);
                return Err(error);
            }
        };
    let surface_label = action_label(&selected.entry.action);
    let affinity = selected.entry.affinity;
    let pin_target = selected.target.clone();
    let pin = selected.pin.take();
    let result = action(core, control, ctx, &plan, &identity, selected, started).await;
    let commits_pin = result
        .as_ref()
        .is_ok_and(|outcome| outcome.disposition == Disposition::Success);
    if commits_pin {
        if let Some(pin) = pin {
            crate::surface_pin::commit(core, pin).await;
        }
        if let Some(pin) = result.as_ref().ok().and_then(|outcome| {
            crate::surface_pin::response_pin(affinity, &identity, &pin_target, outcome)
        }) {
            crate::surface_pin::commit(core, pin).await;
        }
    }
    if let Err(error) = &result {
        core.host.finish_admission(&ctx.request_id, None).await;
        funnel_error::request_failed_surface(ctx, None, surface_label, error);
    }
    result
}

async fn action<H: Host>(
    core: &Core<H>,
    control: &impl ControlPlane,
    ctx: &RequestCtx,
    plan: &Plan,
    identity: &gproxy_channel_api::CallerIdentity,
    selected: Selected,
    started: Instant,
) -> Result<ExecOutcome, CoreError> {
    match &selected.entry.action {
        SurfaceAction::Forward(_) | SurfaceAction::ForwardWebSocket(_) => {
            crate::surface_forward::declared(core, &selected, ctx, started).await
        }
        SurfaceAction::Synthesize { .. } => {
            crate::surface_synth::run(core, control, ctx, plan, identity, selected, started).await
        }
    }
}

fn reject<T>(
    ctx: &RequestCtx,
    surface: Option<&'static str>,
    error: CoreError,
) -> Result<T, CoreError> {
    funnel_error::request_failed_surface(ctx, None, surface, &error);
    Err(error)
}

fn action_label(action: &SurfaceAction) -> Option<&'static str> {
    match action {
        SurfaceAction::Forward(spec) | SurfaceAction::ForwardWebSocket(spec) => Some(spec.label),
        SurfaceAction::Synthesize { .. } => None,
    }
}
