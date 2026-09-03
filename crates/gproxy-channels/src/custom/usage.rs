use gproxy_channel_api::{NormalizedUsage, UsageCtx};
use gproxy_protocol::{ContentGenerationKind, OperationKind, WireFamily};

pub(super) fn from_body(ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
    match ctx.key.kind() {
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
        | OperationKind::Family(WireFamily::Claude) => {
            crate::shared::claude::usage::from_body(ctx.response_body)
        }
        OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent)
        | OperationKind::Family(WireFamily::Gemini) => crate::shared::gemini::usage::from_body(ctx),
        _ => crate::shared::openai::usage::from_body(ctx),
    }
}
