mod model;
mod prepare;
mod resource;
mod stream;
mod usage;

use gproxy_channel_api::{
    Channel, ChannelDescriptor, ChannelSupport, Disposition, NormalizedUsage, PrepareCtx,
    PreparedRequest, ResourceCtx, ResourceMutation, ResponseView, StreamCtx, StreamDecoder,
    UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};

pub struct AiStudioChannel;

const fn family(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::Gemini)
}

const fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

const fn gemini_content(operation: Operation) -> OperationKey {
    content(operation, ContentGenerationKind::GeminiGenerateContent)
}

static SUPPORTS: [ChannelSupport; 21] = [
    ChannelSupport::passthrough(family(Operation::ListModels)),
    ChannelSupport::passthrough(family(Operation::GetModel)),
    ChannelSupport::passthrough(family(Operation::CountTokens)),
    ChannelSupport::passthrough(gemini_content(Operation::GenerateContent)),
    ChannelSupport::passthrough(gemini_content(Operation::StreamGenerateContent)),
    ChannelSupport::passthrough(family(Operation::CreateEmbedding)),
    ChannelSupport::passthrough(family(Operation::BatchCreateEmbedding)),
    ChannelSupport::passthrough(family(Operation::CreateImage)),
    ChannelSupport::passthrough(family(Operation::CreateVideo)),
    ChannelSupport::passthrough(family(Operation::RetrieveVideo)),
    ChannelSupport::passthrough(family(Operation::CreateFile)),
    ChannelSupport::passthrough(family(Operation::ListFiles)),
    ChannelSupport::passthrough(family(Operation::RetrieveFile)),
    ChannelSupport::passthrough(family(Operation::RetrieveFileContent)),
    ChannelSupport::passthrough(family(Operation::DeleteFile)),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
        gemini_content(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        gemini_content(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        gemini_content(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
        gemini_content(Operation::StreamGenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        gemini_content(Operation::StreamGenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        gemini_content(Operation::StreamGenerateContent),
    ),
];

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "aistudio",
    display_name: "Google AI Studio",
    supports: &SUPPORTS,
};

impl Channel for AiStudioChannel {
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
        stream::GeminiStreamDecoder::for_operation(ctx)
            .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>)
    }

    fn extract_usage(&self, ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
        usage::from_body(ctx)
    }

    fn settlement_ready(
        &self,
        ctx: UsageCtx<'_>,
    ) -> Result<bool, gproxy_channel_api::ChannelError> {
        resource::settlement_ready(ctx)
    }

    fn resource_mutations(
        &self,
        ctx: ResourceCtx<'_>,
    ) -> Result<Vec<ResourceMutation>, gproxy_channel_api::ChannelError> {
        resource::mutations(ctx)
    }
}

#[cfg(test)]
mod tests;
