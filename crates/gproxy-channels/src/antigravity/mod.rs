mod routes;

mod auth;
mod models;
mod prepare;
mod profile;
mod quota;
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

pub struct AntigravityChannel;

const fn family(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::Gemini)
}

const fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

const fn gemini(operation: Operation) -> OperationKey {
    content(operation, ContentGenerationKind::GeminiGenerateContent)
}

static SUPPORTS: [ChannelSupport; 10] = [
    ChannelSupport::passthrough(family(Operation::ListModels)),
    ChannelSupport::passthrough(family(Operation::CountTokens)),
    ChannelSupport::passthrough(gemini(Operation::GenerateContent)),
    ChannelSupport::passthrough(gemini(Operation::StreamGenerateContent)),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
        gemini(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        gemini(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        gemini(Operation::GenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiChat,
        ),
        gemini(Operation::StreamGenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        gemini(Operation::StreamGenerateContent),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        gemini(Operation::StreamGenerateContent),
    ),
];

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "antigravity",
    display_name: "Antigravity",
    supports: &SUPPORTS,
    provider_fields: crate::metadata::VERTEX,
    credential_fields: crate::metadata::GOOGLE_OAUTH,
    endpoint_overrides: true,
    traffic_policy: crate::policy::ANTIGRAVITY,
};

impl Channel for AntigravityChannel {
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
        auth::refresh_due(secret)
    }

    fn refresh<'a>(
        &'a self,
        secret: &'a Value,
        settings: &'a Value,
        http: &'a dyn SimpleHttp,
    ) -> Option<BoxFuture<'a, Result<Value, gproxy_channel_api::ChannelError>>> {
        Some(auth::refresh(secret, settings, http))
    }
}

#[cfg(test)]
mod tests;
