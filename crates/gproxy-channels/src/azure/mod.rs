mod routes;

mod model;
mod prepare;
mod shape;
mod sse;
mod usage;

use gproxy_channel_api::{
    Channel, ChannelDescriptor, ChannelSupport, Disposition, NormalizedUsage, PrepareCtx,
    PreparedRequest, ResourceCtx, ResourceMutation, ResponseView, StreamCtx, StreamDecoder,
    UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};

pub struct AzureChannel;

const fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

const fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

static SUPPORTS: [ChannelSupport; 27] = [
    ChannelSupport::passthrough(family(Operation::ListModels, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::GetModel, WireFamily::OpenAi)),
    ChannelSupport::transform(
        family(Operation::ListModels, WireFamily::Claude),
        family(Operation::ListModels, WireFamily::OpenAi),
    ),
    ChannelSupport::transform(
        family(Operation::GetModel, WireFamily::Claude),
        family(Operation::GetModel, WireFamily::OpenAi),
    ),
    ChannelSupport::passthrough(family(Operation::CountTokens, WireFamily::Claude)),
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
        Operation::GenerateContent,
        ContentGenerationKind::ClaudeMessages,
    )),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::ClaudeMessages,
    )),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
    ),
    ChannelSupport::passthrough(family(Operation::CreateEmbedding, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::CreateImage, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::EditImage, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::CreateVideo, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::RetrieveVideo, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::ListVideos, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::DeleteVideo, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::DownloadVideoContent, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::RemixVideo, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::CreateVideoCharacter, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::GetVideoCharacter, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::EditVideo, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::ExtendVideo, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::CompactContent, WireFamily::OpenAi)),
];

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "azure",
    display_name: "Microsoft Azure",
    supports: &SUPPORTS,
    provider_fields: crate::metadata::AZURE,
    credential_fields: crate::metadata::API_KEY,
    endpoint_overrides: true,
    traffic_policy: crate::policy::AZURE,
};

impl Channel for AzureChannel {
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

    fn claude_fallback(&self) -> Option<gproxy_channel_api::ClaudeFallbackCapabilities> {
        Some(gproxy_channel_api::ClaudeFallbackCapabilities {
            server_side: false,
            credit: true,
            recommended_model: Some(crate::shared::claude::fallback::RECOMMENDED_MODEL),
        })
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
        crate::shared::openai::resource::settlement_ready(ctx)
    }

    fn resource_mutations(
        &self,
        ctx: ResourceCtx<'_>,
    ) -> Result<Vec<ResourceMutation>, gproxy_channel_api::ChannelError> {
        crate::shared::openai::resource::mutations(ctx)
    }
}

#[cfg(test)]
mod tests;
