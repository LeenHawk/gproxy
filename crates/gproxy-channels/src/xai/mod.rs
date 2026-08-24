mod model;
mod multipart;
mod prepare;
mod resource;
mod shape;
mod sse;
mod usage;

use gproxy_channel_api::{
    Channel, ChannelDescriptor, ChannelSupport, Disposition, NormalizedUsage, PrepareCtx,
    PreparedRequest, ResourceCtx, ResourceMutation, ResponseShapeCtx, ResponseView, StreamCtx,
    StreamDecoder, UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};

pub struct XaiChannel;

const fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

const fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

const fn openai(operation: Operation) -> OperationKey {
    family(operation, WireFamily::OpenAi)
}

static SUPPORTS: [ChannelSupport; 21] = [
    ChannelSupport::passthrough(openai(Operation::ListModels)),
    ChannelSupport::passthrough(openai(Operation::GetModel)),
    ChannelSupport::passthrough(content(
        Operation::GenerateContent,
        ContentGenerationKind::OpenAiChat,
    )),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiChat,
    )),
    ChannelSupport::passthrough(content(
        Operation::GenerateContent,
        ContentGenerationKind::OpenAiResponses,
    )),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiResponses,
    )),
    ChannelSupport::passthrough(openai(Operation::CreateImage)),
    ChannelSupport::passthrough(openai(Operation::EditImage)),
    ChannelSupport::passthrough(openai(Operation::CreateSpeech)),
    ChannelSupport::passthrough(openai(Operation::CreateTranscription)),
    ChannelSupport::passthrough(openai(Operation::CreateVideo)),
    ChannelSupport::passthrough(openai(Operation::RetrieveVideo)),
    ChannelSupport::passthrough(openai(Operation::EditVideo)),
    ChannelSupport::passthrough(openai(Operation::ExtendVideo)),
    ChannelSupport::passthrough(openai(Operation::CompactContent)),
    ChannelSupport::transform(
        family(Operation::ListModels, WireFamily::Claude),
        openai(Operation::ListModels),
    ),
    ChannelSupport::transform(
        family(Operation::GetModel, WireFamily::Claude),
        openai(Operation::GetModel),
    ),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
    ),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
    ),
];

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "xai",
    display_name: "xAI",
    supports: &SUPPORTS,
};

impl Channel for XaiChannel {
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
            401 => Disposition::CredentialDead,
            429 | 500..=599 => Disposition::Retryable,
            _ => Disposition::Terminal,
        }
    }

    fn stream_decoder(&self, ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
        sse::decoder(ctx)
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

    fn shape_response(
        &self,
        ctx: ResponseShapeCtx<'_>,
    ) -> Result<bytes::Bytes, gproxy_channel_api::ChannelError> {
        shape::response(ctx)
    }
}

#[cfg(test)]
mod tests;
