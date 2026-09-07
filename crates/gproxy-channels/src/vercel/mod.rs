mod routes;

mod model;
mod prepare;
mod shape;
mod sse;
mod usage;

use gproxy_channel_api::{
    Channel, ChannelDescriptor, ChannelSupport, Disposition, NormalizedUsage, PrepareCtx,
    PreparedRequest, ResponseView, StreamCtx, StreamDecoder, UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};

pub struct VercelChannel;

const fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

const fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

static SUPPORTS: [ChannelSupport; 15] = [
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
    ChannelSupport::passthrough(content(
        Operation::GenerateContent,
        ContentGenerationKind::OpenAiChat,
    )),
    ChannelSupport::passthrough(content(
        Operation::GenerateContent,
        ContentGenerationKind::OpenAiResponses,
    )),
    ChannelSupport::passthrough(content(
        Operation::GenerateContent,
        ContentGenerationKind::ClaudeMessages,
    )),
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
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiChat,
    )),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiResponses,
    )),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::ClaudeMessages,
    )),
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
    ChannelSupport::passthrough(family(Operation::CreateEmbedding, WireFamily::OpenAi)),
];

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "vercel",
    display_name: "Vercel AI Gateway",
    supports: &SUPPORTS,
    provider_fields: crate::metadata::VERCEL,
    credential_fields: crate::metadata::API_KEY,
    endpoint_overrides: true,
    traffic_policy: crate::policy::VERCEL,
};

impl Channel for VercelChannel {
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
            credit: false,
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
        sse::decoder(ctx)
    }

    fn extract_usage(&self, ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
        usage::from_body(ctx)
    }
}

#[cfg(test)]
mod tests;
