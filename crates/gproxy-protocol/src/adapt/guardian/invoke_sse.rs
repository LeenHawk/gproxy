use super::{
    GuardianLimits, GuardianStreamContext, GuardianStreamInvocation, GuardianStreamLimits,
};
use crate::{
    capability::Upstream,
    transform::{TransformError, guardian::GuardianPreparedRequest, identity::IdentityFlow},
};
/// Run one prepared review request and return the client's native Responses SSE
/// body. Return controls are taken from the prepared client request.
pub async fn review_sse<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: GuardianPreparedRequest,
    context: GuardianStreamContext,
    flow: &mut IdentityFlow,
    limits: GuardianLimits,
    stream_limits: GuardianStreamLimits,
) -> Result<GuardianStreamInvocation, TransformError> {
    let context = source_context(&request, context)?;
    super::review(upstream, target, request, limits)
        .await?
        .into_stream(context, flow, stream_limits)
}
pub async fn classify_sse<U: Upstream>(
    upstream: &U,
    target: &U::Target,
    request: GuardianPreparedRequest,
    context: GuardianStreamContext,
    flow: &mut IdentityFlow,
    limits: GuardianLimits,
    stream_limits: GuardianStreamLimits,
) -> Result<GuardianStreamInvocation, TransformError> {
    let context = source_context(&request, context)?;
    super::classify(upstream, target, request, limits)
        .await?
        .into_stream(context, flow, stream_limits)
}
fn source_context(
    request: &GuardianPreparedRequest,
    context: GuardianStreamContext,
) -> Result<GuardianStreamContext, TransformError> {
    let source = request.response_request()?;
    Ok(match context {
        GuardianStreamContext::Responses => GuardianStreamContext::Responses,
        GuardianStreamContext::Chat(mut context) => {
            context.request = source;
            GuardianStreamContext::Chat(context)
        }
        GuardianStreamContext::Claude(mut context) => {
            context.request = source;
            GuardianStreamContext::Claude(context)
        }
        GuardianStreamContext::Gemini(mut context) => {
            context.request = source;
            GuardianStreamContext::Gemini(context)
        }
    })
}
