use std::time::Instant;

use crate::api::Core;
use crate::attempt::{self, Failure};
use crate::boundary::{ExecOutcome, RequestCtx};
use crate::control::{ControlPlane, Target};
use crate::error::CoreError;
use crate::host::Host;
use crate::{funnel, funnel_error};

pub(crate) async fn run<H: Host>(
    core: &Core<H>,
    control: &impl ControlPlane,
    target: &Target,
    ctx: RequestCtx,
) -> Result<ExecOutcome, CoreError> {
    let started = Instant::now();
    let classified = crate::request::classify(&ctx)?;
    if !attempt::supports(core, target, classified.key)? {
        return Err(CoreError::Unsupported);
    }
    let prepared =
        attempt::prepare(core, control, target, &ctx, &classified, false, started).await?;
    match attempt::send(core, prepared, &classified).await {
        Ok(completed) => Ok(attempt::finish(core, completed).await),
        Err(Failure::Transport { facts, error }) => {
            funnel_error::terminal_transport(core.host.as_ref(), &facts, &error).await;
            Err(error.into())
        }
        Err(Failure::Interrupted {
            channel,
            facts,
            status,
            body,
            error,
        }) => {
            let channel = core
                .channels
                .get(channel)
                .expect("attempt channel remains registered");
            funnel::interrupted(core.host.as_ref(), channel, facts, status, body).await;
            Err(error.into())
        }
    }
}
