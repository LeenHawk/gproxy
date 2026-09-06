use gproxy_channel_api::{NormalizedUsage, UsageCtx};
use gproxy_protocol::{ContentGenerationKind, OperationKind};

pub(super) fn from_body(ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
    if matches!(
        ctx.key.operation(),
        gproxy_protocol::Operation::CreateEmbedding
            | gproxy_protocol::Operation::BatchCreateEmbedding
    ) {
        return super::embeddings::usage(ctx.response_body);
    }
    match ctx.key.kind() {
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat) => {
            crate::shared::openai::usage_from_body(ctx)
        }
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
        | OperationKind::Family(gproxy_protocol::WireFamily::Claude) => {
            crate::shared::claude::usage::from_body(ctx.response_body)
        }
        _ => crate::shared::gemini::usage::from_body(ctx),
    }
}
