mod plan;
mod record;
mod stream;
mod transform;
mod usage;

pub use plan::{
    CountTokensPlan, DispatchPlan, GenerateContentPlan, ModelsGetPlan, ModelsListPlan,
    StreamContentPlan, TransformPlan, UsageKind,
};

use async_trait::async_trait;

use gproxy_provider_core::{
    CallContext, ProxyRequest, ProxyResponse, UpstreamPassthroughError, UpstreamRecordMeta,
};

use record::record_upstream_and_downstream;

pub struct UpstreamOk {
    pub response: ProxyResponse,
    pub meta: UpstreamRecordMeta,
}

#[async_trait]
pub trait DispatchProvider: Send + Sync {
    fn dispatch_plan(&self, req: ProxyRequest) -> DispatchPlan;

    async fn call_native(
        &self,
        req: ProxyRequest,
        ctx: CallContext,
    ) -> Result<UpstreamOk, UpstreamPassthroughError>;
}

pub async fn dispatch_request<P: DispatchProvider>(
    provider: &P,
    req: ProxyRequest,
    ctx: CallContext,
) -> Result<ProxyResponse, UpstreamPassthroughError> {
    match provider.dispatch_plan(req) {
        DispatchPlan::Native { req, usage } => dispatch_native(provider, req, usage, ctx).await,
        DispatchPlan::Transform { plan, usage } => {
            transform::dispatch_transform(provider, plan, usage, ctx).await
        }
    }
}

async fn dispatch_native<P: DispatchProvider>(
    provider: &P,
    req: ProxyRequest,
    usage: UsageKind,
    ctx: CallContext,
) -> Result<ProxyResponse, UpstreamPassthroughError> {
    let UpstreamOk { response, meta } = provider.call_native(req, ctx.clone()).await?;
    record_upstream_and_downstream(response, meta, usage, ctx).await
}
