use gproxy_channel_api::{StreamCtx, StreamDecoder};
use gproxy_protocol::{ContentGenerationKind, OperationKind};

pub(super) fn decoder(ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
    match ctx.key.kind() {
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) => {
            crate::shared::claude::sse::ClaudeSseDecoder::for_operation(ctx)
                .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>)
        }
        OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent) => {
            crate::shared::gemini::stream::GeminiStreamDecoder::for_operation(ctx)
                .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>)
        }
        OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat) => {
            crate::shared::openai::OpenAiSseDecoder::for_operation(ctx)
                .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>)
        }
        OperationKind::ContentGeneration(
            ContentGenerationKind::OpenAiResponses
            | ContentGenerationKind::OpenAiResponsesWebSocket,
        )
        | OperationKind::Family(_) => None,
    }
}
