mod model;
mod prepare;
mod shape;
mod sse;
mod usage;

use gproxy_channel_api::{
    Channel, ChannelDescriptor, ChannelSupport, Disposition, NormalizedUsage, PrepareCtx,
    PreparedRequest, ResponseShapeCtx, ResponseView, StreamCtx, StreamDecoder, UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};

pub struct VertexExpressChannel;

const fn family(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::Gemini)
}

const fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

const fn gemini(operation: Operation) -> OperationKey {
    content(operation, ContentGenerationKind::GeminiGenerateContent)
}

static SUPPORTS: [ChannelSupport; 9] = [
    ChannelSupport::passthrough(family(Operation::CountTokens)),
    ChannelSupport::passthrough(gemini(Operation::GenerateContent)),
    ChannelSupport::passthrough(gemini(Operation::StreamGenerateContent)),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
        gemini(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
        gemini(Operation::StreamGenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        gemini(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        gemini(Operation::StreamGenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        gemini(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        gemini(Operation::StreamGenerateContent),
    ),
];

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "vertexexpress",
    display_name: "Google Vertex AI Express",
    supports: &SUPPORTS,
};

impl Channel for VertexExpressChannel {
    fn descriptor(&self) -> &'static ChannelDescriptor {
        &DESCRIPTOR
    }

    fn prepare(
        &self,
        ctx: PrepareCtx<'_>,
    ) -> Result<PreparedRequest, gproxy_channel_api::ChannelError> {
        prepare::request(ctx)
    }

    fn classify(&self, response: ResponseView<'_>) -> Disposition {
        match response.status.as_u16() {
            200..=299 => Disposition::Success,
            401..=403 => Disposition::CredentialDead,
            429 | 500..=599 => Disposition::Retryable,
            _ => Disposition::Terminal,
        }
    }

    fn stream_decoder(&self, ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
        sse::GeminiStreamDecoder::for_operation(ctx)
            .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>)
    }

    fn extract_usage(&self, ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
        usage::from_body(ctx)
    }

    fn shape_response(
        &self,
        ctx: ResponseShapeCtx<'_>,
    ) -> Result<bytes::Bytes, gproxy_channel_api::ChannelError> {
        if ctx.status.is_success() && model::is_gemini_content(&ctx) {
            crate::shared::gemini::vertex::normalize_content(ctx.body)
        } else {
            Ok(ctx.body.clone())
        }
    }
}

#[cfg(test)]
mod tests;
