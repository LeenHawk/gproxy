mod routes;

mod auth;
mod identity;
mod model;
mod prepare;
mod quota;
mod refresh;
mod shape;
mod sse;
mod usage;

use gproxy_channel_api::{
    BoxFuture, Channel, ChannelDescriptor, ChannelSupport, Disposition, NormalizedUsage,
    PrepareCtx, PreparedRequest, ResponseShapeCtx, ResponseView, SimpleHttp, StreamCtx,
    StreamDecoder, UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};
use serde_json::Value;

pub struct WorkBuddyChannel;

const fn family(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::OpenAi)
}

const fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

static SUPPORTS: [ChannelSupport; 12] = [
    ChannelSupport::passthrough(family(Operation::ListModels)),
    ChannelSupport::transform(
        OperationKey::family(Operation::ListModels, WireFamily::Claude),
        family(Operation::ListModels),
    ),
    ChannelSupport::passthrough(content(
        Operation::GenerateContent,
        ContentGenerationKind::OpenAiChat,
    )),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
    ),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        content(
            Operation::GenerateContent,
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
            ContentGenerationKind::OpenAiChat,
        ),
    ),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiChat,
    )),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
    ),
    ChannelSupport::transform(
        content(
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
            Operation::StreamGenerateContent,
            ContentGenerationKind::GeminiGenerateContent,
        ),
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
    ),
    ChannelSupport::passthrough(family(Operation::CreateImage)),
    ChannelSupport::passthrough(family(Operation::EditImage)),
];

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "workbuddy",
    display_name: "WorkBuddy",
    supports: &SUPPORTS,
    provider_fields: crate::metadata::BASE_URL,
    credential_fields: crate::metadata::OAUTH,
    endpoint_overrides: true,
    traffic_policy: crate::policy::WORKBUDDY,
};

impl Channel for WorkBuddyChannel {
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

    fn prepare_quota_probe(
        &self,
        secret: &Value,
        provider_settings: &Value,
    ) -> Result<Option<http::Request<bytes::Bytes>>, gproxy_channel_api::ChannelError> {
        quota::probe_request(secret, provider_settings)
    }

    fn parse_quota_probe(
        &self,
        status: http::StatusCode,
        body: &[u8],
    ) -> Vec<gproxy_channel_api::QuotaObservation> {
        quota::parse_probe(status, body)
    }

    fn shape_response(
        &self,
        ctx: ResponseShapeCtx<'_>,
    ) -> Result<bytes::Bytes, gproxy_channel_api::ChannelError> {
        shape::response(ctx)
    }

    fn refresh_due(&self, secret: &Value) -> Option<i64> {
        refresh::due(secret)
    }

    fn refresh<'a>(
        &'a self,
        secret: &'a Value,
        settings: &'a Value,
        http: &'a dyn SimpleHttp,
    ) -> Option<BoxFuture<'a, Result<Value, gproxy_channel_api::ChannelError>>> {
        Some(refresh::refresh(secret, settings, http))
    }
}

#[cfg(test)]
mod tests;
