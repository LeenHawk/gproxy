//! The `BaseChannel` implementation: the Gemini operations express mode
//! serves, and how one request is built. Every operation uses the default
//! HTTP flow; the body is Gemini's own and passes through untouched.

use super::{ID, VertexExpress, endpoint};
use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    HOST_CONFIG_KEYS, HeaderAllowlist, LoginMode, PrepareContext, ProviderView, forwardable,
};
use gproxy_protocol::{Dialect, HttpBody, Operation};


impl BaseChannel for VertexExpress {
    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Google Vertex AI Express",
            login_modes: vec![LoginMode::ApiKey],
            capabilities: ChannelCapabilities::default(),
            config_keys: [ConfigKey::optional(
                "base_url",
                ConfigKeyKind::String,
                "Complete origin, replacing https://aiplatform.googleapis.com. Provider column, not config JSON. Express mode has no project and no region to configure.",
            ).with_placeholder(endpoint::DEFAULT_ORIGIN)]
            .into_iter()
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    /// Express mode serves Google's own models only, through the Gemini wire.
    fn native_dialects(&self, _provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent
            | Operation::StreamGenerateContent
            | Operation::CountTokens => vec![Dialect::Gemini],
            _ => Vec::new(),
        }
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let key = ctx
            .credential
            .secret
            .get("api_key")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .ok_or(ChannelError::InvalidCredential)?;
        let uri = endpoint::method_url(
            ctx.provider.base_url,
            ctx.endpoint_override,
            ctx.operation,
            &ctx.request.path,
            ctx.request.query.as_deref(),
            key,
        )?;
        let allowlist = HeaderAllowlist::from_view_for(
            ctx.provider,
            crate::channel::ChannelHeaders::native(ctx.operation.dialect),
        )?;
        let headers = forwardable(&ctx.request.headers, allowlist.as_ref(), &[]);
        let mut builder = http::Request::builder()
            .method(ctx.request.method.clone())
            .uri(uri);
        if let Some(map) = builder.headers_mut() {
            *map = headers;
        }
        builder
            .body(ctx.request.body)
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}
