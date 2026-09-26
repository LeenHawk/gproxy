//! An API-key upstream that speaks one or more native dialects verbatim.
//!
//! The provider's `base_url` is the upstream origin, optionally with a path
//! prefix; the request path arrives already native (from the client or from
//! the host's conversion), so the final URL is `base_url + path` unless the
//! host supplies a complete method URL. Authentication is injected per target
//! wire family from the credential's `api_key`; source authentication in
//! headers and query is always removed, and `config.allowed_headers` can
//! restrict forwarding to a named set. No protocol conversion happens here.
//!
//! The reply is the upstream's own body for the same reason, so it is metered
//! the way the other pass-through channels meter theirs: with the vendor's
//! field names, through `shared::vendor_usage`.

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    HOST_CONFIG_KEYS, HeaderAllowlist, LoginMode, NormalizedUsage, PrepareContext, ProviderView,
    UsageContext, UsageExtractor, forwardable,
};
use crate::channels::shared::{cache, vendor_usage};
use gproxy_protocol::{Dialect, HttpBody, Operation, WireFamily, WireRequest};
use http::{HeaderName, HeaderValue, header};
use serde::Deserialize;

pub const ID: &str = "custom";
const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";
const USAGE: CustomUsage = CustomUsage;

/// Provider `config` JSON understood by this channel. Unknown keys are ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CustomConfig {
    pub fallback_mode: crate::channels::shared::claude_fallback::FallbackMode,
    pub fallback_models: Vec<String>,
    /// Dialects the upstream accepts. Empty means the
    /// canonical dialect of every family (`openai`, `claude`, `gemini`) plus
    /// `openai_chat`, i.e. "whatever the client sends, forward it".
    pub dialects: Vec<Dialect>,
    /// Override the authentication header for every family, e.g. an
    /// OpenAI-compatible vendor that wants `api-key`.
    pub auth_header: Option<String>,
    /// Prefix before the key when `auth_header` is set; defaults to none.
    pub auth_prefix: Option<String>,
    /// Static headers added to every upstream request.
    pub headers: std::collections::BTreeMap<String, String>,
    /// Place `cache_control` where a client embeds a magic cache string in
    /// a Claude-dialect body (`channels::shared::cache`). Off by default;
    /// the strings are stripped either way.
    pub enable_claude_magic_cache: bool,
    /// Place `prompt_cache_breakpoint` where a client embeds a magic cache
    /// string in an OpenAI Chat or Responses body. Off by default; the
    /// strings are stripped either way.
    pub enable_openai_magic_cache: bool,
}

impl CustomConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Custom;

