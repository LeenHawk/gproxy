//! Route layout, credential injection and header filtering for the Anthropic
//! API, plus the body shaping each route needs.

use super::hygiene;
use super::{ANTHROPIC_VERSION, Claudeapi, ClaudeapiConfig, DEFAULT_BASE_URL, ID};
use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ChannelHeaders, ConfigKey,
    ConfigKeyKind, HOST_CONFIG_KEYS, HeaderAllowlist, LoginMode, PrepareContext, ProviderView,
    QuotaHeaders, QuotaQuery, UsageExtractor, UsageStream, forwardable,
};
use crate::channels::shared::{cache, openai_wire};
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireFamily, WireRequest};
use http::{HeaderName, HeaderValue};

/// Query parameters that carry a client's own credential; never forwarded.
const QUERY_AUTH: &[&str] = &["access_token", "api_key", "key", "x-api-key"];
/// Headers this channel sets itself, so a client cannot supply them.
const CHANNEL_HEADERS: &[&str] = &["anthropic-version"];

/// Headers an Anthropic SDK sends that mean something to the API rather than
/// to the gateway: the feature betas the channel also writes into, and the
/// profile id the Console attaches (v3 `policy::CLAUDE_API`). A provider
/// allow-list narrows what *other* headers a client may add, not these.
pub const CLIENT_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &["anthropic-beta", "anthropic-user-profile-id"],
    prefixes: &[],
};

fn invalid_config(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidConfig(message.into())
}

pub(super) fn base_url(provider: ProviderView<'_>) -> String {
    provider
        .base_url
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE_URL)
        .trim_end_matches('/')
        .to_owned()
}

pub(super) fn api_key<'a>(secret: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    secret
        .get(name)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
}

/// Whether the operation is served by the OpenAI SDK compatibility layer.
fn is_chat(dialect: Dialect) -> bool {
    dialect.family() == WireFamily::OpenAi
}

fn unsupported(operation: Operation, dialect: Dialect) -> ChannelError {
    ChannelError::UnsupportedOperation(OperationKey { operation, dialect })
}

/// The route for an operation. The API mounts its own paths, so the client's
/// is only consulted for `GetModel`, where the model id lives nowhere else.
fn route(operation: Operation, dialect: Dialect, path: &str) -> Result<String, ChannelError> {
    if is_chat(dialect) {
        return match operation {
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                Ok("/v1/chat/completions".into())
            }
            other => Err(unsupported(other, dialect)),
        };
    }
    Ok(match operation {
        Operation::ListModels => "/v1/models".into(),
        Operation::GetModel => {
            let model = path
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .filter(|segment| !segment.is_empty())
                .ok_or_else(|| unsupported(operation, dialect))?;
            format!("/v1/models/{model}")
        }
        Operation::CountTokens => "/v1/messages/count_tokens".into(),
        Operation::GenerateContent | Operation::StreamGenerateContent => "/v1/messages".into(),
        other => return Err(unsupported(other, dialect)),
    })
}

/// The client's query minus any credential it carried, order preserved.
fn query(source: Option<String>) -> Option<String> {
    let kept = source
        .unwrap_or_default()
        .split('&')
        .filter(|pair| {
            !pair.is_empty() && !QUERY_AUTH.contains(&pair.split('=').next().unwrap_or_default())
        })
        .collect::<Vec<_>>()
        .join("&");
    (!kept.is_empty()).then_some(kept)
}

