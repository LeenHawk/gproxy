use web_time::Instant;

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
    Continue {
        ctx: Box<RequestCtx>,
        classified: crate::execution::request::Classified,
        identity: gproxy_channel_api::CallerIdentity,
        plan: Plan,
        started: Instant,
    },
    Outcome(Result<ExecOutcome, CoreError>),
}

pub(crate) async fn dispatch<H: Host>(
    core: &Core<H>,
    control: &dyn ControlPlane,
    ctx: RequestCtx,
    planned: Option<&Plan>,
    classified: Result<crate::execution::request::Classified, CoreError>,
) -> Dispatch {
    let matches = affinity::table_matches(core, &ctx);
    run(core, control, ctx, planned, classified, matches).await
}

async fn run<H: Host>(
    core: &Core<H>,
    control: &dyn ControlPlane,
    mut ctx: RequestCtx,
    planned: Option<&Plan>,
    mut classified: Result<crate::execution::request::Classified, CoreError>,
    matches: Vec<affinity::TableMatch>,
) -> Dispatch {
    let started = Instant::now();
    let mut alias_request = match operation_alias_request(&ctx, &matches) {
        Ok(alias) => alias,
        Err(error) => return Dispatch::Outcome(reject(&ctx, None, error)),
    };
    if matches.is_empty()
        && let Ok(classified) = classified.as_mut()
        && let Err(error) = crate::execution::preprocess::apply(control, &mut ctx, classified)
    {
        return Dispatch::Outcome(reject(&ctx, None, error));
    }
    if let Some((request, classified)) = alias_request.as_mut()
        && let Err(error) = crate::execution::preprocess::apply(control, request, classified)
    {
        return Dispatch::Outcome(reject(&ctx, None, error));
    }
    let matched_label = matches
        .first()
        .and_then(|matched| action_label(&matched.entry.action));
    let bearer_auth = matches.iter().any(|matched| {
        matches!(
            matched.entry.affinity,
            gproxy_channel_api::SurfaceAffinity::BearerToken { .. }
        )
    });
    let public = matches
        .iter()
        .any(|matched| matches!(matched.entry.action, SurfaceAction::PublicSynthesize { .. }));
    let resolve = |affinity| {
        planned.cloned().map_or_else(
            || {
                let model = alias_request
                    .as_ref()
                    .and_then(|(_, classified)| classified.model.as_deref())
                    .or_else(|| {
                        matches
                            .is_empty()
                            .then(|| classified.as_ref().ok()?.model.as_deref())
                            .flatten()
                    });
                control.resolve_preprocessed(model, &ctx.mode, affinity)
            },
            Ok,
        )
    };
    let (identity, mut plan) = if public {
        let plan = match resolve(None) {
            Ok(plan) => plan,
            Err(error) => return Dispatch::Outcome(reject(&ctx, matched_label, error)),
        };
        (
            gproxy_channel_api::CallerIdentity {
                oauth_access_digest: None,
                user_id: 0,
                user_key_id: 0,
                org_id: None,
                team_id: None,
            },
            plan,
        )
    } else if bearer_auth {
        let plan = match resolve(None) {
            Ok(plan) => plan,
            Err(error) => return Dispatch::Outcome(reject(&ctx, matched_label, error)),
        };
        let identity = match affinity::bearer_identity(core, &ctx, &plan, &matches).await {
            Ok(Some(identity)) => identity,
            Ok(None) => {
                return Dispatch::Outcome(reject(&ctx, matched_label, CoreError::Unauthorized));
            }
            Err(error) => return Dispatch::Outcome(reject(&ctx, matched_label, error)),
        };
        (identity, plan)
    } else {
        let identity = match core.host.authenticate(&ctx).await {
            Ok(identity) => identity,
            Err(error) => return Dispatch::Outcome(reject(&ctx, matched_label, error)),
        };
        let classified_affinity = alias_request
            .as_ref()
            .map(|(_, classified)| classified)
            .or_else(|| classified.as_ref().ok());
        let affinity = classified_affinity
            .map(|classified| classified.routing_affinity(identity.user_key_id))
            .unwrap_or(identity.user_key_id);
        let plan = match resolve(Some(affinity)) {
            Ok(plan) => plan,
            Err(error) => return Dispatch::Outcome(reject(&ctx, matched_label, error)),
        };
        (identity, plan)
    };
    let serves_surface = plan.targets.iter().any(|target| {
        matches
            .iter()
            .any(|matched| matched.channel == target.provider.channel)
    });
    enum Route {
        Continue(crate::execution::request::Classified),
        Alias {
            plan: Plan,
            classified: crate::execution::request::Classified,
        },
        Surface {
            selected: Box<Selected>,
            affinity: gproxy_channel_api::SurfaceAffinity,
            pin: Option<pin::AffinityPin>,
            label: Option<&'static str>,
        },
    }
    let (mut ctx, route) = if !serves_surface {
        let classified = match classified {
            Ok(classified) => classified,
            Err(error) => return Dispatch::Outcome(reject(&ctx, None, error)),
        };
        (ctx, Route::Continue(classified))
    } else if let Some(alias_plan) = operation_alias_plan(&matches, &plan)
        && let Some((request, classified)) = alias_request.take()
    {
        (
            request,
            Route::Alias {
                plan: alias_plan,
                classified,
            },
        )
    } else {
        if !public {
            plan = match core.host.admit(&identity, &ctx, None, &plan).await {
                Ok(plan) => plan,
                Err(error) => return Dispatch::Outcome(reject(&ctx, matched_label, error)),
            };
        }
        let mut selected = match affinity::select(core, &ctx, &identity, &plan, matches).await {
            Ok(selected) => selected,
            Err(error) => {
                core.host.finish_admission(&ctx.request_id, None).await;
                funnel_error::request_failed_surface(&ctx, None, matched_label, &error);
                return Dispatch::Outcome(Err(error));
            }
        };
        let label = action_label(&selected.entry.action);
        let affinity = selected.entry.affinity;
        let pin = selected.pin.take();
        (
            ctx,
            Route::Surface {
                selected: Box::new(selected),
                affinity,
                pin,
                label,
            },
        )
    };
    if !public && !ctx.upgrade {
        crate::execution::ingress::strip(&mut ctx);
    }
    let (selected, affinity, pin, surface_label) = match route {
        Route::Continue(classified) => {
            return Dispatch::Continue {
                ctx: Box::new(ctx),
                classified,
                identity,
                plan,
                started,
            };
        }
        Route::Alias { plan, classified } => {
            return Dispatch::Outcome(
                crate::execution::resolved(core, control, ctx, plan, classified, identity, started)
                    .await,
            );
        }
        Route::Surface {
            selected,
            affinity,
            pin,
            label,
        } => (*selected, affinity, pin, label),
    };
    let result = action(core, control, &ctx, &plan, &identity, selected, started).await;
    if let Ok((outcome, winner)) = &result
        && outcome.disposition == Disposition::Success
    {
        let response_pins = pin::response_pins(affinity, &identity, winner, outcome);
        let mut committed = Ok(());
        if let Some(pin) = pin {
            committed = pin::commit(core, pin, winner).await;
        }
        for response_pin in response_pins {
            if committed.is_ok() {
                committed = pin::commit(core, response_pin, winner).await;
            }
        }
        if let Err(error) = committed {
            tracing::error!(
                request_id = %ctx.request_id,
                error = %error,
                "surface affinity commit failed"
            );
            return Dispatch::Outcome(Err(error));
        }
    }
    if let Err(error) = &result {
        core.host.finish_admission(&ctx.request_id, None).await;
        funnel_error::request_failed_surface(&ctx, None, surface_label, error);
    }
    Dispatch::Outcome(result.map(|(outcome, _)| outcome))
}

async fn action<H: Host>(
    core: &Core<H>,
    control: &dyn ControlPlane,
    ctx: &RequestCtx,
    plan: &Plan,
    identity: &gproxy_channel_api::CallerIdentity,
    selected: Selected,
    started: Instant,
) -> Result<(ExecOutcome, crate::control::Target), CoreError> {
    match &selected.entry.action {
        SurfaceAction::Forward(_) | SurfaceAction::ForwardWebSocket(_) => {
            forward::declared(core, &selected, ctx, plan.budget, started).await
        }
        SurfaceAction::OperationAlias { .. } => Err(CoreError::Internal(
            "operation alias reached the surface action engine".into(),
        )),
        SurfaceAction::Synthesize { .. } | SurfaceAction::PublicSynthesize { .. } => {
            let target = selected.target.clone();
            synth::run(core, control, ctx, plan, identity, selected, started)
                .await
                .map(|outcome| (outcome, target))
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
        SurfaceAction::OperationAlias { .. }
        | SurfaceAction::Synthesize { .. }
        | SurfaceAction::PublicSynthesize { .. } => None,
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
