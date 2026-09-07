use web_time::Instant;

use crate::api::Core;
use crate::boundary::{ExecOutcome, RequestCtx};
use crate::control::{ControlPlane, Plan};
use crate::error::CoreError;
use crate::funnel::error as funnel_error;
use crate::host::Host;

mod codex_models;
pub(crate) mod credential;
mod failover;
pub(crate) mod forwarding;
pub(crate) mod ingress;
pub(crate) mod invoke;
mod local;
mod local_models;
mod model_catalogue;
mod model_refresh;
pub(crate) mod preprocess;
pub(crate) mod request;
pub(crate) mod resource;
mod session;
mod synthetic;
mod websocket;

use self::request::Classified;

pub(super) struct AdmittedRequest {
    classified: Classified,
    owner_user_id: i64,
    session_affinity: Option<session::SessionAffinity>,
    started: Instant,
}

pub(crate) async fn resolved<H: Host>(
    core: &Core<H>,
    control: &dyn ControlPlane,
    ctx: RequestCtx,
    mut plan: Plan,
    classified: Classified,
    identity: gproxy_channel_api::CallerIdentity,
    started: Instant,
) -> Result<ExecOutcome, CoreError> {
    if ctx.upgrade && classified.key.operation() != gproxy_protocol::Operation::ConnectRealtime {
        return websocket::run(core, control, ctx, plan, classified, identity);
    }
    if classified.key.operation() == gproxy_protocol::Operation::ConnectRealtime {
        resource::restore_realtime_model(core, &mut plan, &classified, identity.user_id).await?;
    }
    let session_affinity =
        session::apply(core, &ctx, &classified, identity.user_key_id, &mut plan).await;
    let plan = match core
        .host
        .admit(&identity, &ctx, Some(classified.key), &plan)
        .await
    {
        Ok(plan) => plan,
        Err(error) => return reject(&ctx, Some(classified.key), error),
    };
    execute_admitted(
        core,
        control,
        ctx,
        plan,
        AdmittedRequest {
            classified,
            owner_user_id: identity.user_id,
            session_affinity,
            started,
        },
        identity,
    )
    .await
}

async fn execute_admitted<H: Host>(
    core: &Core<H>,
    control: &dyn ControlPlane,
    ctx: RequestCtx,
    plan: Plan,
    request: AdmittedRequest,
    identity: gproxy_channel_api::CallerIdentity,
) -> Result<ExecOutcome, CoreError> {
    if let Some(synthetic) = synthetic::plan(core, control, &plan, &request.classified) {
        return Ok(synthetic::run(
            core, synthetic, ctx, plan, request, identity,
        ));
    }
    upstream(core, control, ctx, plan, request, identity).await
}

async fn upstream<H: Host>(
    core: &Core<H>,
    control: &dyn ControlPlane,
    ctx: RequestCtx,
    plan: Plan,
    request: AdmittedRequest,
    identity: gproxy_channel_api::CallerIdentity,
) -> Result<ExecOutcome, CoreError> {
    let telemetry_ctx = ctx.clone();
    let key = request.classified.key;
    let result = match local::run(
        core,
        control,
        &ctx,
        &plan,
        &request.classified,
        &identity,
        request.started,
    )
    .await
    {
        Some(result) => result,
        None => failover::run(core, control, ctx, plan, request).await,
    };
    if let Err(error) = &result {
        core.host
            .finish_admission(&telemetry_ctx.request_id, None)
            .await;
        funnel_error::request_failed(&telemetry_ctx, Some(key), error);
    }
    result.and_then(|outcome| codex_models::render(&telemetry_ctx.headers, key, outcome))
}

fn reject<T>(
    ctx: &RequestCtx,
    key: Option<gproxy_protocol::OperationKey>,
    error: CoreError,
) -> Result<T, CoreError> {
    funnel_error::request_failed(ctx, key, &error);
    Err(error)
}
