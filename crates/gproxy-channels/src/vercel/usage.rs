use gproxy_channel_api::{NormalizedUsage, UsageCtx};
use gproxy_protocol::{ContentGenerationKind, OperationKind};

pub(super) fn from_body(ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
    if ctx.key.kind == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) {
        crate::shared::claude::usage::from_body(ctx.response_body)
    } else {
        crate::shared::openai::usage_from_body(ctx)
    }
}
