//! URL layout, credential injection and header filtering for the OpenAI
//! platform, over both HTTP and the two WebSocket surfaces.

use super::{DEFAULT_BASE_URL, ID, OpenAi, OpenAiConfig};
use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ChannelHeaders, ConfigKey,
    ConfigKeyKind, HOST_CONFIG_KEYS, HeaderAllowlist, LoginMode, PrepareContext, ProviderView,
    QuotaHeaders, QuotaQuery, UsageExtractor, UsageStream, forwardable,
};
use crate::channels::shared::{cache, openai_wire};
use gproxy_protocol::{Dialect, HttpBody, Operation, WireRequest};
use http::{HeaderName, HeaderValue, header};

/// The betas a Responses WebSocket handshake declares (v3 `prepare.rs`).
pub const RESPONSES_WS_BETA: &str = "responses_websockets=2026-02-06";
pub const RESPONSES_MULTI_AGENT_BETA: &str = "responses_multi_agent=v1";
/// Query parameters that carry a client's own credential; never forwarded.
const QUERY_AUTH: &[&str] = &["access_token", "api_key", "key", "x-api-key"];

/// Headers an OpenAI SDK sends that select something upstream rather than
/// describing the gateway's client: the feature betas and the organization
/// and project a key may be scoped to (v3 `policy::OPENAI_API`). A provider
/// allow-list narrows what *other* headers a client may add, not these.
pub const CLIENT_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &["openai-beta", "openai-organization", "openai-project"],
    prefixes: &[],
};

