use std::time::Instant;

use crate::api::Core;
use crate::boundary::{ExecOutcome, RequestCtx};
use crate::control::{ControlPlane, Plan};
use crate::error::CoreError;
use crate::funnel_error;
use crate::host::Host;
use crate::request::Classified;

pub(crate) async fn run<H: Host>(
    core: &Core<H>,
    control: &impl ControlPlane,
    ctx: RequestCtx,
) -> Result<ExecOutcome, CoreError> {
    let started = Instant::now();
    let classified = match crate::request::classify(&ctx) {
        Ok(classified) => classified,
        Err(error) => return reject(&ctx, None, error),
    };
    let identity = match core.host.authenticate(&ctx).await {
        Ok(identity) => identity,
        Err(error) => return reject(&ctx, Some(classified.key), error),
    };
    let plan = match control.resolve(classified.model.as_deref(), &ctx.mode) {
        Ok(plan) => plan,
        Err(error) => return reject(&ctx, Some(classified.key), error),
    };
    resolved(core, control, ctx, plan, classified, identity, started).await
}

pub(crate) async fn planned<H: Host>(
    core: &Core<H>,
    control: &impl ControlPlane,
    ctx: RequestCtx,
    plan: Plan,
) -> Result<ExecOutcome, CoreError> {
    let started = Instant::now();
    let classified = match crate::request::classify(&ctx) {
        Ok(classified) => classified,
        Err(error) => return reject(&ctx, None, error),
    };
    let identity = match core.host.authenticate(&ctx).await {
        Ok(identity) => identity,
        Err(error) => return reject(&ctx, Some(classified.key), error),
    };
    resolved(core, control, ctx, plan, classified, identity, started).await
}

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
    let result =
        crate::failover::run(core, control, ctx, plan, classified, owner_user_id, started).await;
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