impl Custom {
    fn build<B>(
        &self,
        ctx: PrepareContext<'_, B>,
    ) -> Result<(http::request::Builder, WireRequest<B>), ChannelError> {
        let config = CustomConfig::from_view(ctx.provider)?;
        let key = ctx
            .credential
            .secret
            .get("api_key")
            .and_then(|v| v.as_str())
            .filter(|k| !k.is_empty())
            .ok_or(ChannelError::InvalidCredential)?;
        let mut request = ctx.request;
        let url = match ctx.endpoint_override {
            Some(url) => url.to_owned(),
            None => {
                let base = ctx
                    .provider
                    .base_url
                    .ok_or_else(|| ChannelError::InvalidConfig("base_url is required".into()))?
                    .trim_end_matches('/');
                let path = if request.path.starts_with('/') {
                    request.path.clone()
                } else {
                    format!("/{}", request.path)
                };
                format!("{base}{path}")
            }
        };
        let query = request.query.take().map(|q| strip_query_auth(&q));
        let uri = match query.filter(|q| !q.is_empty()) {
            Some(q) => format!("{url}?{q}"),
            None => url,
        };
        let allowlist = HeaderAllowlist::from_view(ctx.provider)?;
        let mut headers = forwardable(&request.headers, allowlist.as_ref(), &[]);
        let family = ctx.operation.dialect.family();
        match (&config.auth_header, family) {
            (Some(name), _) => {
                let value = format!("{}{key}", config.auth_prefix.as_deref().unwrap_or(""));
                headers.insert(
                    HeaderName::from_bytes(name.as_bytes())
                        .map_err(|_| ChannelError::InvalidConfig("auth_header".into()))?,
                    HeaderValue::from_str(&value).map_err(|_| ChannelError::InvalidCredential)?,
                );
            }
            (None, WireFamily::OpenAi) => {
                headers.insert(
                    header::AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {key}"))
                        .map_err(|_| ChannelError::InvalidCredential)?,
                );
            }
            (None, WireFamily::Claude) => {
                headers.insert(
                    HeaderName::from_static("x-api-key"),
                    HeaderValue::from_str(key).map_err(|_| ChannelError::InvalidCredential)?,
                );
                headers
                    .entry(HeaderName::from_static("anthropic-version"))
                    .or_insert_with(|| HeaderValue::from_static(DEFAULT_ANTHROPIC_VERSION));
            }
            (None, WireFamily::Gemini) => {
                headers.insert(
                    HeaderName::from_static("x-goog-api-key"),
                    HeaderValue::from_str(key).map_err(|_| ChannelError::InvalidCredential)?,
                );
            }
        }
        for (name, value) in &config.headers {
            headers.insert(
                HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| ChannelError::InvalidConfig(format!("header `{name}`")))?,
                HeaderValue::from_str(value)
                    .map_err(|_| ChannelError::InvalidConfig(format!("header `{name}`")))?,
            );
        }
        let mut builder = http::Request::builder()
            .method(request.method.clone())
            .uri(uri);
        if let Some(map) = builder.headers_mut() {
            *map = headers;
        }
        Ok((builder, request))
    }
}

impl BaseChannel for Custom {
    fn id(&self) -> &'static str {
        ID
    }

    /// An API key and a base URL; nothing to refresh, no account to query.
    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Custom (API key)",
            login_modes: vec![LoginMode::ApiKey],
            capabilities: ChannelCapabilities::default(),
            config_keys: [
                ConfigKey::required(
                    "base_url",
                    ConfigKeyKind::String,
                    "Upstream origin, optionally with a path prefix; the native request path is appended. Provider column, not config JSON.",
                ),
                ConfigKey::optional(
                    "dialects",
                    ConfigKeyKind::Json,
                    "Wire dialects the upstream accepts natively. Empty accepts openai, openai_chat, claude and gemini.",
                ),
                ConfigKey::optional(
                    "auth_header",
                    ConfigKeyKind::String,
                    "Header carrying the API key instead of each family's own, e.g. `api-key`.",
                ),
                ConfigKey::optional(
                    "auth_prefix",
                    ConfigKeyKind::String,
                    "Prefix written before the key when auth_header is set.",
                ),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static headers added to every upstream request.",
                ),
                ConfigKey::optional(
                    "enable_claude_magic_cache",
                    ConfigKeyKind::Bool,
                    "Turn a client's magic cache string in a Claude-dialect body into cache_control. The strings are stripped either way.",
                ),
                ConfigKey::optional(
                    "enable_openai_magic_cache",
                    ConfigKeyKind::Bool,
                    "Turn a client's magic cache string in an OpenAI Chat or Responses body into prompt_cache_breakpoint.",
                ),
            ]
            .into_iter()
            .chain(crate::channel::CLAUDE_FALLBACK_KEYS)
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    fn native_dialects(&self, provider: ProviderView<'_>, _operation: Operation) -> Vec<Dialect> {
        let configured = CustomConfig::from_view(provider)
            .map(|c| c.dialects)
            .unwrap_or_default();
        if configured.is_empty() {
            vec![
                Dialect::OpenAi,
                Dialect::OpenAiChat,
                Dialect::Claude,
                Dialect::Gemini,
            ]
        } else {
            configured
        }
    }

    /// Buffered JSON bodies are shaped for the magic cache strings by the
    /// operation's native dialect; a streamed request body passes through.
    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let config = CustomConfig::from_view(ctx.provider)?;
        let rules = cache::rules_for(
            ctx.operation.dialect,
            config.enable_claude_magic_cache,
            config.enable_openai_magic_cache,
        );
        let operation = ctx.operation;
        let (mut builder, request) = self.build(ctx)?;
        let body = match request.body {
            HttpBody::Bytes(bytes) => {
                let bytes = cache::shape(bytes, rules);
                if operation.dialect == Dialect::Claude
                    && matches!(operation.operation, Operation::GenerateContent | Operation::StreamGenerateContent)
                    && let Ok(mut body) = serde_json::from_slice::<serde_json::Value>(&bytes)
                {
                    // `None` when the builder already holds an error, such as a
                    // base URL that is not a URI; that is configuration, not a panic.
                    let headers = builder.headers_mut().ok_or_else(|| {
                        ChannelError::InvalidConfig("custom request could not be built".into())
                    })?;
                    crate::channels::shared::claude_fallback::fallbacks(
                        &mut body, headers,
                        &config.fallback_mode, &config.fallback_models,
                    );
                    HttpBody::Bytes(gproxy_protocol::connection::Bytes::from(body.to_string()))
                } else { HttpBody::Bytes(bytes) }
            },
            other => other,
        };
        builder
            .body(body)
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    fn prepare_connect(
        &self,
        ctx: PrepareContext<'_, ()>,
    ) -> Result<http::Request<()>, ChannelError> {
        let (builder, _) = self.build(ctx)?;
        builder
            .body(())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        Some(&USAGE)
    }
}

