//! An API-key upstream that speaks one or more native dialects verbatim.
//!
//! The provider's `base_url` is the upstream origin, optionally with a path
//! prefix; the request path arrives already native (from the client or from
//! the host's conversion), so the final URL is `base_url + path` unless the
//! host supplies a complete method URL. Authentication is injected per target
//! wire family from the credential's `api_key`; source authentication in
//! headers and query is always removed. No protocol conversion happens here.

use crate::channel::{BaseChannel, ChannelError, PrepareContext, ProviderView};
use gproxy_protocol::{Dialect, HttpBody, Operation, WireFamily, WireRequest};
use http::{HeaderMap, HeaderName, HeaderValue, header};
use serde::Deserialize;

pub const ID: &str = "custom";
const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";

/// Provider `config` JSON understood by this channel. Unknown keys are ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CustomConfig {
    /// Dialects the upstream accepts, in preference order. Empty means the
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
        let mut headers = forwardable(&request.headers);
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

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let (builder, request) = self.build(ctx)?;
        builder
            .body(request.body)
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
}

/// Drop hop-by-hop headers, the source host/length and any source
/// authentication; keep every vendor header (betas, versions, tracing).
fn forwardable(source: &HeaderMap) -> HeaderMap {
    const DROP: &[&str] = &[
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
        "host",
        "content-length",
        "authorization",
        "x-api-key",
        "x-goog-api-key",
        "api-key",
    ];
    let mut out = HeaderMap::with_capacity(source.len());
    for (name, value) in source {
        if DROP.contains(&name.as_str()) {
            continue;
        }
        out.append(name.clone(), value.clone());
    }
    out
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
