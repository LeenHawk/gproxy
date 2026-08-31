mod routes;

mod auth;
mod identity;
mod model;
mod prepare;
mod profile;
mod sse;
mod usage;

use gproxy_channel_api::{
    BoxFuture, Channel, ChannelDescriptor, ChannelSupport, Disposition, NormalizedUsage,
    PrepareCtx, PreparedRequest, ResponseView, SimpleHttp, StreamCtx, StreamDecoder, UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};
use serde_json::Value;

pub struct CopilotCliChannel;

const fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

const fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

const fn chat(operation: Operation) -> OperationKey {
    content(operation, ContentGenerationKind::OpenAiChat)
}

static SUPPORTS: [ChannelSupport; 10] = [
    ChannelSupport::passthrough(family(Operation::ListModels, WireFamily::OpenAi)),
    ChannelSupport::transform(
        family(Operation::ListModels, WireFamily::Claude),
        family(Operation::ListModels, WireFamily::OpenAi),
    ),
    ChannelSupport::passthrough(chat(Operation::GenerateContent)),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        chat(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        chat(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        chat(Operation::GenerateContent),
    ),
    ChannelSupport::passthrough(chat(Operation::StreamGenerateContent)),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        chat(Operation::StreamGenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        chat(Operation::StreamGenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        chat(Operation::StreamGenerateContent),
    ),
];

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "copilotcli",
    display_name: "GitHub Copilot CLI",
    supports: &SUPPORTS,
    provider_fields: crate::metadata::BASE_URL,
    credential_fields: crate::metadata::GITHUB,
    endpoint_overrides: true,
    traffic_policy: crate::policy::COPILOT,
};

impl Channel for CopilotCliChannel {
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
