use gproxy_channel_api::{StreamCtx, StreamDecoder};
use gproxy_protocol::{ContentGenerationKind, OperationKind, WireFamily};

pub(super) fn decoder(ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
    match ctx.key.kind {
        OperationKind::ContentGeneration(
            ContentGenerationKind::OpenAiChat | ContentGenerationKind::OpenAiResponses,
        ) => crate::shared::openai::OpenAiSseDecoder::for_operation(ctx)
            .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>),
        OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) => {
            crate::shared::claude::sse::ClaudeSseDecoder::for_operation(ctx)
                .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>)
        }
        OperationKind::ContentGeneration(
            ContentGenerationKind::OpenAiResponsesWebSocket
            | ContentGenerationKind::GeminiGenerateContent,
        )
        | OperationKind::Family(WireFamily::OpenAi | WireFamily::Claude | WireFamily::Gemini) => {
            None
        }
    }
}