/// Per-call metering. The upstream's reply is forwarded verbatim, so its
/// counts are read with the vendor's own field names, keyed by the dialect the
/// exchange actually spoke — the same reading `azure`, `vertex` and
/// `vertexexpress` take, and for the same reason.
///
/// One extractor covers both framings: the host accumulates a streamed body
/// whenever a channel offers an extractor and no observer, and
/// `vendor_usage::from_body` reads accumulated `text/event-stream` bytes as
/// readily as a buffered JSON document. A realtime websocket is the exception
/// — only a `UsageStream` observer sees frames — but `ConnectRealtime` is not
/// a metered operation here either way.
///
/// A body this reader does not recognise yields `None`, and the host's local
/// estimate fills the record as before. That is deliberate: an unrecognised
/// shape is a missing number, not a zero.
struct CustomUsage;

impl UsageExtractor for CustomUsage {
    fn extract(&self, context: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        if !context.response.status.is_success() || !is_metered(context.operation.operation) {
            return Ok(None);
        }
        vendor_usage::from_body(context.operation.dialect, context.response.body)
    }
}

/// Operations that consume model tokens and whose replies carry a vendor usage
/// block: the union of what `azure` and `vertex` meter, because a `custom`
/// provider may be pointed at either kind of upstream. Counting tokens is
/// excluded for the reason it is excluded there — `count_tokens` answers with
/// numbers that were never consumed.
fn is_metered(operation: Operation) -> bool {
    matches!(
        operation,
        Operation::GenerateContent
            | Operation::StreamGenerateContent
            | Operation::CompactContent
            | Operation::CreateEmbedding
            | Operation::BatchCreateEmbedding
            | Operation::CreateImage
            | Operation::EditImage
    )
}

/// Remove `key`, `access_token` and `api_key` parameters, keeping the rest
/// byte for byte in their original order.
fn strip_query_auth(query: &str) -> String {
    query
        .split('&')
        .filter(|segment| {
            let name = segment.split('=').next().unwrap_or("");
            !matches!(name, "key" | "access_token" | "api_key")
        })
        .collect::<Vec<_>>()
        .join("&")
}
