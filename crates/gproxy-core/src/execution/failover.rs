use std::collections::BTreeSet;

use gproxy_channel_api::{ChannelError, Disposition};

use crate::api::Core;
use crate::attempt::{self, AdmissionCtx, Failure};
use crate::boundary::{ExecOutcome, RequestCtx};
use crate::control::{ControlPlane, Plan};
use crate::error::CoreError;
use crate::funnel::error as funnel_error;
use crate::host::Host;

use super::AdmittedRequest;

pub(crate) async fn run<H: Host>(
    core: &Core<H>,
    control: &dyn ControlPlane,
    ctx: RequestCtx,
    plan: Plan,
    request: AdmittedRequest,
) -> Result<ExecOutcome, CoreError> {
    if plan.targets.is_empty() {
        return Err(CoreError::NoCredentials);
    }
    if plan.budget.max_attempts == 0 {
        return Err(CoreError::UpstreamExhausted(
            "attempt budget is zero".into(),
        ));
    }
    let resource_pins =
        super::resource::pins(core, &plan, &request.classified, request.owner_user_id).await?;

    let mut attempts = 0;
    let mut supported = false;
    let mut selected = false;
    let mut dead = BTreeSet::new();
    let mut last_reason = None;
    let mut pre_send_error = None;
    for target in &plan.targets {
        if attempts >= plan.budget.max_attempts {
            break;
        }
        if dead.contains(&target.credential) {
            continue;
        }
        if resource_pins
            .as_ref()
            .is_some_and(|pins| pins.get(&target.provider.id) != Some(&target.credential))
        {
            continue;
        }
        let Some(support) = attempt::support(core, target, request.classified.key)? else {
            continue;
        };
        if support.source != support.target
            && !gproxy_transform::can_transform(support.source, support.target)
        {
            continue;
        }
        supported = true;
        if let Err(error) = core
            .host
            .admit_credential(
                &ctx.request_id,
                target,
                &ctx.body,
                request.classified.key.operation().spec().settle,
            )
            .await
        {
            if matches!(
                error,
                CoreError::RateLimited { .. } | CoreError::QuotaExceeded
            ) {
                let reason = "credential admission limit reached";
                last_reason = Some(reason);
                funnel_error::pre_send(&ctx, target, request.classified.key, reason);
                pre_send_error = Some(error);
                continue;
            }
            return Err(error);
        }
        let admission = AdmissionCtx {
            admitted: true,
            owner_user_id: Some(request.owner_user_id),
        };
        let mut prepared = match attempt::prepare(
            core,
            control,
            target,
            &ctx,
            &request.classified,
            admission,
            request.started,
        )
        .await
        {
            Ok(prepared) => prepared,
            Err(CoreError::Channel(ChannelError::Secret(_))) => {
                dead.insert(target.credential);
                let reason = "credential secret rejected";
                last_reason = Some(reason);
                funnel_error::pre_send(&ctx, target, request.classified.key, reason);
                continue;
            }
            Err(CoreError::Channel(ChannelError::Refresh(_))) => {
                dead.insert(target.credential);
                let reason = "credential refresh failed";
                last_reason = Some(reason);
                funnel_error::pre_send(&ctx, target, request.classified.key, reason);
                continue;
            }
            Err(error @ CoreError::Channel(ChannelError::Prepare(_))) => {
                let reason = "channel prepare failed";
                last_reason = Some(reason);
                funnel_error::pre_send(&ctx, target, request.classified.key, reason);
                pre_send_error = Some(error);
                continue;
            }
            Err(CoreError::Unsupported) => continue,
            Err(error) => return Err(error),
        };
        if let Some(refusal) = prepared.refusal.as_mut() {
            refusal.budget = plan.budget.max_attempts - attempts;
        }
        selected = true;
        attempts += 1;
        match attempt::send(core, prepared).await {
            Ok(completed) => {
                let disposition = completed.disposition;
                if !disposition.should_failover() {
                    if disposition == Disposition::Success
                        && let Some(affinity) = &request.session_affinity
                    {
                        affinity.commit(core, &completed.facts.target).await;
                    }
                    return Ok(attempt::finish(core, control, completed).await);
                }
                if disposition == Disposition::CredentialDead {
                    dead.insert(completed.facts.target.credential);
                }
                last_reason = Some(match disposition {
                    Disposition::Retryable => "retryable upstream response",
                    Disposition::CredentialDead => "credential rejected upstream",
                    Disposition::Success | Disposition::Terminal => "unexpected disposition",
                });
                let (facts, status, body) = attempt::discard(completed);
                funnel_error::attempt_response(
                    core.host.as_ref(),
                    &facts,
                    status,
                    body,
                    disposition,
                )
                .await;
            }
            Err(failure) => match *failure {
                Failure::Transport { facts, error } => {
                    let reason = funnel_error::transport_error_kind(&error);
                    last_reason = Some(reason);
                    funnel_error::attempt_transport(core.host.as_ref(), &facts, &error).await;
                }
                Failure::Interrupted {
                    facts,
                    status,
                    body,
                    error,
                    ..
                } => {
                    last_reason = Some("upstream response interrupted");
                    funnel_error::attempt_interrupted(
                        core.host.as_ref(),
                        &facts,
                        status,
                        body,
                        &error,
                    )
                    .await;
                }
                Failure::Committed { error, .. } => return Err(error),
            },
        }
    }

    if !supported {
        Err(CoreError::Transform(
            "passthrough is unavailable for every plan target".into(),
        ))
    } else if !selected && pre_send_error.is_none() {
        Err(CoreError::Unsupported)
    } else if attempts == 0 {
        Err(pre_send_error.unwrap_or(CoreError::NoCredentials))
    } else {
        Err(CoreError::UpstreamExhausted(format!(
            "all candidates exhausted after {attempts} upstream attempt(s); last failure: {}",
            last_reason.unwrap_or("no upstream attempt")
        )))
    }
}
