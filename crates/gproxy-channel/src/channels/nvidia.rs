//! NVIDIA NIM (`integrate.api.nvidia.com`), ported from v3's `nvidia`.
//!
//! An OpenAI-compatible catalogue: Chat Completions for conversation, plus
//! the model list, a single model and embeddings. Every other dialect reaches
//! it through the host's conversion to Chat, as v3's routing table did. The
//! key is a bearer on every call.
//!
//! v4 served this vendor as a `custom` provider for a while and it became a
//! channel again so a streamed Chat reply would report usage. Core now asks
//! every Chat stream for its usage, whatever the channel. v3 reported no quota
//! for NVIDIA and neither does this.

use crate::channel::{
    BaseChannel, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind, HOST_CONFIG_KEYS,
    HeaderAllowlist, LoginMode, PrepareContext, ProviderView, forwardable,
};
use crate::channels::shared::compatible::http::{insert_configured, strip_query_auth};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey};
use http::{HeaderValue, header};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

pub const ID: &str = "nvidia";
pub const DEFAULT_BASE_URL: &str = "https://integrate.api.nvidia.com";

/// Provider `config` JSON understood by this channel. Unknown keys ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct NvidiaConfig {
    /// Static headers added to every upstream request.
    pub headers: BTreeMap<String, String>,
}

impl NvidiaConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}

fn base_url(provider: ProviderView<'_>) -> &str {
    provider
        .base_url
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE_URL)
        .trim_end_matches('/')
}

fn api_key(secret: &Value) -> Result<&str, ChannelError> {
    secret
        .get("api_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or(ChannelError::InvalidCredential)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Nvidia;

impl BaseChannel for Nvidia {
    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "NVIDIA NIM",
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
            Operation::ListModels | Operation::GetModel | Operation::CreateEmbedding => {
                vec![Dialect::OpenAi]
            }
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                vec![Dialect::OpenAiChat]
            }
            _ => Vec::new(),
        }
    }

    /// Conversation in any other dialect is converted to Chat, the one the
    /// upstream speaks (v3 `nvidia/routes.rs`).
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

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        if !self
            .native_dialects(ctx.provider, ctx.operation.operation)
            .contains(&ctx.operation.dialect)
        {
            return Err(ChannelError::UnsupportedOperation(ctx.operation));
        }
        let config = NvidiaConfig::from_view(ctx.provider)?;
        let key = api_key(ctx.credential.secret)?;
        let url = match ctx.endpoint_override {
            Some(url) => url.to_owned(),
            None => format!(
                "{}/{}",
                base_url(ctx.provider),
                ctx.request.path.trim_start_matches('/')
            ),
        };
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
        let allowlist = HeaderAllowlist::from_view(ctx.provider)?;
        let mut headers = forwardable(&ctx.request.headers, allowlist.as_ref(), &[]);
        for (name, value) in &config.headers {
            insert_configured(&mut headers, name, value)?;
        }
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {key}"))
                .map_err(|_| ChannelError::InvalidCredential)?,
        );
        let body = match ctx.request.body {
            HttpBody::Bytes(bytes) if !bytes.is_empty() => {
                headers
                    .entry(header::CONTENT_TYPE)
                    .or_insert_with(|| HeaderValue::from_static("application/json"));
                HttpBody::Bytes(bytes)
            }
            body => body,
        };
        let mut builder = http::Request::builder().method(ctx.request.method).uri(uri);
        if let Some(target) = builder.headers_mut() {
            *target = headers;
        }
        builder
            .body(body)
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}