fn invalid_config(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidConfig(message.into())
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

/// A Realtime handshake names either the call it is joining or the model it
/// is opening; `call_id` wins, because a live call has a model already. The
/// client's own `model` and `call_id` are re-appended at the end rather than
/// forwarded in place, so the last one the upstream reads is the chosen one.
fn realtime_query(source: Option<String>) -> Result<String, ChannelError> {
    let source = source.unwrap_or_default();
    let pairs = source.split('&').filter(|pair| !pair.is_empty());
    let call_id = pairs
        .clone()
        .find(|pair| pair.starts_with("call_id="))
        .map(str::to_owned)
        .filter(|pair| pair.len() > "call_id=".len());
    let model = pairs
        .clone()
        .find(|pair| pair.starts_with("model="))
        .map(str::to_owned)
        .filter(|pair| pair.len() > "model=".len());
    let mut kept = pairs
        .filter(|pair| {
            let name = pair.split('=').next().unwrap_or_default();
            !matches!(name, "model" | "call_id") && !QUERY_AUTH.contains(&name)
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    match call_id.or(model) {
        Some(selector) => kept.push(selector),
        None => {
            return Err(invalid_config(
                "a Realtime handshake needs a model or a call_id",
            ));
        }
    }
    Ok(kept.join("&"))
}

/// Whether the operation is served over a socket rather than over HTTP.
fn is_websocket(operation: Operation, dialect: Dialect) -> bool {
    operation == Operation::ConnectRealtime || dialect == Dialect::OpenAiResponsesWebSocket
}

impl OpenAi {
    fn build<B>(
        &self,
        ctx: PrepareContext<'_, B>,
        websocket: bool,
    ) -> Result<(http::request::Builder, WireRequest<B>), ChannelError> {
        let config = OpenAiConfig::from_view(ctx.provider)?;
        let key = ctx
            .credential
            .secret
            .get("api_key")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .ok_or(ChannelError::InvalidCredential)?;
        let operation = ctx.operation.operation;
        let dialect = ctx.operation.dialect;
        let mut request = ctx.request;
        let url = match ctx.endpoint_override {
            Some(url) => url.to_owned(),
            None => {
                let base = ctx
                    .provider
                    .base_url
                    .map(str::trim)
                    .filter(|base| !base.is_empty())
                    .unwrap_or(DEFAULT_BASE_URL)
                    .trim_end_matches('/');
                let path = if request.path.starts_with('/') {
                    request.path.clone()
                } else {
                    format!("/{}", request.path)
                };
                format!("{base}{path}")
            }
        };
        let url = if websocket {
            url.replacen("https://", "wss://", 1)
                .replacen("http://", "ws://", 1)
        } else {
            url
        };
        let source_query = request.query.take();
        let query = if operation == Operation::ConnectRealtime {
            Some(realtime_query(source_query)?)
        } else {
            query(source_query)
        };
        let uri = match query.filter(|q| !q.is_empty()) {
            Some(q) if url.contains('?') => format!("{url}&{q}"),
            Some(q) => format!("{url}?{q}"),
            None => url,
        };
        let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLIENT_HEADERS)?;
        let mut headers = forwardable(&request.headers, allowlist.as_ref(), &[]);
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {key}"))
                .map_err(|_| ChannelError::InvalidCredential)?,
        );
        if websocket && dialect == Dialect::OpenAiResponsesWebSocket {
            headers.append(
                HeaderName::from_static("openai-beta"),
                HeaderValue::from_static(RESPONSES_WS_BETA),
            );
            headers.append(
                HeaderName::from_static("openai-beta"),
                HeaderValue::from_static(RESPONSES_MULTI_AGENT_BETA),
            );
        }
        for (name, value) in &config.headers {
            headers.insert(
                HeaderName::from_bytes(name.as_bytes())
                    .map_err(|_| invalid_config(format!("header `{name}`")))?,
                HeaderValue::from_str(value)
                    .map_err(|_| invalid_config(format!("header `{name}`")))?,
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

impl BaseChannel for OpenAi {
    fn id(&self) -> &'static str {
        ID
    }

    /// An API key against OpenAI's own platform: nothing to log into, rate
    /// limits on every reply, an organization cost report to query, and
    /// Responses and Realtime over a socket as well as over HTTP.
    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "OpenAI",
            login_modes: vec![LoginMode::ApiKey],
            capabilities: ChannelCapabilities {
                refresh: false,
                quota_query: true,
                quota_reset: false,
                services: false,
                websocket: true,
            },
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "OpenAI API origin; defaults to https://api.openai.com. Provider column, not config JSON.",
                ),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static headers added to every upstream request.",
                ),
                ConfigKey::optional(
                    "enable_openai_magic_cache",
                    ConfigKeyKind::Bool,
                    "Turn a client's magic cache string in a Chat Completions or Responses body into prompt_cache_breakpoint. The strings are stripped either way.",
                ),
                ConfigKey::optional(
                    "quota_base_url",
                    ConfigKeyKind::String,
                    "Origin of the organization cost report when it differs from base_url.",
                ),
            ]
            .into_iter()
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    /// Everything OpenAI serves is native OpenAI; conversations additionally
    /// have the Chat Completions and the Responses-over-WebSocket shapes.
    fn native_dialects(&self, _provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent | Operation::StreamGenerateContent => vec![
                Dialect::OpenAi,
                Dialect::OpenAiChat,
                Dialect::OpenAiResponsesWebSocket,
            ],
            Operation::ListModels
            | Operation::GetModel
            | Operation::CompactContent
            | Operation::CreateEmbedding
            | Operation::CreateImage
            | Operation::EditImage
            | Operation::CreateSpeech
            | Operation::CreateTranscription
            | Operation::CreateTranslation
            | Operation::CreateFile
            | Operation::ListFiles
            | Operation::RetrieveFile
            | Operation::RetrieveFileContent
            | Operation::DeleteFile
            | Operation::CreateVideo
            | Operation::RetrieveVideo
            | Operation::ListVideos
            | Operation::DeleteVideo
            | Operation::DownloadVideoContent
            | Operation::ConnectRealtime => vec![Dialect::OpenAi],
            _ => Vec::new(),
        }
    }

    /// A buffered conversation body is shaped for the magic cache strings and,
    /// when it is a Chat Completions stream, asked to end with a usage chunk.
    /// A streamed request body passes through untouched.
    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let config = OpenAiConfig::from_view(ctx.provider)?;
        let rules = cache::rules_for(
            ctx.operation.dialect,
            false,
            config.enable_openai_magic_cache,
        );
        let chat = ctx.operation.dialect == Dialect::OpenAiChat
            && ctx.operation.operation == Operation::StreamGenerateContent;
        let (builder, request) = self.build(ctx, false)?;
        let body = match request.body {
            HttpBody::Bytes(bytes) => {
                let bytes = cache::shape(bytes, rules);
                HttpBody::Bytes(if chat {
                    openai_wire::stream_usage_opt_in(bytes)
                } else {
                    bytes
                })
            }
            other => other,
        };
        builder
            .body(body)
            .map_err(|error| invalid_config(error.to_string()))
    }

    fn prepare_connect(
        &self,
        ctx: PrepareContext<'_, ()>,
    ) -> Result<http::Request<()>, ChannelError> {
        if !is_websocket(ctx.operation.operation, ctx.operation.dialect) {
            return Err(ChannelError::WrongTransport(ctx.operation));
        }
        let (builder, _) = self.build(ctx, true)?;
        builder
            .body(())
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
