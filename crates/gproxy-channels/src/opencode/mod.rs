mod routes;

mod auth;
mod login;
mod model;
mod prepare;
mod shape;
mod sse;
mod usage;

use gproxy_channel_api::{
    BoxFuture, Channel, ChannelDescriptor, ChannelLoginRef, ChannelSupport, Disposition,
    LoginDescriptor, LoginMode, NormalizedUsage, PrepareCtx, PreparedRequest, ResponseView,
    SimpleHttp, StreamCtx, StreamDecoder, UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};
use serde_json::Value;

pub struct OpenCodeChannel;

const fn family(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::OpenAi)
}

const fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

static SUPPORTS: [ChannelSupport; 10] = [
    ChannelSupport::passthrough(family(Operation::ListModels)),
    ChannelSupport::transform(
        OperationKey::family(Operation::ListModels, WireFamily::Claude),
        family(Operation::ListModels),
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
            ContentGenerationKind::OpenAiChat,
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
            ContentGenerationKind::OpenAiChat,
        ),
    ),
];

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "opencode",
    display_name: "OpenCode",
    supports: &SUPPORTS,
    provider_fields: crate::metadata::OPENCODE,
    credential_fields: crate::metadata::API_KEY_OR_OAUTH,
    endpoint_overrides: true,
    traffic_policy: crate::policy::OPENCODE,
};

static LOGIN: LoginDescriptor = LoginDescriptor {
    modes: &[LoginMode::Device],
    params: &[],
};

impl Channel for OpenCodeChannel {
    fn prepare_quota_source_page(
        &self,
        source: &str,
        secret: &serde_json::Value,
        settings: &serde_json::Value,
        cursor: Option<&str>,
    ) -> Result<Option<http::Request<bytes::Bytes>>, gproxy_channel_api::ChannelError> {
        crate::shared::quota_api::prepare_page(
            self.descriptor().id,
            source,
            secret,
            settings,
            cursor,
        )
    }

    fn parse_quota_source_page(
        &self,
        source: &str,
        status: http::StatusCode,
        _headers: &http::HeaderMap,
        body: &[u8],
    ) -> Result<gproxy_channel_api::QuotaSourcePage, gproxy_channel_api::ChannelError> {
        crate::shared::quota_api::parse_page(self.descriptor().id, source, status, body)
    }

    fn quota_fields(&self) -> &'static [gproxy_channel_api::ChannelField] {
        crate::shared::quota_catalog::fields(self.descriptor().id)
    }

    fn prepare_quota_source(
        &self,
        source: &str,
        secret: &serde_json::Value,
        settings: &serde_json::Value,
    ) -> Result<Option<http::Request<bytes::Bytes>>, gproxy_channel_api::ChannelError> {
        crate::shared::quota_api::prepare(self.descriptor().id, source, secret, settings)
    }

    fn parse_quota_source(
        &self,
        source: &str,
        status: http::StatusCode,
        _headers: &http::HeaderMap,
        body: &[u8],
    ) -> Result<Vec<gproxy_channel_api::QuotaEntry>, gproxy_channel_api::ChannelError> {
        crate::shared::quota_api::parse(self.descriptor().id, source, status, body)
    }

    fn quota_sources(
        &self,
        secret: &serde_json::Value,
        settings: &serde_json::Value,
    ) -> Vec<gproxy_channel_api::QuotaSource> {
        crate::shared::quota_catalog::sources(self.descriptor().id, secret, settings)
    }

    fn login(&self) -> Option<ChannelLoginRef<'_>> {
        Some(ChannelLoginRef {
            adapter: self,
            descriptor: &LOGIN,
        })
    }
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
