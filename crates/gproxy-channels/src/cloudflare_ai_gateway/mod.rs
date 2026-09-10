mod routes;

mod model;
mod prepare;
mod sse;
mod usage;

use gproxy_channel_api::{
    Channel, ChannelDescriptor, ChannelSupport, Disposition, NormalizedUsage, PrepareCtx,
    PreparedRequest, ResponseView, StreamCtx, StreamDecoder, UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey};

pub struct CloudflareAiGatewayChannel;

const fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

static SUPPORTS: [ChannelSupport; 8] = [
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
];

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "cloudflare-ai-gateway",
    display_name: "Cloudflare AI Gateway",
    supports: &SUPPORTS,
    provider_fields: crate::metadata::BASE_URL,
    credential_fields: crate::metadata::API_KEY,
    endpoint_overrides: true,
    traffic_policy: crate::policy::CLOUDFLARE,
};

impl Channel for CloudflareAiGatewayChannel {
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
}

#[cfg(test)]
mod tests;
