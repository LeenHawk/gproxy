mod routes;

mod model;
mod prepare;
mod resource;
mod sse;
mod usage;

use gproxy_channel_api::{
    Channel, ChannelDescriptor, ChannelSupport, Disposition, NormalizedUsage, PrepareCtx,
    PreparedRequest, ResourceCtx, ResourceMutation, ResponseView, StreamCtx, StreamDecoder,
    UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};

pub struct OpenAiChannel;

const fn family(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::OpenAi)
}

const fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

static SUPPORTS: [ChannelSupport; 35] = [
    ChannelSupport::passthrough(family(Operation::ListModels)),
    ChannelSupport::passthrough(family(Operation::GetModel)),
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
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiResponsesWebSocket,
    )),
    ChannelSupport::passthrough(family(Operation::CompactContent)),
    ChannelSupport::passthrough(family(Operation::CreateEmbedding)),
    ChannelSupport::passthrough(family(Operation::CreateImage)),
    ChannelSupport::passthrough(family(Operation::EditImage)),
    ChannelSupport::passthrough(family(Operation::CreateSpeech)),
    ChannelSupport::passthrough(family(Operation::CreateTranscription)),
    ChannelSupport::passthrough(family(Operation::CreateTranslation)),
    ChannelSupport::passthrough(family(Operation::CreateFile)),
    ChannelSupport::passthrough(family(Operation::ListFiles)),
    ChannelSupport::passthrough(family(Operation::RetrieveFile)),
    ChannelSupport::passthrough(family(Operation::RetrieveFileContent)),
    ChannelSupport::passthrough(family(Operation::DeleteFile)),
    ChannelSupport::passthrough(family(Operation::CreateVideo)),
    ChannelSupport::passthrough(family(Operation::RetrieveVideo)),
    ChannelSupport::passthrough(family(Operation::ListVideos)),
    ChannelSupport::passthrough(family(Operation::DeleteVideo)),
    ChannelSupport::passthrough(family(Operation::DownloadVideoContent)),
    ChannelSupport::passthrough(family(Operation::RemixVideo)),
    ChannelSupport::passthrough(family(Operation::CreateVideoCharacter)),
    ChannelSupport::passthrough(family(Operation::GetVideoCharacter)),
    ChannelSupport::passthrough(family(Operation::EditVideo)),
    ChannelSupport::passthrough(family(Operation::ExtendVideo)),
    ChannelSupport::transform(
        OperationKey::family(Operation::ListModels, WireFamily::Claude),
        family(Operation::ListModels),
    ),
    ChannelSupport::transform(
        OperationKey::family(Operation::GetModel, WireFamily::Claude),
        family(Operation::GetModel),
    ),
    ChannelSupport::transform(
        OperationKey::content(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
    ),
    ChannelSupport::transform(
        OperationKey::content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiChat,
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
    id: "openai",
    display_name: "OpenAI",
    supports: &SUPPORTS,
    provider_fields: crate::metadata::OPENAI_CACHE,
    credential_fields: crate::metadata::API_KEY,
    endpoint_overrides: true,
    traffic_policy: crate::policy::OPENAI_API,
};

impl Channel for OpenAiChannel {
    fn routing_table(&self) -> &'static [ChannelSupport] {
        routes::ROUTES
    }

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
        crate::shared::openai::disposition::classify(response)
    }

    fn stream_decoder(&self, ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
        sse::OpenAiSseDecoder::for_operation(ctx)
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
