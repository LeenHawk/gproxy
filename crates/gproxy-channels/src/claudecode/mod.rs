mod routes;

mod account;
mod auth;
mod cch;
mod cookie;
mod hygiene;
mod login;
mod prepare;
mod profile;
mod quota;
mod surface;

use gproxy_channel_api::{
    BoxFuture, Channel, ChannelDescriptor, ChannelLoginRef, ChannelSupport, Disposition,
    LoginDescriptor, LoginMode, NormalizedUsage, PrepareCtx, PreparedRequest, ResponseView,
    SimpleHttp, StreamCtx, StreamDecoder, UsageCtx,
};
use serde_json::Value;

pub struct ClaudeCodeChannel;

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "claudecode",
    display_name: "Claude Code",
    provider_fields: crate::metadata::CLAUDE,
    credential_fields: crate::metadata::OAUTH,
    endpoint_overrides: true,
    traffic_policy: crate::policy::CLAUDE_CODE,
};

#[cfg(not(target_arch = "wasm32"))]
const LOGIN_MODES: &[LoginMode] = &[LoginMode::AuthCode, LoginMode::Cookie];
#[cfg(target_arch = "wasm32")]
const LOGIN_MODES: &[LoginMode] = &[LoginMode::AuthCode];

static LOGIN: LoginDescriptor = LoginDescriptor {
    modes: LOGIN_MODES,
    params: &[],
};

impl Channel for ClaudeCodeChannel {
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

    fn claude_fallback(&self) -> Option<gproxy_channel_api::ClaudeFallbackCapabilities> {
        Some(gproxy_channel_api::ClaudeFallbackCapabilities {
            server_side: true,
            credit: true,
            recommended_model: None,
        })
    }

    fn fallback_model(&self, primary: &str, model: &str) -> String {
        crate::shared::claude::fallback::namespaced(primary, model)
    }

    fn classify(&self, response: ResponseView<'_>) -> Disposition {
        crate::shared::disposition::unauthorized_or_forbidden(response)
    }

    fn stream_decoder(&self, ctx: StreamCtx<'_>) -> Option<Box<dyn StreamDecoder>> {
        crate::shared::claude::sse::ClaudeSseDecoder::for_operation(ctx)
            .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>)
    }

    fn extract_usage(&self, ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
        crate::shared::claude::usage::from_body(ctx.response_body)
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
