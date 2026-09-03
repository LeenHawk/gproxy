use gproxy_channel_api::{StreamCtx, StreamDecoder};
use gproxy_protocol::{ContentGenerationKind, OperationKind};

pub(super) fn decoder(ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
    if ctx.key.kind() == OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) {
        crate::shared::claude::sse::ClaudeSseDecoder::for_operation(ctx)
            .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>)
    } else {
        crate::shared::openai::OpenAiSseDecoder::for_operation(ctx)
            .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>)
    }
}
