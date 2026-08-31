mod routes;

mod auth;
mod identity;
mod model;
mod prepare;
mod sse;
mod supports;
mod usage;

use gproxy_channel_api::{
    BoxFuture, Channel, ChannelDescriptor, ChannelSupport, Disposition, NormalizedUsage,
    PrepareCtx, PreparedRequest, ResponseView, SimpleHttp, StreamCtx, StreamDecoder, UsageCtx,
};
use gproxy_protocol::OperationKey;
use serde_json::Value;

pub struct KimiChannel;

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "kimi",
    display_name: "Kimi",
    supports: &supports::SUPPORTS,
    provider_fields: crate::metadata::BASE_URL,
    credential_fields: crate::metadata::API_KEY_OR_OAUTH,
    endpoint_overrides: true,
    traffic_policy: crate::policy::KIMI,
};

impl Channel for KimiChannel {
    fn routing_table(&self) -> &'static [ChannelSupport] {
        routes::ROUTES
    }

    fn descriptor(&self) -> &'static ChannelDescriptor {
        &DESCRIPTOR
    }

    fn select_support(&self, source: OperationKey, secret: &Value) -> Option<ChannelSupport> {
        let selected = supports::select(source, auth::mode(secret));
        if supports::SUPPORTS
            .iter()
            .any(|support| support.source == source)
        {
            return selected;
        }
        routes::ROUTES
            .iter()
            .find(|support| {
                support.source == source
                    && matches!(
                        support.action,
                        gproxy_channel_api::ChannelRouteAction::Passthrough
                            | gproxy_channel_api::ChannelRouteAction::TransformTo
                    )
            })
            .copied()
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
        provider_settings: &'a Value,
        http: &'a dyn SimpleHttp,
    ) -> Option<BoxFuture<'a, Result<Value, gproxy_channel_api::ChannelError>>> {
        (auth::mode(secret) == auth::Mode::Oauth)
            .then(|| auth::refresh(secret, provider_settings, http))
    }
}

#[cfg(test)]
mod tests;
