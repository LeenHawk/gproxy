mod routes;

mod auth;
mod login;
mod model;
mod prepare;
mod quota;
mod refresh;
mod response;
mod sse;
mod usage;

use gproxy_channel_api::{
    BoxFuture, Channel, ChannelDescriptor, ChannelLoginRef, ChannelSupport, Disposition,
    LoginDescriptor, LoginMode, NormalizedUsage, PrepareCtx, PreparedRequest, ResponseShapeCtx,
    ResponseView, SimpleHttp, StreamCtx, StreamDecoder, UsageCtx,
};
use serde_json::Value;

pub struct ClineChannel;

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "cline",
    display_name: "Cline",
    provider_fields: crate::metadata::BASE_URL,
    credential_fields: crate::metadata::API_KEY_OR_OAUTH,
    endpoint_overrides: true,
    traffic_policy: crate::policy::CLINE,
};

static LOGIN: LoginDescriptor = LoginDescriptor {
    modes: &[LoginMode::Device],
    params: &[],
};

impl Channel for ClineChannel {
    fn quota_sources(
        &self,
        secret: &serde_json::Value,
        _settings: &serde_json::Value,
    ) -> Vec<gproxy_channel_api::QuotaSource> {
        quota::sources(secret)
    }
    fn prepare_quota_source(
        &self,
        source_id: &str,
        secret: &serde_json::Value,
        settings: &serde_json::Value,
    ) -> Result<Option<http::Request<bytes::Bytes>>, gproxy_channel_api::ChannelError> {
        quota::prepare(source_id, secret, settings)
    }
    fn parse_quota_source(
        &self,
        source_id: &str,
        status: http::StatusCode,
        _headers: &http::HeaderMap,
        body: &[u8],
    ) -> Result<Vec<gproxy_channel_api::QuotaEntry>, gproxy_channel_api::ChannelError> {
        quota::parse(source_id, status, body)
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
        crate::shared::disposition::unauthorized_only(response)
    }

    fn stream_decoder(&self, ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
        sse::decoder(ctx)
    }

    fn extract_usage(&self, ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
        usage::from_body(ctx)
    }

    fn shape_response(
        &self,
        ctx: ResponseShapeCtx<'_>,
    ) -> Result<bytes::Bytes, gproxy_channel_api::ChannelError> {
        response::shape(ctx)
    }

    fn refresh_due(&self, secret: &Value) -> Option<i64> {
        refresh::due(secret)
    }

    fn refresh<'a>(
        &'a self,
        secret: &'a Value,
        provider_settings: &'a Value,
        http: &'a dyn SimpleHttp,
    ) -> Option<BoxFuture<'a, Result<Value, gproxy_channel_api::ChannelError>>> {
        auth::field(secret, "refresh_token")
            .is_some()
            .then(|| refresh::refresh(secret, provider_settings, http))
    }
}

#[cfg(test)]
mod tests;
