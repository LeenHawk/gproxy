use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind, OperationKey, OperationKind, WireFamily};

pub(super) fn is_openai(key: OperationKey) -> bool {
    matches!(
        key.kind(),
        OperationKind::Family(WireFamily::OpenAi)
            | OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat)
    )
}

pub(super) fn rewrite(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    if is_openai(ctx.key) {
        crate::shared::openai::shape_request(
            ctx.key,
            ctx.stream,
            ctx.upstream_model,
            ctx.headers,
            ctx.body,
        )
    } else {
        crate::shared::gemini::model::rewrite(ctx.key.operation(), ctx.body, ctx.upstream_model)
    }
}
