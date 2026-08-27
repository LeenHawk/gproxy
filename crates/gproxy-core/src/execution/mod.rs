use web_time::Instant;

use crate::api::Core;
use crate::boundary::{ExecOutcome, RequestCtx};
use crate::control::{ControlPlane, Plan};
use crate::error::CoreError;
use crate::funnel::error as funnel_error;
use crate::host::Host;

pub(crate) mod credential;
mod failover;
pub(crate) mod ingress;
pub(crate) mod invoke;
mod local;
pub(crate) mod preprocess;
pub(crate) mod request;
pub(crate) mod resource;

use self::request::Classified;

pub(crate) async fn resolved<H: Host>(
    core: &Core<H>,
    control: &impl ControlPlane,
    ctx: RequestCtx,
    plan: Plan,
    classified: Classified,
    identity: gproxy_channel_api::CallerIdentity,
    started: Instant,
) -> Result<ExecOutcome, CoreError> {
    if let Err(error) = core
        .host
        .admit(&identity, &ctx, Some(classified.key), &plan)
        .await
    {
        return reject(&ctx, Some(classified.key), error);
    }
    execute_admitted(
        core,
        control,
        ctx,
        plan,
        classified,
        identity.user_id,
        started,
    )
    .await
}

async fn execute_admitted<H: Host>(
    core: &Core<H>,
    control: &impl ControlPlane,
    ctx: RequestCtx,
    plan: Plan,
    classified: Classified,
    owner_user_id: i64,
    started: Instant,
) -> Result<ExecOutcome, CoreError> {
    let telemetry_ctx = ctx.clone();
    let key = classified.key;
    let result = match local::run(core, control, &ctx, &plan, &classified, started).await {
        Some(result) => result,
        None => failover::run(core, control, ctx, plan, classified, owner_user_id, started).await,
    };
    if let Err(error) = &result {
        core.host
            .finish_admission(&telemetry_ctx.request_id, None)
            .await;
        funnel_error::request_failed(&telemetry_ctx, Some(key), error);
    }
    result
}

fn reject<T>(
    ctx: &RequestCtx,
    key: Option<gproxy_protocol::OperationKey>,
    error: CoreError,
) -> Result<T, CoreError> {
    funnel_error::request_failed(ctx, key, &error);
    Err(error)
}
