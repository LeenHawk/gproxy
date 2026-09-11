mod routes;

mod model;
mod prepare;
mod resource;

use gproxy_channel_api::{
    Channel, ChannelDescriptor, ChannelSupport, Disposition, NormalizedUsage, PrepareCtx,
    PreparedRequest, ResourceCtx, ResourceMutation, ResponseView, StreamCtx, StreamDecoder,
    UsageCtx,
};

pub struct AiStudioChannel;

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "aistudio",
    display_name: "Google AI Studio",
    provider_fields: crate::metadata::BASE_URL,
    credential_fields: crate::metadata::API_KEY,
    endpoint_overrides: true,
    traffic_policy: crate::policy::AISTUDIO,
};

impl Channel for AiStudioChannel {
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
        crate::shared::disposition::unauthorized_or_forbidden(response)
    }

    fn stream_decoder(&self, ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
        if model::is_openai(ctx.key) {
            return crate::shared::openai::OpenAiSseDecoder::for_operation(ctx)
                .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>);
        }
        crate::shared::gemini::stream::GeminiStreamDecoder::for_operation(ctx)
            .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>)
    }

    fn extract_usage(&self, ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
        if model::is_openai(ctx.key) {
            crate::shared::openai::usage_from_body(ctx)
        } else {
            crate::shared::gemini::usage::from_body(ctx)
        }
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
}

#[cfg(test)]
mod tests;
