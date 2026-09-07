mod embeddings;
mod routes;

mod auth;
mod endpoint;
mod model;
mod prepare;
mod resource;
mod response;
mod shape;
mod sse;
mod usage;

use gproxy_channel_api::{
    BoxFuture, Channel, ChannelDescriptor, ChannelSupport, Disposition, NormalizedUsage,
    PrepareCtx, PreparedRequest, ResourceCtx, ResourceMutation, ResponseShapeCtx, ResponseView,
    SimpleHttp, StreamCtx, StreamDecoder, UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};
use serde_json::Value;

pub struct VertexChannel;

const fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

const fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

const fn gemini(operation: Operation) -> OperationKey {
    content(operation, ContentGenerationKind::GeminiGenerateContent)
}

static SUPPORTS: [ChannelSupport; 17] = [
    ChannelSupport::passthrough(family(Operation::CreateEmbedding, WireFamily::Gemini)),
    ChannelSupport::passthrough(family(Operation::BatchCreateEmbedding, WireFamily::Gemini)),
    ChannelSupport::passthrough(family(Operation::ListModels, WireFamily::Gemini)),
    ChannelSupport::passthrough(family(Operation::GetModel, WireFamily::Gemini)),
    ChannelSupport::passthrough(family(Operation::CountTokens, WireFamily::Gemini)),
    ChannelSupport::passthrough(gemini(Operation::GenerateContent)),
    ChannelSupport::passthrough(gemini(Operation::StreamGenerateContent)),
    ChannelSupport::passthrough(family(Operation::CountTokens, WireFamily::Claude)),
    ChannelSupport::passthrough(content(
        Operation::GenerateContent,
        ContentGenerationKind::ClaudeMessages,
    )),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::ClaudeMessages,
    )),
    ChannelSupport::passthrough(content(
        Operation::GenerateContent,
        ContentGenerationKind::OpenAiChat,
    )),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiChat,
    )),
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
    ChannelSupport::passthrough(family(Operation::CreateImage, WireFamily::Gemini)),
    ChannelSupport::passthrough(family(Operation::CreateVideo, WireFamily::Gemini)),
    ChannelSupport::passthrough(family(Operation::RetrieveVideo, WireFamily::Gemini)),
];

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "vertex",
    display_name: "Google Vertex AI",
    supports: &SUPPORTS,
    provider_fields: crate::metadata::VERTEX,
    credential_fields: crate::metadata::SERVICE_ACCOUNT,
    endpoint_overrides: true,
    traffic_policy: crate::policy::VERTEX,
};

impl Channel for VertexChannel {
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
            401..=403 => Disposition::CredentialDead,
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
        response::shape(ctx)
    }

    fn refresh_due(&self, secret: &Value) -> Option<i64> {
        auth::refresh_due(secret)
    }

    fn refresh<'a>(
        &'a self,
        secret: &'a Value,
        _provider_settings: &'a Value,
        http: &'a dyn SimpleHttp,
    ) -> Option<BoxFuture<'a, Result<Value, gproxy_channel_api::ChannelError>>> {
        Some(auth::refresh(secret, http))
    }
}

#[cfg(test)]
mod tests;
