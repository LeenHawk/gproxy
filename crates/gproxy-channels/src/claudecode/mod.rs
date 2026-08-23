mod auth;
mod cache;
mod cch;
mod hygiene;
mod prepare;
mod profile;
mod sse;
mod surface;
mod usage;

use gproxy_channel_api::{
    BoxFuture, Channel, ChannelDescriptor, ChannelSupport, Disposition, NormalizedUsage,
    PrepareCtx, PreparedRequest, ResponseView, SimpleHttp, StreamCtx, StreamDecoder, UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};
use serde_json::Value;

pub struct ClaudeCodeChannel;

const fn family(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::Claude)
}

const fn content(operation: Operation) -> OperationKey {
    OperationKey::content(operation, ContentGenerationKind::ClaudeMessages)
}

static SUPPORTS: [ChannelSupport; 15] = [
    ChannelSupport::passthrough(family(Operation::ListModels)),
    ChannelSupport::passthrough(family(Operation::GetModel)),
    ChannelSupport::passthrough(family(Operation::CountTokens)),
    ChannelSupport::passthrough(content(Operation::GenerateContent)),
    ChannelSupport::passthrough(content(Operation::StreamGenerateContent)),
    ChannelSupport::transform(
        OperationKey::family(Operation::ListModels, WireFamily::OpenAi),
        family(Operation::ListModels),
    ),
    ChannelSupport::transform(
        OperationKey::family(Operation::GetModel, WireFamily::OpenAi),
        family(Operation::GetModel),
    ),
    ChannelSupport::transform(
        OperationKey::family(Operation::CountTokens, WireFamily::OpenAi),
        family(Operation::CountTokens),
    ),
    ChannelSupport::transform(
        OperationKey::content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
        content(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        OperationKey::content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
        content(Operation::StreamGenerateContent),
    ),
    ChannelSupport::transform(
        OperationKey::content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        content(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        OperationKey::content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        content(Operation::StreamGenerateContent),
    ),
    ChannelSupport::transform(
        OperationKey::family(Operation::CompactContent, WireFamily::OpenAi),
        content(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        OperationKey::content(
            Operation::GenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        content(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        OperationKey::content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        content(Operation::StreamGenerateContent),
    ),
];

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "claudecode",
    display_name: "Claude Code",
    supports: &SUPPORTS,
};

impl Channel for ClaudeCodeChannel {
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
        sse::ClaudeSseDecoder::for_operation(ctx)
            .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>)
    }

    fn extract_usage(&self, ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
        usage::from_body(ctx.response_body)
    }

    fn refresh_due(&self, secret: &Value) -> Option<i64> {
        auth::refresh_due(secret)
    }

    fn refresh<'a>(
        &'a self,
        secret: &'a Value,
        http: &'a dyn SimpleHttp,
    ) -> Option<BoxFuture<'a, Result<Value, gproxy_channel_api::ChannelError>>> {
        Some(auth::refresh(secret, http))
    }

    fn prepare_surface(
        &self,
        request: &gproxy_channel_api::SurfaceRequest,
        websocket: bool,
        provider_settings: &Value,
        secret: &Value,
    ) -> Result<PreparedRequest, gproxy_channel_api::ChannelError> {
        prepare::surface(request, websocket, provider_settings, secret)
    }

    fn surfaces(&self) -> gproxy_channel_api::SurfaceTable {
        surface::table()
    }
}

#[cfg(test)]
mod tests;
