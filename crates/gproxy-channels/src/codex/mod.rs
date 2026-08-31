mod routes;

mod auth;
mod login;
mod model;
mod multipart;
mod prepare;
mod profile;
mod quota;
mod realtime;
mod resource;
mod shape;
mod sse;
mod surface;
mod usage;

use gproxy_channel_api::{
    BoxFuture, Channel, ChannelDescriptor, ChannelLoginRef, ChannelSupport, Disposition,
    LoginDescriptor, LoginMode, NormalizedUsage, PrepareCtx, PreparedRequest, ResourceCtx,
    ResourceMutation, ResponseShapeCtx, ResponseView, SessionPreparer, SimpleHttp, StreamCtx,
    StreamDecoder, UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};
use serde_json::Value;

pub struct CodexChannel;

const fn family(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::OpenAi)
}

const fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

static SUPPORTS: [ChannelSupport; 18] = [
    ChannelSupport::passthrough(family(Operation::ListModels)),
    ChannelSupport::passthrough(family(Operation::GetModel)),
    ChannelSupport::passthrough(family(Operation::SummarizeMemory)),
    ChannelSupport::transform(
        OperationKey::family(Operation::ListModels, WireFamily::Claude),
        family(Operation::ListModels),
    ),
    ChannelSupport::transform(
        OperationKey::family(Operation::GetModel, WireFamily::Claude),
        family(Operation::GetModel),
    ),
    ChannelSupport::passthrough(content(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiResponses,
    )),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
    ),
    ChannelSupport::passthrough(family(Operation::CompactContent)),
    ChannelSupport::passthrough(family(Operation::CreateImage)),
    ChannelSupport::passthrough(family(Operation::EditImage)),
    ChannelSupport::passthrough(family(Operation::WebSearch)),
    ChannelSupport::passthrough(family(Operation::CreateRealtimeCall)),
    ChannelSupport::transform(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        content(
            Operation::StreamGenerateContent,
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
            ContentGenerationKind::OpenAiChat,
        ),
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
    ),
    ChannelSupport::transform(
        content(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiChat,
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
            Operation::StreamGenerateContent,
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
    id: "codex",
    display_name: "OpenAI Codex",
    supports: &SUPPORTS,
    provider_fields: crate::metadata::OPENAI_CACHE,
    credential_fields: crate::metadata::OAUTH,
    endpoint_overrides: true,
    traffic_policy: crate::policy::CODEX,
};

static LOGIN: LoginDescriptor = LoginDescriptor {
    modes: &[LoginMode::AuthCode, LoginMode::Device],
    params: &[],
};

impl Channel for CodexChannel {
    fn routing_table(&self) -> &'static [ChannelSupport] {
        routes::ROUTES
    }

    fn descriptor(&self) -> &'static ChannelDescriptor {
        &DESCRIPTOR
    }

    fn login(&self) -> Option<ChannelLoginRef<'_>> {
        Some(ChannelLoginRef {
            adapter: self,
            descriptor: &LOGIN,
        })
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
        sse::CodexSseDecoder::for_operation(ctx)
            .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>)
    }

    fn extract_usage(&self, ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
        usage::from_body(ctx)
    }

    fn observe_quota(
        &self,
        headers: &http::HeaderMap,
    ) -> Vec<gproxy_channel_api::QuotaObservation> {
        quota::from_headers(headers)
    }

    fn prepare_quota_probe(
        &self,
        secret: &serde_json::Value,
        provider_settings: &serde_json::Value,
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

    fn prepare_quota_credits_probe(
        &self,
        secret: &serde_json::Value,
        provider_settings: &serde_json::Value,
    ) -> Result<Option<http::Request<bytes::Bytes>>, gproxy_channel_api::ChannelError> {
        quota::credits_probe_request(secret, provider_settings)
    }

    fn parse_quota_probe_credits(
        &self,
        status: http::StatusCode,
        body: &[u8],
    ) -> Option<gproxy_channel_api::QuotaResetCredits> {
        quota::parse_probe_credits(status, body)
    }

    fn prepare_quota_reset(
        &self,
        secret: &serde_json::Value,
        provider_settings: &serde_json::Value,
        redeem_request_id: &str,
    ) -> Result<Option<http::Request<bytes::Bytes>>, gproxy_channel_api::ChannelError> {
        quota::reset_request(secret, provider_settings, redeem_request_id)
    }

    fn parse_quota_reset(
        &self,
        status: http::StatusCode,
        body: &[u8],
    ) -> Option<gproxy_channel_api::QuotaResetResult> {
        quota::parse_reset(status, body)
    }

    fn session_preparer(&self) -> Option<SessionPreparer> {
        Some(realtime::prepare)
    }

    fn shape_response(
        &self,
        ctx: ResponseShapeCtx<'_>,
    ) -> Result<bytes::Bytes, gproxy_channel_api::ChannelError> {
        model::shape(ctx)
    }

    fn resource_mutations(
        &self,
        ctx: ResourceCtx<'_>,
    ) -> Result<Vec<ResourceMutation>, gproxy_channel_api::ChannelError> {
        resource::mutations(ctx)
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
