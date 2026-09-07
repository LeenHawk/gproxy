mod routes;

mod auth;
mod endpoint;
mod messages;
mod model;
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

pub struct AwsBedrockChannel;

const fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

const fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

const fn claude(operation: Operation) -> OperationKey {
    content(operation, ContentGenerationKind::ClaudeMessages)
}

static SUPPORTS: [ChannelSupport; 17] = [
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
    ChannelSupport::transform(
        family(Operation::CountTokens, WireFamily::OpenAi),
        family(Operation::CountTokens, WireFamily::Claude),
    ),
    ChannelSupport::passthrough(claude(Operation::GenerateContent)),
    ChannelSupport::passthrough(claude(Operation::StreamGenerateContent)),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
        claude(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
        claude(Operation::StreamGenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        claude(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        claude(Operation::StreamGenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        claude(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        claude(Operation::StreamGenerateContent),
    ),
    ChannelSupport::transform(
        family(Operation::CompactContent, WireFamily::OpenAi),
        claude(Operation::GenerateContent),
    ),
    ChannelSupport::passthrough(family(Operation::CreateVideo, WireFamily::OpenAi)),
    ChannelSupport::passthrough(family(Operation::RetrieveVideo, WireFamily::OpenAi)),
];

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "aws-bedrock",
    display_name: "AWS Bedrock",
    supports: &SUPPORTS,
    provider_fields: crate::metadata::BEDROCK,
    credential_fields: crate::metadata::AWS,
    endpoint_overrides: true,
    traffic_policy: crate::policy::AWS_BEDROCK,
};

impl Channel for AwsBedrockChannel {
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

    fn fallback_model(&self, primary: &str, model: &str) -> String {
        crate::shared::claude::fallback::namespaced(primary, model)
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
        if messages::native(ctx.request_body) {
            return Some(Box::new(sse::invoke::InvokeDecoder::new(ctx)));
        }
        (ctx.key == claude(Operation::StreamGenerateContent))
            .then(|| Box::new(sse::BedrockStreamDecoder::new()) as Box<dyn StreamDecoder>)
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
