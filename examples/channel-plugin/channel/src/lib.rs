//! `acme`: an OpenAI-compatible API-key upstream, implemented outside
//! `gproxy-channel` against nothing but its public contract.
//!
//! It is the smallest channel that is still a real one: a descriptor the
//! console renders a provider form from, the dialects it speaks natively, a
//! conversion target for everything else, and `prepare`, which turns the
//! host's request into the upstream's. Login, refresh, quota and usage
//! extras are optional abilities with default-`None` accessors, so leaving
//! them out is a channel without them rather than a broken one.

use std::collections::BTreeMap;

use gproxy_channel::channel::{
    BaseChannel, ChannelDescriptor, ChannelError, ChannelHeaders, ConfigKey, ConfigKeyKind,
    HOST_CONFIG_KEYS, HeaderAllowlist, LoginMode, PrepareContext, ProviderView, forwardable,
    insert_configured, strip_query_auth,
};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey};
use http::{HeaderValue, header};
use serde::Deserialize;
use serde_json::Value;

/// The provider's `channel` column. Unique across every channel the binary
/// registers; one that matches a built-in id replaces the built-in.
pub const ID: &str = "acme";
pub const DEFAULT_BASE_URL: &str = "https://api.acme.example";

/// The provider `config` JSON. Unknown keys are ignored, so the host's own
/// keys (`HOST_CONFIG_KEYS`) can sit beside these.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct AcmeConfig {
    /// Static headers added to every upstream request.
    pub headers: BTreeMap<String, String>,
}

/// Stateless, so one instance serves every provider on this channel.
#[derive(Debug, Default, Clone, Copy)]
pub struct Acme;

impl BaseChannel for Acme {
    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Acme",
            credential_kinds: vec!["api_key"],
            login_modes: vec![LoginMode::ApiKey],
            capabilities: Default::default(),
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Upstream origin, without `/v1`. Provider column, not config JSON.",
                )
                .with_placeholder(DEFAULT_BASE_URL),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static headers added to every upstream request.",
                ),
            ]
            .into_iter()
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    fn native_dialects(&self, _: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::ListModels | Operation::GetModel => vec![Dialect::OpenAi],
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                vec![Dialect::OpenAiChat]
            }
            _ => Vec::new(),
        }
    }

    /// A Claude Messages or Gemini request is converted to Chat Completions by
    /// the host before `prepare` sees it.
    fn default_conversion_target(
        &self,
        _: ProviderView<'_>,
        source: OperationKey,
    ) -> Option<OperationKey> {
        (matches!(
            source.operation,
            Operation::GenerateContent | Operation::StreamGenerateContent
        ) && source.dialect != Dialect::OpenAiChat)
            .then_some(OperationKey {
                operation: source.operation,
                dialect: Dialect::OpenAiChat,
            })
    }

    /// Pure and synchronous: no I/O here. The host sends what this returns.
    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        if !self
            .native_dialects(ctx.provider, ctx.operation.operation)
            .contains(&ctx.operation.dialect)
        {
            return Err(ChannelError::UnsupportedOperation(ctx.operation));
        }
        let config: AcmeConfig = serde_json::from_value(ctx.provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
        let key = ctx
            .credential
            .secret
            .get("api_key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .ok_or(ChannelError::InvalidCredential)?;

        // An endpoint override is the complete URL, not a base to append to.
        let url = match ctx.endpoint_override {
            Some(url) => url.to_owned(),
            None => format!(
                "{}/{}",
                ctx.provider
                    .base_url
                    .map(str::trim)
                    .filter(|base| !base.is_empty())
                    .unwrap_or(DEFAULT_BASE_URL)
                    .trim_end_matches('/'),
                ctx.request.path.trim_start_matches('/')
            ),
        };
        // The caller's `?key=` never reaches the upstream.
        let uri = match ctx
            .request
            .query
            .as_deref()
            .map(strip_query_auth)
            .filter(|query| !query.is_empty())
        {
            Some(query) => format!("{url}{}{query}", if url.contains('?') { "&" } else { "?" }),
            None => url,
        };

        // Client headers the global, provider and channel lists allow, minus
        // hop-by-hop headers and the caller's own authentication.
        let allowlist = HeaderAllowlist::from_view_for(
            ctx.provider,
            ChannelHeaders::native(ctx.operation.dialect),
        )?;
        let mut headers = forwardable(&ctx.request.headers, allowlist.as_ref(), &[]);
        for (name, value) in &config.headers {
            insert_configured(&mut headers, name, value)?;
        }
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {key}"))
                .map_err(|_| ChannelError::InvalidCredential)?,
        );
        if matches!(&ctx.request.body, HttpBody::Bytes(bytes) if !bytes.is_empty()) {
            headers
                .entry(header::CONTENT_TYPE)
                .or_insert_with(|| HeaderValue::from_static("application/json"));
        }

        let mut builder = http::Request::builder().method(ctx.request.method).uri(uri);
        if let Some(target) = builder.headers_mut() {
            *target = headers;
        }
        builder
            .body(ctx.request.body)
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}