impl BaseChannel for Claudeapi {
    fn id(&self) -> &'static str {
        ID
    }

    /// An API key against Anthropic's own API: nothing to log into, rate
    /// limits on every reply and an organization cost report to query.
    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Claude API (Anthropic)",
            login_modes: vec![LoginMode::ApiKey],
            capabilities: ChannelCapabilities {
                refresh: false,
                quota_query: true,
                quota_reset: false,
                services: false,
                websocket: false,
            },
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Anthropic API origin; defaults to https://api.anthropic.com. Provider column, not config JSON.",
                ).with_placeholder(super::config::DEFAULT_BASE_URL),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static headers added to every upstream request.",
                ),
                ConfigKey::optional(
                    "enable_claude_magic_cache",
                    ConfigKeyKind::Bool,
                    "Turn a client's magic cache string in a Messages or count_tokens body into cache_control. The strings are stripped either way.",
                ),
                ConfigKey::optional(
                    "fallback_mode",
                    ConfigKeyKind::String,
                    "Server-side fallback for Messages requests that name none: off (default), default, or models.",
                ),
                ConfigKey::optional(
                    "fallback_models",
                    ConfigKeyKind::Json,
                    "The chain fallback_mode `models` installs; at most three are sent.",
                ),
                ConfigKey::optional(
                    "quota_base_url",
                    ConfigKeyKind::String,
                    "Origin of the organization cost report when it differs from base_url.",
                ).with_placeholder(super::config::QUOTA_DEFAULT_BASE_URL),
            ]
            .into_iter()
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    /// Claude natively, plus Chat Completions on Anthropic's OpenAI SDK
    /// compatibility layer, which serves conversations only.
    fn native_dialects(&self, _provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                vec![Dialect::Claude, Dialect::OpenAiChat]
            }
            Operation::ListModels | Operation::GetModel | Operation::CountTokens => {
                vec![Dialect::Claude]
            }
            _ => Vec::new(),
        }
    }

    /// A buffered Messages or count_tokens body is shaped; a Chat Completions
    /// body only loses its prefill and gains the streaming usage opt-in. A
    /// streamed request body passes through untouched, as in `claudecode`.
    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let config = ClaudeapiConfig::from_view(ctx.provider)?;
        let key =
            api_key(ctx.credential.secret, "api_key").ok_or(ChannelError::InvalidCredential)?;
        let operation = ctx.operation.operation;
        let dialect = ctx.operation.dialect;
        let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLIENT_HEADERS)?;
        let WireRequest {
            method,
            path,
            query: source_query,
            headers: source,
            body,
        } = ctx.request;

        let url = match ctx.endpoint_override {
            Some(url) => url.to_owned(),
            None => format!(
                "{}{}",
                base_url(ctx.provider),
                route(operation, dialect, &path)?
            ),
        };
        let uri = match query(source_query) {
            Some(q) if url.contains('?') => format!("{url}&{q}"),
            Some(q) => format!("{url}?{q}"),
            None => url,
        };

        let mut headers = forwardable(&source, allowlist.as_ref(), CHANNEL_HEADERS);
        let messages = matches!(
            operation,
            Operation::GenerateContent | Operation::StreamGenerateContent
        );
        let body = match body {
            HttpBody::Bytes(bytes) if is_chat(dialect) && messages => {
                let bytes = match hygiene::json_object(&bytes) {
                    Some(mut value) => {
                        hygiene::chat(&mut value);
                        Bytes::from(value.to_string())
                    }
                    None => bytes,
                };
                HttpBody::Bytes(if operation == Operation::StreamGenerateContent {
                    openai_wire::stream_usage_opt_in(bytes)
                } else {
                    bytes
                })
            }
            HttpBody::Bytes(bytes) if messages => match hygiene::json_object(&bytes) {
                Some(mut value) => {
                    hygiene::messages(
                        &mut value,
                        &mut headers,
                        config.enable_claude_magic_cache,
                        &config.fallback_mode,
                        &config.fallback_models,
                    );
                    HttpBody::Bytes(Bytes::from(value.to_string()))
                }
                None => HttpBody::Bytes(bytes),
            },
            HttpBody::Bytes(bytes) if operation == Operation::CountTokens => {
                if let Some(value) = hygiene::json_object(&bytes) {
                    hygiene::count_tokens(&value, &mut headers);
                }
                // Rewritten only when a magic cache string is present.
                HttpBody::Bytes(cache::shape(
                    bytes,
                    config
                        .enable_claude_magic_cache
                        .then_some(cache::Rules::Claude),
                ))
            }
            other => other,
        };

        headers.insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_str(key).map_err(|_| ChannelError::InvalidCredential)?,
        );
        headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static(ANTHROPIC_VERSION),
        );
        for (name, value) in &config.headers {
            headers.insert(
                HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| invalid_config(format!("header `{name}`")))?,
                HeaderValue::from_str(value)
                    .map_err(|_| invalid_config(format!("header `{name}`")))?,
            );
        }
        let mut builder = http::Request::builder().method(method).uri(uri);
        if let Some(map) = builder.headers_mut() {
            *map = headers;
        }
        builder
            .body(body)
            .map_err(|error| invalid_config(error.to_string()))
    }

    fn quota_headers(&self) -> Option<&dyn QuotaHeaders> {
        Some(self)
    }
    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }
    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        Some(self)
    }
    fn usage_stream(&self) -> Option<&dyn UsageStream> {
        Some(self)
    }
}
