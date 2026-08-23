use std::time::Instant;

use gproxy_channel_api::{Disposition, SurfaceAction};

use crate::api::Core;
use crate::boundary::{ExecOutcome, RequestCtx};
use crate::control::{ControlPlane, Plan};
use crate::error::CoreError;
use crate::funnel::error as funnel_error;
use crate::host::Host;

mod affinity;
mod forward;
mod invoke;
mod pin;
mod reply;
mod synth;
mod template;

use self::affinity::Selected;

pub(crate) enum Dispatch {
    Unmatched,
    Continue {
        identity: gproxy_channel_api::CallerIdentity,
        plan: Plan,
        started: Instant,
    },
    Outcome(Result<ExecOutcome, CoreError>),
}

pub(crate) async fn dispatch<H: Host>(
    core: &Core<H>,
    control: &impl ControlPlane,
    ctx: &RequestCtx,
    planned: Option<&Plan>,
) -> Dispatch {
    let matches = affinity::table_matches(core, ctx);
    if matches.is_empty() {
        return Dispatch::Unmatched;
    }
    run(core, control, ctx, planned, matches).await
}

async fn run<H: Host>(
    core: &Core<H>,
    control: &impl ControlPlane,
    ctx: &RequestCtx,
    planned: Option<&Plan>,
    matches: Vec<affinity::TableMatch>,
) -> Dispatch {
    let started = Instant::now();
    let alias_request = match operation_alias_request(ctx, &matches) {
        Ok(alias) => alias,
        Err(error) => return Dispatch::Outcome(reject(ctx, None, error)),
    };
    let matched_label = matches
        .first()
        .and_then(|matched| action_label(&matched.entry.action));
    let bearer_auth = matches.iter().any(|matched| {
        matches!(
            matched.entry.affinity,
            gproxy_channel_api::SurfaceAffinity::BearerToken { .. }
        )
    });
    let resolve = || match planned {
        Some(plan) => Ok(plan.clone()),
        None => control.resolve(
            alias_request
                .as_ref()
                .and_then(|(_, classified)| classified.model.as_deref()),
            &ctx.mode,
        ),
    };
    let (identity, plan) = if bearer_auth {
        let plan = match resolve() {
            Ok(plan) => plan,
            Err(error) => return Dispatch::Outcome(reject(ctx, matched_label, error)),
        };
        let identity = match affinity::bearer_identity(core, ctx, &plan, &matches).await {
            Ok(Some(identity)) => identity,
            Ok(None) => {
                return Dispatch::Outcome(reject(ctx, matched_label, CoreError::Unauthorized));
            }
            Err(error) => return Dispatch::Outcome(reject(ctx, matched_label, error)),
        };
        (identity, plan)
    } else {
        let identity = match core.host.authenticate(ctx).await {
            Ok(identity) => identity,
            Err(error) => return Dispatch::Outcome(reject(ctx, matched_label, error)),
        };
        let plan = match resolve() {
            Ok(plan) => plan,
            Err(error) => return Dispatch::Outcome(reject(ctx, matched_label, error)),
        };
        (identity, plan)
    };
    let serves_surface = plan.targets.iter().any(|target| {
        matches
            .iter()
            .any(|matched| matched.channel == target.provider.channel)
    });
    if !serves_surface {
        return Dispatch::Continue {
            identity,
            plan,
            started,
        };
    }
    if let Some((request, classified)) = alias_request
        && let Some(plan) = operation_alias_plan(&matches, &plan)
    {
        return Dispatch::Outcome(
            crate::execution::resolved(core, control, request, plan, classified, identity, started)
                .await,
        );
    }
    if let Err(error) = core.host.admit(&identity, ctx, None, &plan).await {
        return Dispatch::Outcome(reject(ctx, matched_label, error));
    }
    let mut selected = match affinity::select(core, ctx, &identity, &plan, matches).await {
        Ok(selected) => selected,
        Err(error) => {
            core.host.finish_admission(&ctx.request_id, None).await;
            funnel_error::request_failed_surface(ctx, None, matched_label, &error);
            return Dispatch::Outcome(Err(error));
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
            pin::commit(core, pin).await;
        }
        if let Ok(outcome) = &result {
            for pin in pin::response_pins(affinity, &identity, &pin_target, outcome) {
                pin::commit(core, pin).await;
            }
        }
    }
    if let Err(error) = &result {
        core.host.finish_admission(&ctx.request_id, None).await;
        funnel_error::request_failed_surface(ctx, None, surface_label, error);
    }
    Dispatch::Outcome(result)
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
            forward::declared(core, &selected, ctx, started).await
        }
        SurfaceAction::OperationAlias { .. } => Err(CoreError::Internal(
            "operation alias reached the surface action engine".into(),
        )),
        SurfaceAction::Synthesize { .. } => {
            synth::run(core, control, ctx, plan, identity, selected, started).await
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
        SurfaceAction::OperationAlias { .. } | SurfaceAction::Synthesize { .. } => None,
    }
}

fn operation_alias_request(
    ctx: &RequestCtx,
    matches: &[affinity::TableMatch],
) -> Result<Option<(RequestCtx, crate::execution::request::Classified)>, CoreError> {
    let Some(canonical_path) = matches.iter().find_map(|matched| {
        let SurfaceAction::OperationAlias { canonical_path } = &matched.entry.action else {
            return None;
        };
        Some(*canonical_path)
    }) else {
        return Ok(None);
    };
    let mut request = ctx.clone();
    request.path = canonical_path.into();
    let classified = crate::execution::request::classify(&request)?;
    Ok(Some((request, classified)))
}

fn operation_alias_plan(matches: &[affinity::TableMatch], plan: &Plan) -> Option<Plan> {
    for target in &plan.targets {
        let Some(matched) = matches
            .iter()
            .find(|matched| matched.channel == target.provider.channel)
        else {
            continue;
        };
        let SurfaceAction::OperationAlias { .. } = &matched.entry.action else {
            return None;
        };
        let targets = plan
            .targets
            .iter()
            .filter(|candidate| candidate.provider.id == target.provider.id)
            .cloned()
            .collect();
        return Some(Plan {
            targets,
            budget: plan.budget,
        });
    }
    None
}
