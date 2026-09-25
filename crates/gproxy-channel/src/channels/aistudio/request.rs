//! URL layout, credential injection and header filtering for AI Studio.

use super::{Aistudio, AistudioConfig, DEFAULT_BASE_URL, ID, OPENAI_PREFIX};
use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ChannelHeaders, ConfigKey,
    ConfigKeyKind, HOST_CONFIG_KEYS, HeaderAllowlist, LoginMode, PrepareContext, ProviderView,
    UsageExtractor, UsageStream, forwardable,
};
use crate::channels::shared::openai_wire;
use gproxy_protocol::{Dialect, HttpBody, Operation, WireFamily, WireRequest};
use http::{HeaderName, HeaderValue, header};

/// The API key header of the native Gemini surface.
const GOOG_API_KEY: &str = "x-goog-api-key";
/// Query parameters that carry a client's own credential; never forwarded.
const QUERY_AUTH: &[&str] = &["access_token", "api_key", "key", "x-api-key"];
/// Headers this channel sets itself, so a client cannot supply them.
const CHANNEL_HEADERS: &[&str] = &[GOOG_API_KEY];

/// Headers a Google client sends that carry meaning to the upstream rather
/// than to the gateway: the resumable-upload protocol of the Files API and
/// ranged downloads (v3 `policy::AISTUDIO`). A provider allow-list narrows
/// what *other* headers a client may add; it must not strip these.
pub const CLIENT_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &["range"],
    prefixes: &["x-goog-upload-"],
};

/// Whether the operation's native dialect is served by the OpenAI
/// compatibility layer rather than by the Gemini methods.
fn is_openai(dialect: Dialect) -> bool {
    dialect.family() == WireFamily::OpenAi
}

/// The upstream path. Client paths already shaped for the native Gemini
/// methods are forwarded as they are; paths shaped for OpenAI's own layout
/// (`/v1/...`) move onto the compatibility prefix, where AI Studio actually
/// serves them. Video jobs are OpenAI-shaped even on the Gemini dialect,
/// because that is the only place AI Studio exposes them (v3 `prepare.rs`).
fn upstream_path(dialect: Dialect, operation: Operation, path: &str) -> String {
    let path = if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    };
    if is_openai(dialect) {
        return match path.strip_prefix("/v1/") {
            Some(rest) => format!("{OPENAI_PREFIX}/{rest}"),
            None => path,
        };
    }
    if matches!(
        operation,
        Operation::CreateVideo
            | Operation::RetrieveVideo
            | Operation::ListVideos
            | Operation::DeleteVideo
            | Operation::DownloadVideoContent
    ) && let Some(suffix) = path.strip_prefix("/v1/videos")
    {
        return format!("{OPENAI_PREFIX}/videos{suffix}");
    }
    path
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

impl Aistudio {
    fn build<B>(
        &self,
        ctx: PrepareContext<'_, B>,
    ) -> Result<(http::request::Builder, WireRequest<B>), ChannelError> {
        let config = AistudioConfig::from_view(ctx.provider)?;
        let key = ctx
            .credential
            .secret
            .get("api_key")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .ok_or(ChannelError::InvalidCredential)?;
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
                format!(
                    "{base}{}",
                    upstream_path(dialect, ctx.operation.operation, &request.path)
                )
            }
        };
        let uri = match query(request.query.take()) {
            Some(q) if url.contains('?') => format!("{url}&{q}"),
            Some(q) => format!("{url}?{q}"),
            None => url,
        };
        let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLIENT_HEADERS)?;
        let mut headers = forwardable(&request.headers, allowlist.as_ref(), CHANNEL_HEADERS);
        if is_openai(dialect) {
            headers.insert(
                header::AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {key}"))
                    .map_err(|_| ChannelError::InvalidCredential)?,
            );
        } else {
            headers.insert(
                HeaderName::from_static(GOOG_API_KEY),
                HeaderValue::from_str(key).map_err(|_| ChannelError::InvalidCredential)?,
            );
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

impl BaseChannel for Aistudio {
    fn id(&self) -> &'static str {
        ID
    }

    /// An API key against one origin serving two surfaces; nothing to log
    /// into, nothing to refresh, no account endpoint to read a balance from.
    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Google AI Studio",
            login_modes: vec![LoginMode::ApiKey],
            capabilities: ChannelCapabilities::default(),
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Gemini API origin; defaults to https://generativelanguage.googleapis.com. Provider column, not config JSON.",
                ).with_placeholder(super::config::DEFAULT_BASE_URL),
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

    /// Gemini natively, plus what the OpenAI compatibility layer covers:
    /// Chat Completions, the models list and embeddings. The layer speaks no
    /// Responses, so `Dialect::OpenAi` is only claimed where AI Studio's
    /// OpenAI-shaped routes are the whole surface (models, videos).
    fn native_dialects(&self, _provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                vec![Dialect::Gemini, Dialect::OpenAiChat]
            }
            Operation::ListModels | Operation::GetModel => vec![Dialect::Gemini, Dialect::OpenAi],
            Operation::CreateEmbedding => vec![Dialect::Gemini, Dialect::OpenAi],
            Operation::CountTokens
            | Operation::BatchCreateEmbedding
            | Operation::CreateImage
            | Operation::CreateFile
            | Operation::ListFiles
            | Operation::RetrieveFile
            | Operation::RetrieveFileContent
            | Operation::DeleteFile => vec![Dialect::Gemini],
            Operation::CreateVideo | Operation::RetrieveVideo => {
                vec![Dialect::Gemini, Dialect::OpenAi]
            }
            _ => Vec::new(),
        }
    }

    /// A buffered Chat Completions body asks for the closing usage chunk the
    /// compatibility layer only sends when told to; everything else, and any
    /// streamed request body, is forwarded as the client wrote it.
    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let chat = ctx.operation.dialect == Dialect::OpenAiChat
            && ctx.operation.operation == Operation::StreamGenerateContent;
        let (builder, request) = self.build(ctx)?;
        let body = match request.body {
            HttpBody::Bytes(bytes) if chat => {
                HttpBody::Bytes(openai_wire::stream_usage_opt_in(bytes))
            }
            other => other,
        };
        builder
            .body(body)
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        Some(self)
    }

    fn usage_stream(&self) -> Option<&dyn UsageStream> {
        Some(self)
    }
}
