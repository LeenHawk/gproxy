use web_time::Instant;

use crate::api::Core;
use crate::attempt::{self, AdmissionCtx, Failure};
use crate::boundary::{ExecOutcome, RequestCtx};
use crate::control::{ControlPlane, Target};
use crate::error::CoreError;
use crate::funnel::{self, error as funnel_error};
use crate::host::Host;

pub(crate) async fn run<H: Host>(
    core: &Core<H>,
    control: &dyn ControlPlane,
    target: &Target,
    ctx: RequestCtx,
) -> Result<ExecOutcome, CoreError> {
    let started = Instant::now();
    let classified = super::request::classify(&ctx)?;
    attempt::native_support(core, target, classified.key)?.ok_or(CoreError::Unsupported)?;
    core.host
        .admit_credential(
            &ctx.request_id,
            target,
            &ctx.body,
            classified.key.operation().spec().settle,
        )
        .await?;
    let prepared = attempt::prepare(
        core,
        control,
        target,
        &ctx,
        &classified,
        AdmissionCtx {
            admitted: false,
            owner_user_id: None,
        },
        started,
    )
    .await?;
    match attempt::send(core, prepared).await {
        Ok(completed) => Ok(attempt::finish(core, control, completed).await),
        Err(failure) => match *failure {
            Failure::Transport { facts, error } => {
                funnel_error::terminal_transport(core.host.as_ref(), &facts, &error).await;
                Err(error.into())
            }
            Failure::Interrupted {
                channel,
                facts,
                status,
                headers,
                body,
                error,
            } => {
                let channel = core
                    .channels
                    .get(channel)
                    .expect("attempt channel remains registered");
                funnel::interrupted(core.host.as_ref(), channel, facts, status, headers, body)
                    .await;
                Err(error.into())
            }
            Failure::Committed { error, .. } => Err(error),
        },
    }
}
