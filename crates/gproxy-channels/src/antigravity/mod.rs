mod routes;

mod auth;
mod login;
mod models;
mod prepare;
mod profile;
mod quota;
mod shape;
mod sse;
mod usage;

use gproxy_channel_api::{
    BoxFuture, Channel, ChannelDescriptor, ChannelLoginRef, ChannelSupport, Disposition,
    LoginDescriptor, LoginMode, LoginParam, LoginParamKind, NormalizedUsage, PrepareCtx,
    PreparedRequest, ResponseShapeCtx, ResponseView, SimpleHttp, StreamCtx, StreamDecoder,
    UsageCtx,
};
use serde_json::Value;

pub struct AntigravityChannel;

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "antigravity",
    display_name: "Antigravity",
    provider_fields: crate::metadata::VERTEX,
    credential_fields: crate::metadata::GOOGLE_OAUTH,
    endpoint_overrides: true,
    traffic_policy: crate::policy::ANTIGRAVITY,
};

static LOGIN_PARAMS: &[LoginParam] = &[LoginParam {
    name: "project_id",
    kind: LoginParamKind::Text,
    required: false,
    default_value: None,
    options: &[],
    modes: &[LoginMode::AuthCode],
    condition: None,
}];

static LOGIN: LoginDescriptor = LoginDescriptor {
    modes: &[LoginMode::AuthCode],
    params: LOGIN_PARAMS,
};

impl Channel for AntigravityChannel {
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
        crate::shared::disposition::unauthorized_or_forbidden(response)
    }

    fn stream_decoder(&self, ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
        sse::decoder(ctx)
    }

    fn extract_usage(&self, ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
        usage::from_body(ctx)
    }

    fn quota_capabilities(&self, _secret: &Value) -> Option<gproxy_channel_api::QuotaCapabilities> {
        Some(gproxy_channel_api::QuotaCapabilities::SUBSCRIPTION)
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
