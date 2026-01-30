mod plan;
mod record;
mod stream;
mod transform;
mod usage;

pub use plan::{
    CountTokensPlan, DispatchPlan, DispatchTable, GenerateContentPlan, ModelsGetPlan,
    ModelsListPlan, OpMode, OpSpec, OperationKind, StreamContentPlan, TransformPlan,
    TransformTarget, UsageKind, dispatch_plan_from_table, native_spec, transform_spec,
    unsupported_spec,
};

use async_trait::async_trait;

use gproxy_provider_core::{
    DownstreamContext, ProxyRequest, ProxyResponse, UpstreamContext, UpstreamPassthroughError,
    UpstreamRecordMeta,
};

use record::record_upstream_and_downstream;

pub struct UpstreamOk {
    pub response: ProxyResponse,
    pub meta: UpstreamRecordMeta,
}

#[async_trait]
pub trait DispatchProvider: Send + Sync {
    fn dispatch_table(&self) -> &'static DispatchTable;

    async fn call_native(
        &self,
        req: ProxyRequest,
        ctx: UpstreamContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError>;

    fn dispatch_plan(&self, req: ProxyRequest) -> DispatchPlan {
        dispatch_plan_from_table(req, self.dispatch_table())
    }
}

pub async fn dispatch_request<P: DispatchProvider>(
    provider: &P,
    req: ProxyRequest,
    ctx: DownstreamContext,
) -> Result<ProxyResponse, UpstreamPassthroughError> {
    match provider.dispatch_plan(req) {
        DispatchPlan::Native { req, usage } => {
            dispatch_native(provider, req, usage, ctx).await
        }
        DispatchPlan::Transform { plan, usage } => {
            transform::dispatch_transform(provider, plan, usage, ctx).await
        }
        DispatchPlan::Unsupported { reason } => Err(UpstreamPassthroughError::service_unavailable(
            reason.to_string(),
        )),
    }
}

async fn dispatch_native<P: DispatchProvider>(
    provider: &P,
    req: ProxyRequest,
    usage: UsageKind,
    ctx: DownstreamContext,
) -> Result<ProxyResponse, UpstreamPassthroughError> {
    let UpstreamOk { response, meta } = provider.call_native(req, ctx.upstream()).await?;
    record_upstream_and_downstream(response, meta, usage, ctx).await
}
