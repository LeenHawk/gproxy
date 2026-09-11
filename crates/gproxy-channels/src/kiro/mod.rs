mod routes;

mod auth;
mod decoder;
mod endpoint;
mod login;
mod model_list;
mod prepare;
mod profile;
mod quota;
mod request;
mod sse;
mod tool_stream;
mod usage;

use gproxy_channel_api::{
    BoxFuture, Channel, ChannelDescriptor, ChannelLoginRef, ChannelSupport, Disposition,
    LoginDescriptor, LoginMode, LoginParam, LoginParamCondition, LoginParamKind, NormalizedUsage,
    PrepareCtx, PreparedRequest, ResponseShapeCtx, ResponseView, SimpleHttp, StreamCtx,
    StreamDecoder, UsageCtx,
};
use gproxy_protocol::Operation;
use serde_json::Value;

pub struct KiroChannel;

static DESCRIPTOR: ChannelDescriptor = ChannelDescriptor {
    id: "kiro",
    display_name: "Kiro",
    provider_fields: crate::metadata::KIRO,
    credential_fields: crate::metadata::KIRO_CREDENTIAL,
    endpoint_overrides: true,
    traffic_policy: crate::policy::KIRO,
};

static LOGIN_PARAMS: &[LoginParam] = &[
    LoginParam {
        name: "login_provider",
        kind: LoginParamKind::Select,
        required: true,
        default_value: Some("github"),
        options: &["github", "google"],
        modes: &[LoginMode::Device],
        condition: None,
    },
    LoginParam {
        name: "auth_method",
        kind: LoginParamKind::Select,
        required: true,
        default_value: Some("builder_id"),
        options: &["builder_id", "idc"],
        modes: &[LoginMode::AuthCode],
        condition: None,
    },
    LoginParam {
        name: "start_url",
        kind: LoginParamKind::Text,
        required: true,
        default_value: None,
        options: &[],
        modes: &[LoginMode::AuthCode],
        condition: Some(LoginParamCondition {
            param: "auth_method",
            equals: "idc",
        }),
    },
    LoginParam {
        name: "region",
        kind: LoginParamKind::Text,
        required: true,
        default_value: None,
        options: &[],
        modes: &[LoginMode::AuthCode],
        condition: Some(LoginParamCondition {
            param: "auth_method",
            equals: "idc",
        }),
    },
];

static LOGIN: LoginDescriptor = LoginDescriptor {
    modes: &[LoginMode::Device, LoginMode::AuthCode],
    params: LOGIN_PARAMS,
};

impl Channel for KiroChannel {
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
        decoder::KiroDecoder::for_operation(ctx)
            .map(|decoder| Box::new(decoder) as Box<dyn StreamDecoder>)
    }

    fn extract_usage(&self, ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
        usage::from_body(ctx.response_body)
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
        if ctx.status.is_success() && ctx.key.operation() == Operation::ListModels {
            model_list::response(ctx.body)
        } else {
            Ok(ctx.body.clone())
        }
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
        Some(auth::refresh(secret, provider_settings, http))
    }
}

#[cfg(test)]
mod tests;
