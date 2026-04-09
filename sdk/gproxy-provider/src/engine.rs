use std::pin::Pin;
use std::sync::Arc;

use async_stream::try_stream;
use bytes::Bytes;
use futures_util::Stream;
use futures_util::StreamExt;
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::Instrument;

use crate::Channel;
use crate::dispatch::RouteKey;
use crate::health::ModelCooldownHealth;
use crate::request::PreparedRequest;
use crate::response::UpstreamError;
use crate::store::{CredentialUpdate, ProviderStore, ProviderStoreBuilder};

fn is_stream_aggregation_route(
    src_operation: OperationFamily,
    dst_operation: OperationFamily,
    _src_protocol: ProtocolKind,
    _dst_protocol: ProtocolKind,
) -> bool {
    src_operation == OperationFamily::GenerateContent
        && dst_operation == OperationFamily::StreamGenerateContent
}

fn aggregate_stream_body(protocol: ProtocolKind, body: &[u8]) -> Result<Vec<u8>, UpstreamError> {
    let ndjson = match protocol {
        ProtocolKind::OpenAiResponse
        | ProtocolKind::OpenAiChatCompletion
        | ProtocolKind::Claude => {
            gproxy_protocol::stream::sse_to_ndjson_stream(&String::from_utf8_lossy(body))
        }
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
            String::from_utf8_lossy(body).into_owned()
        }
        _ => {
            return Err(UpstreamError::Channel(format!(
                "no stream aggregation for protocol: {protocol}"
            )));
        }
    };

    let owned_chunks: Vec<Vec<u8>> = ndjson
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.as_bytes().to_vec())
        .collect();
    let chunk_refs: Vec<&[u8]> = owned_chunks.iter().map(Vec::as_slice).collect();
    crate::transform_dispatch::stream_to_nonstream(protocol, &chunk_refs)
}

/// Execution request passed to the engine.
pub struct ExecuteRequest {
    pub provider: String,
    pub operation: OperationFamily,
    pub protocol: ProtocolKind,
    pub body: Vec<u8>,
    pub headers: http::HeaderMap,
    pub model: Option<String>,
    pub forced_credential_index: Option<usize>,
}

/// Result of an engine execution.
pub struct ExecuteResult {
    pub status: u16,
    pub headers: http::HeaderMap,
    pub body: ExecuteBody,
    pub usage: Option<Usage>,
    pub cost: Option<f64>,
    pub billing: Option<crate::billing::BillingResult>,
    pub billing_context: Option<crate::billing::BillingContext>,
    pub meta: Option<UpstreamRequestMeta>,
    pub credential_updates: Vec<CredentialUpdate>,
    pub credential_index: usize,
}

/// Engine execution error bundled with optional upstream-request log
/// metadata captured from the last attempt. Lets the caller record a
/// full upstream-request row on the error path — real URL, headers,
/// request body, response status, response headers, response body —
/// instead of the placeholder row the previous implementation wrote.
#[derive(Debug)]
pub struct ExecuteError {
    pub error: UpstreamError,
    pub meta: Option<UpstreamRequestMeta>,
    pub credential_index: Option<usize>,
}

impl ExecuteError {
    pub fn bare(error: UpstreamError) -> Self {
        Self {
            error,
            meta: None,
            credential_index: None,
        }
    }
}

impl From<UpstreamError> for ExecuteError {
    fn from(error: UpstreamError) -> Self {
        Self::bare(error)
    }
}

impl std::fmt::Display for ExecuteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for ExecuteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// Turn a `FailedUpstreamAttempt` from the retry layer into an
/// `ExecuteError` with an `UpstreamRequestMeta` suitable for the
/// upstream-request log. Respects `enable_upstream_log_body` — the
/// request/response bodies are dropped when body logging is off so the
/// caller never writes them to the DB.
fn build_execute_error(
    error: UpstreamError,
    failed_attempt: Option<crate::response::FailedUpstreamAttempt>,
    model: Option<String>,
    start: std::time::Instant,
    enable_upstream_log: bool,
    enable_upstream_log_body: bool,
) -> ExecuteError {
    let credential_index = failed_attempt.as_ref().and_then(|a| a.credential_index);
    let meta = if enable_upstream_log {
        failed_attempt.map(|a| UpstreamRequestMeta {
            method: a.method,
            url: a.url,
            request_headers: a.request_headers,
            request_body: if enable_upstream_log_body {
                a.request_body
            } else {
                None
            },
            response_status: a.response_status,
            response_headers: a.response_headers,
            response_body: if enable_upstream_log_body {
                a.response_body
            } else {
                None
            },
            model,
            latency_ms: start.elapsed().as_millis() as u64,
            credential_index,
        })
    } else {
        None
    };
    ExecuteError {
        error,
        meta,
        credential_index,
    }
}

pub type ExecuteBodyStream = Pin<Box<dyn Stream<Item = Result<Bytes, UpstreamError>> + Send>>;

pub enum ExecuteBody {
    Full(Vec<u8>),
    Stream(ExecuteBodyStream),
}

/// Token usage extracted from upstream response.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_input_tokens: Option<i64>,
    pub cache_creation_input_tokens: Option<i64>,
    pub cache_creation_input_tokens_5min: Option<i64>,
    pub cache_creation_input_tokens_1h: Option<i64>,
}

/// Metadata about the upstream request for logging/storage.
#[derive(Debug, Clone)]
pub struct UpstreamRequestMeta {
    pub method: String,
    pub url: String,
    pub request_headers: Vec<(String, String)>,
    pub request_body: Option<Vec<u8>>,
    pub response_status: Option<u16>,
    pub response_headers: Vec<(String, String)>,
    /// Raw upstream response body, captured before any cross-protocol
    /// transform or stream aggregation. Populated only when the engine is
    /// built with `enable_upstream_log_body = true`; otherwise `None`.
    pub response_body: Option<Vec<u8>>,
    pub model: Option<String>,
    pub latency_ms: u64,
    pub credential_index: Option<usize>,
}

/// The main SDK entry point. Consumes the current provider store snapshot and an HTTP client.
pub struct GproxyEngine {
    store: Arc<ProviderStore>,
    client: wreq::Client,
    spoof_client: Option<wreq::Client>,
    pub enable_usage: bool,
    pub enable_upstream_log: bool,
    pub enable_upstream_log_body: bool,
}

/// Serialized provider configuration for building an engine from JSON/DB data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub channel: String,
    pub settings_json: serde_json::Value,
    pub credentials: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch: Option<crate::dispatch::DispatchTableDocument>,
}

pub fn built_in_model_prices(channel: &str) -> Option<Vec<crate::billing::ModelPrice>> {
    use crate::channels::*;

    let prices = match channel {
        "openai" => openai::OpenAiChannel.model_pricing(),
        "anthropic" => anthropic::AnthropicChannel.model_pricing(),
        "claudecode" => claudecode::ClaudeCodeChannel.model_pricing(),
        "codex" => codex::CodexChannel.model_pricing(),
        "vertex" => vertex::VertexChannel.model_pricing(),
        "vertexexpress" => vertexexpress::VertexExpressChannel.model_pricing(),
        "aistudio" => aistudio::AiStudioChannel.model_pricing(),
        "geminicli" => geminicli::GeminiCliChannel.model_pricing(),
        "antigravity" => antigravity::AntigravityChannel.model_pricing(),
        "nvidia" => nvidia::NvidiaChannel.model_pricing(),
        "deepseek" => deepseek::DeepSeekChannel.model_pricing(),
        "groq" => groq::GroqChannel.model_pricing(),
        "openrouter" => openrouter::OpenRouterChannel.model_pricing(),
        "custom" => custom::CustomChannel.model_pricing(),
        _ => return None,
    };
    Some(prices.to_vec())
}

/// Validate that a JSON credential matches the schema for a channel.
pub fn validate_credential_json(channel: &str, credential: &Value) -> Result<(), UpstreamError> {
    macro_rules! validate {
        ($ty:ty) => {
            serde_json::from_value::<$ty>(credential.clone())
                .map(|_| ())
                .map_err(|e| {
                    UpstreamError::Channel(format!(
                        "invalid credential for channel '{channel}': {e}"
                    ))
                })
        };
    }

    use crate::channels::*;

    match channel {
        "openai" => validate!(openai::OpenAiCredential),
        "anthropic" => validate!(anthropic::AnthropicCredential),
        "claudecode" => validate!(claudecode::ClaudeCodeCredential),
        "codex" => validate!(codex::CodexCredential),
        "vertex" => validate!(vertex::VertexCredential),
        "vertexexpress" => validate!(vertexexpress::VertexExpressCredential),
        "aistudio" => validate!(aistudio::AiStudioCredential),
        "geminicli" => validate!(geminicli::GeminiCliCredential),
        "antigravity" => validate!(antigravity::AntigravityCredential),
        "nvidia" => validate!(nvidia::NvidiaCredential),
        "deepseek" => validate!(deepseek::DeepSeekCredential),
        "groq" => validate!(groq::GroqCredential),
        "openrouter" => validate!(openrouter::OpenRouterCredential),
        "custom" => validate!(custom::CustomCredential),
        _ => Err(UpstreamError::Channel(format!(
            "unknown channel: {channel}"
        ))),
    }
}

pub struct GproxyEngineBuilder {
    store: Option<Arc<ProviderStore>>,
    store_builder: ProviderStoreBuilder,
    client: Option<wreq::Client>,
    spoof_client: Option<wreq::Client>,
    enable_usage: bool,
    enable_upstream_log: bool,
    enable_upstream_log_body: bool,
}

impl GproxyEngineBuilder {
    pub fn new() -> Self {
        Self {
            store: None,
            store_builder: ProviderStoreBuilder::new(),
            client: None,
            spoof_client: None,
            enable_usage: true,
            enable_upstream_log: true,
            enable_upstream_log_body: true,
        }
    }

    pub fn provider_store(mut self, store: Arc<ProviderStore>) -> Self {
        self.store = Some(store);
        self
    }

    pub fn add_provider<C: crate::Channel>(
        self,
        name: impl Into<String>,
        channel: C,
        settings: C::Settings,
        credentials: Vec<(C::Credential, C::Health)>,
    ) -> Self {
        self.add_provider_with_dispatch(name, channel, settings, credentials, None)
    }

    pub fn add_provider_with_dispatch<C: crate::Channel>(
        mut self,
        name: impl Into<String>,
        channel: C,
        settings: C::Settings,
        credentials: Vec<(C::Credential, C::Health)>,
        dispatch_override: Option<crate::dispatch::DispatchTable>,
    ) -> Self {
        self.store_builder = self.store_builder.add_provider_with_dispatch(
            name,
            channel,
            settings,
            credentials,
            dispatch_override,
        );
        self
    }

    /// Set the HTTP client.
    pub fn http_client(mut self, client: wreq::Client) -> Self {
        self.client = Some(client);
        self
    }

    /// Set the spoof HTTP client (browser-impersonating TLS fingerprint).
    pub fn spoof_client(mut self, client: wreq::Client) -> Self {
        self.spoof_client = Some(client);
        self
    }

    /// Build HTTP clients from proxy and impersonate config.
    ///
    /// Constructs both the normal client (with optional proxy) and the
    /// spoof client (with browser TLS impersonation + optional proxy).
    pub fn configure_clients(self, proxy: Option<&str>, emulation: Option<&str>) -> Self {
        let mut client_builder = wreq::Client::builder();
        if let Some(proxy_url) = proxy
            && let Ok(p) = wreq::Proxy::all(proxy_url)
        {
            client_builder = client_builder.proxy(p);
        }
        let client = client_builder.build().unwrap_or_default();

        let emu = parse_emulation(emulation.unwrap_or("chrome_136"));
        let mut spoof_builder = wreq::Client::builder().emulation(emu);
        if let Some(proxy_url) = proxy
            && let Ok(p) = wreq::Proxy::all(proxy_url)
        {
            spoof_builder = spoof_builder.proxy(p);
        }
        let spoof = spoof_builder.build().unwrap_or_default();

        self.http_client(client).spoof_client(spoof)
    }

    /// Control whether usage is extracted from responses (default: true).
    pub fn enable_usage(mut self, enabled: bool) -> Self {
        self.enable_usage = enabled;
        self
    }

    /// Control whether upstream request metadata is collected (default: true).
    pub fn enable_upstream_log(mut self, enabled: bool) -> Self {
        self.enable_upstream_log = enabled;
        self
    }

    /// Control whether upstream log includes request/response body (default: true).
    pub fn enable_upstream_log_body(mut self, enabled: bool) -> Self {
        self.enable_upstream_log_body = enabled;
        self
    }

    pub fn build(self) -> GproxyEngine {
        GproxyEngine {
            store: self
                .store
                .unwrap_or_else(|| Arc::new(self.store_builder.build())),
            client: self.client.unwrap_or_default(),
            spoof_client: self.spoof_client,
            enable_usage: self.enable_usage,
            enable_upstream_log: self.enable_upstream_log,
            enable_upstream_log_body: self.enable_upstream_log_body,
        }
    }

    /// Add a provider from serialized JSON config.
    ///
    /// Dispatches by `channel` string to the concrete channel type.
    /// Returns an error if the channel ID is unknown or JSON is invalid.
    pub fn add_provider_json(self, config: ProviderConfig) -> Result<Self, UpstreamError> {
        macro_rules! add {
            ($self:expr, $ch:expr, $cfg:expr) => {{
                let crate::engine::ProviderConfig {
                    name,
                    settings_json,
                    credentials,
                    dispatch,
                    ..
                } = $cfg;
                let dispatch = match dispatch {
                    Some(document) => Some(
                        crate::dispatch::DispatchTable::from_document(document).map_err(|e| {
                            UpstreamError::Channel(format!("invalid dispatch for '{}': {e}", name))
                        })?,
                    ),
                    None => None,
                };
                let settings = serde_json::from_value(settings_json).map_err(|e| {
                    UpstreamError::Channel(format!("invalid settings for '{}': {e}", name))
                })?;
                let creds: Vec<_> = credentials
                    .into_iter()
                    .filter_map(|c| {
                        serde_json::from_value(c)
                            .ok()
                            .map(|c| (c, ModelCooldownHealth::default()))
                    })
                    .collect();
                Ok($self.add_provider_with_dispatch(&name, $ch, settings, creds, dispatch))
            }};
        }

        use crate::channels::*;

        match config.channel.as_str() {
            "openai" => add!(self, openai::OpenAiChannel, config),
            "anthropic" => add!(self, anthropic::AnthropicChannel, config),
            "claudecode" => add!(self, claudecode::ClaudeCodeChannel, config),
            "codex" => add!(self, codex::CodexChannel, config),
            "vertex" => add!(self, vertex::VertexChannel, config),
            "vertexexpress" => add!(self, vertexexpress::VertexExpressChannel, config),
            "aistudio" => add!(self, aistudio::AiStudioChannel, config),
            "geminicli" => add!(self, geminicli::GeminiCliChannel, config),
            "antigravity" => add!(self, antigravity::AntigravityChannel, config),
            "nvidia" => add!(self, nvidia::NvidiaChannel, config),
            "deepseek" => add!(self, deepseek::DeepSeekChannel, config),
            "groq" => add!(self, groq::GroqChannel, config),
            "openrouter" => add!(self, openrouter::OpenRouterChannel, config),
            "custom" => add!(self, custom::CustomChannel, config),
            _ => Err(UpstreamError::Channel(format!(
                "unknown channel: {}",
                config.channel
            ))),
        }
    }
}

impl Default for GproxyEngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl GproxyEngine {
    pub fn builder() -> GproxyEngineBuilder {
        GproxyEngineBuilder::new()
    }

    pub fn store(&self) -> &Arc<ProviderStore> {
        &self.store
    }

    pub fn estimate_billing(
        &self,
        provider_name: &str,
        context: &crate::billing::BillingContext,
        usage: &Usage,
    ) -> Option<crate::billing::BillingResult> {
        self.store.estimate_billing(provider_name, context, usage)
    }

    /// Create a new engine with different HTTP clients but the same provider store.
    ///
    /// Use this when proxy or spoof settings change without needing to
    /// rebuild providers/credentials.
    pub fn with_new_clients(&self, proxy: Option<&str>, emulation: Option<&str>) -> GproxyEngine {
        let builder = GproxyEngineBuilder::new()
            .provider_store(self.store.clone())
            .configure_clients(proxy, emulation)
            .enable_usage(self.enable_usage)
            .enable_upstream_log(self.enable_upstream_log)
            .enable_upstream_log_body(self.enable_upstream_log_body);
        builder.build()
    }

    /// Rebuild engine with new settings but same provider store.
    ///
    /// Use when global config changes (proxy, spoof, usage/log flags).
    pub fn with_settings(
        &self,
        proxy: Option<&str>,
        emulation: Option<&str>,
        enable_usage: bool,
        enable_upstream_log: bool,
        enable_upstream_log_body: bool,
    ) -> GproxyEngine {
        let builder = GproxyEngineBuilder::new()
            .provider_store(self.store.clone())
            .configure_clients(proxy, emulation)
            .enable_usage(enable_usage)
            .enable_upstream_log(enable_upstream_log)
            .enable_upstream_log_body(enable_upstream_log_body);
        builder.build()
    }

    /// Connect to an upstream WebSocket endpoint for a provider.
    ///
    /// Returns `Connected` for passthrough (same protocol), `NeedsProtocolBridge`
    /// when the dispatch table maps to a different WS operation, or an error
    /// (e.g. 426, no WS support) that the caller can use to fall back to HTTP.
    pub async fn connect_upstream_ws(
        &self,
        provider_name: &str,
        operation: OperationFamily,
        protocol: ProtocolKind,
        path: &str,
        model: Option<&str>,
    ) -> Result<WsConnectionResult, UpstreamError> {
        let span =
            tracing::info_span!("engine.connect_upstream_ws", provider = provider_name, path);
        async {
            let provider = self.store.get_runtime(provider_name).ok_or_else(|| {
                UpstreamError::Channel(format!("unknown provider: {provider_name}"))
            })?;

            // Check dispatch table to determine WS routing strategy
            let route_key = crate::dispatch::RouteKey::new(operation, protocol);
            let (ws_path, ws_model, src_protocol, dst_protocol) =
                match provider.dispatch_table().resolve(&route_key) {
                    Some(crate::dispatch::RouteImplementation::Passthrough) => {
                        (path.to_string(), model, protocol, protocol)
                    }
                    Some(crate::dispatch::RouteImplementation::TransformTo { destination }) => {
                        // Check if destination is also a WS operation
                        let dst_op = &destination.operation;
                        let dst_proto = &destination.protocol;
                        let (target_path, target_model) = ws_path_for_operation(dst_op, dst_proto, model);
                        match target_path {
                            Some(p) => (p, target_model, protocol, *dst_proto),
                            None => {
                                return Err(UpstreamError::Channel(
                                    "upstream does not support native WebSocket for this operation; use HTTP fallback".into(),
                                ));
                            }
                        }
                    }
                    _ => {
                        return Err(UpstreamError::Channel(
                            "upstream does not support native WebSocket for this operation; use HTTP fallback".into(),
                        ));
                    }
                };

            // Get auth candidates for all credentials
            let auth_candidates = provider.prepare_ws_auth(&ws_path, ws_model)?;

            let mut last_error = None;
            for (idx, (auth_url, auth_headers)) in auth_candidates.into_iter().enumerate() {
                // Convert URL scheme to wss/ws
                let ws_url = auth_url
                    .replace("https://", "wss://")
                    .replace("http://", "ws://");

                // Append model query param if not already in the URL
                let ws_url = if let Some(m) = ws_model
                    && !ws_url.contains("model=")
                {
                    let sep = if ws_url.contains('?') { "&" } else { "?" };
                    format!("{ws_url}{sep}model={m}")
                } else {
                    ws_url
                };

                tracing::info!(url = %ws_url, credential = idx, "connecting upstream websocket");

                // Build WS request with channel-specific auth headers
                let mut ws_builder = wreq::websocket(&ws_url);
                for (name, value) in auth_headers.iter() {
                    if name != http::header::CONTENT_TYPE {
                        ws_builder =
                            ws_builder.header(name.as_str(), value.to_str().unwrap_or(""));
                    }
                }

                let response = match ws_builder.send().await {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!(credential = idx, error = %e, "ws handshake failed, trying next credential");
                        last_error = Some(UpstreamError::Http(format!("ws handshake failed: {e}")));
                        continue;
                    }
                };

                let status = response.status().as_u16();
                if status == 426 {
                    return Err(UpstreamError::Channel(
                        "upstream requires HTTP (426 Upgrade Required)".into(),
                    ));
                }
                if status == 401 || status == 403 {
                    tracing::warn!(credential = idx, status, "ws auth rejected, trying next credential");
                    last_error = Some(UpstreamError::Channel(format!(
                        "ws auth rejected (HTTP {status})"
                    )));
                    continue;
                }

                let ws = match response.into_websocket().await {
                    Ok(ws) => ws,
                    Err(e) => {
                        tracing::warn!(credential = idx, error = %e, "ws upgrade failed, trying next credential");
                        last_error =
                            Some(UpstreamError::Http(format!("ws upgrade failed: {e}")));
                        continue;
                    }
                };

                tracing::info!(credential = idx, "upstream websocket connected");
                let upstream = UpstreamWebSocket { inner: ws };
                let meta = WsUpstreamMeta {
                    url: ws_url,
                    request_headers: auth_headers
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                        .collect(),
                    response_status: status,
                    credential_index: idx,
                };

                return if src_protocol == dst_protocol {
                    Ok(WsConnectionResult::Connected(upstream, meta))
                } else {
                    Ok(WsConnectionResult::NeedsProtocolBridge {
                        upstream,
                        src_protocol,
                        dst_protocol,
                        meta,
                    })
                };
            }

            Err(last_error.unwrap_or(UpstreamError::AllCredentialsExhausted))
        }
        .instrument(span)
        .await
    }

    /// Start an OAuth flow for a provider.
    pub async fn oauth_start(
        &self,
        provider_name: &str,
        params: std::collections::HashMap<String, String>,
    ) -> Result<Option<crate::channel::OAuthFlow>, UpstreamError> {
        let span = tracing::info_span!("engine.oauth_start", provider = provider_name);
        async {
            self.store
                .oauth_start(provider_name, &self.client, params)
                .await
        }
        .instrument(span)
        .await
    }

    /// Finish an OAuth flow (exchange code for tokens).
    pub async fn oauth_finish(
        &self,
        provider_name: &str,
        params: std::collections::HashMap<String, String>,
    ) -> Result<Option<crate::store::OAuthFinishResult>, UpstreamError> {
        let span = tracing::info_span!("engine.oauth_finish", provider = provider_name);
        async {
            self.store
                .oauth_finish(provider_name, &self.client, params)
                .await
        }
        .instrument(span)
        .await
    }

    /// Query upstream quota/usage for a provider credential.
    pub async fn query_quota(
        &self,
        provider_name: &str,
        credential_index: Option<usize>,
    ) -> Result<Option<crate::response::UpstreamResponse>, UpstreamError> {
        let span = tracing::info_span!("engine.query_quota", provider = provider_name);
        async {
            let provider = self.store.get_runtime(provider_name).ok_or_else(|| {
                UpstreamError::Channel(format!("unknown provider: {provider_name}"))
            })?;
            let Some(http_request) = provider.prepare_quota_request(credential_index)? else {
                return Ok(None);
            };
            let response = crate::http_client::send_request(&self.client, http_request).await?;
            Ok(Some(response))
        }
        .instrument(span)
        .await
    }

    /// Execute a request against a named provider.
    pub async fn execute(&self, request: ExecuteRequest) -> Result<ExecuteResult, ExecuteError> {
        let span = tracing::info_span!(
            "engine.execute",
            provider = %request.provider,
            operation = %request.operation,
            protocol = %request.protocol,
            model = request.model.as_deref().unwrap_or(""),
        );
        if request.operation.is_stream() {
            self.execute_stream_inner(request).instrument(span).await
        } else {
            self.execute_inner(request).instrument(span).await
        }
    }

    async fn execute_inner(&self, request: ExecuteRequest) -> Result<ExecuteResult, ExecuteError> {
        let provider = self.store.get_runtime(&request.provider).ok_or_else(|| {
            tracing::warn!(provider = %request.provider, "unknown provider");
            ExecuteError::bare(UpstreamError::Channel(format!(
                "unknown provider: {}",
                request.provider
            )))
        })?;

        let start = std::time::Instant::now();

        // Dispatch table lookup
        let src_key = crate::dispatch::RouteKey::new(request.operation, request.protocol);
        let route = provider
            .dispatch_table()
            .resolve(&src_key)
            .ok_or_else(|| {
                tracing::warn!(operation = %request.operation, protocol = %request.protocol, "route not found");
                UpstreamError::Channel(format!(
                    "unsupported route: ({}, {})",
                    request.operation, request.protocol
                ))
            })?
            .clone();

        let (dst_op, dst_proto, needs_transform) = match &route {
            crate::dispatch::RouteImplementation::Passthrough => {
                (request.operation, request.protocol, false)
            }
            crate::dispatch::RouteImplementation::TransformTo { destination } => {
                (destination.operation, destination.protocol, true)
            }
            crate::dispatch::RouteImplementation::Local => {
                let body = provider
                    .handle_local(request.operation, request.protocol, &request.body)
                    .unwrap_or_else(|| {
                        Err(UpstreamError::Channel("local route not implemented".into()))
                    })?;
                return Ok(ExecuteResult {
                    status: 200,
                    headers: http::HeaderMap::new(),
                    body: ExecuteBody::Full(body),
                    usage: None,
                    cost: None,
                    billing: None,
                    billing_context: None,
                    meta: None,
                    credential_updates: Vec::new(),
                    credential_index: 0,
                });
            }
            crate::dispatch::RouteImplementation::Unsupported => {
                return Err(ExecuteError::bare(UpstreamError::Channel(format!(
                    "unsupported: ({}, {})",
                    request.operation, request.protocol
                ))));
            }
        };

        let force_stream_aggregation =
            is_stream_aggregation_route(request.operation, dst_op, request.protocol, dst_proto);

        // Transform request if needed
        let body = if needs_transform {
            tracing::debug!(dst_op = %dst_op, dst_proto = %dst_proto, "transforming request");
            crate::transform_dispatch::transform_request(
                request.operation,
                request.protocol,
                if force_stream_aggregation {
                    request.operation
                } else {
                    dst_op
                },
                dst_proto,
                request.body,
            )?
        } else {
            request.body
        };

        let method = operation_http_method(dst_op);
        let mut body = body;

        // Suffix processing: match protocol-level + channel-specific suffix groups
        let proto_groups = crate::suffix::suffix_groups_for_protocol(dst_proto);
        let channel_groups = provider.model_suffix_groups();
        let matched = request.model.as_ref().and_then(|model| {
            crate::suffix::match_suffix_groups_combined(model, proto_groups, channel_groups)
        });
        let (model, suffix_str) = if let Some(ref m) = matched {
            crate::suffix::strip_model_suffix_in_body(&mut body, &m.base_model);
            (Some(m.base_model.clone()), Some(m.combined_suffix.clone()))
        } else {
            (request.model.clone(), None)
        };
        let mut prepared = PreparedRequest {
            method,
            route: RouteKey::new(dst_op, dst_proto),
            model,
            body,
            headers: request.headers,
        };
        if let Some(ref m) = matched {
            for apply_fn in &m.apply_fns {
                apply_fn(&mut prepared);
            }
        }
        let prepared = provider.finalize_request(prepared)?;
        let affinity_hint = crate::affinity::cache_affinity_hint_for_request(dst_proto, &prepared);

        let forced_credential = request.forced_credential_index;

        let provider_outcome = provider
            .execute(
                prepared.clone(),
                affinity_hint,
                forced_credential,
                &self.client,
                self.spoof_client.as_ref(),
            )
            .await;
        let provider_result = match provider_outcome.inner {
            Ok(r) => r,
            Err(error) => {
                return Err(build_execute_error(
                    error,
                    provider_outcome.failed_attempt,
                    prepared.model.clone(),
                    start,
                    self.enable_upstream_log,
                    self.enable_upstream_log_body,
                ));
            }
        };
        let response = provider_result.response;
        let credential_updates = provider_result.credential_updates;
        let used_credential_index = provider_result.credential_index;
        let attempt_meta = provider_result.attempt_meta;

        // Capture the raw upstream response body before any normalization
        // or cross-protocol transform, so the upstream-request log shows
        // what actually came over the wire. Only retained when body
        // logging is enabled to avoid the per-request clone.
        let raw_response_body_for_log = if self.enable_upstream_log
            && self.enable_upstream_log_body
        {
            Some(response.body.clone())
        } else {
            None
        };

        // 1. Normalize upstream response (channel-specific fixups)
        let normalized_body = provider.normalize_response(&prepared, response.body);
        let response_transform_dst_op = if force_stream_aggregation {
            request.operation
        } else {
            dst_op
        };
        let normalized_nonstream_body =
            if force_stream_aggregation && (200..=299).contains(&response.status) {
                aggregate_stream_body(dst_proto, &normalized_body)?
            } else {
                normalized_body
            };

        // 2. Extract usage from normalized upstream body (before protocol transform)
        let usage = if self.enable_usage {
            crate::usage::extract_usage(dst_proto, &normalized_nonstream_body)
        } else {
            None
        };
        let billing_context = provider.build_billing_context(&prepared);
        let billing = usage.as_ref().and_then(|usage| {
            billing_context
                .as_ref()
                .and_then(|context| provider.estimate_billing(context, usage))
        });
        let cost = billing.as_ref().map(|billing| billing.total_cost);

        // 2.5. Suffix response rewriting: append suffix to model field
        let mut normalized_nonstream_body = normalized_nonstream_body;
        if let Some(ref suffix) = suffix_str {
            crate::suffix::rewrite_model_suffix_in_body(&mut normalized_nonstream_body, suffix);
        }

        // 3. Transform response if needed (cross-protocol).
        //
        // Only transform on 2xx success bodies. Upstream error bodies
        // (e.g. codex returning `{"detail":{"code":"deactivated_workspace"}}`
        // on HTTP 402) are in a provider-specific error schema that the
        // destination-protocol wrapper enum can't parse. Attempting to
        // transform them produces
        // `"body does not match success or error variant of ..."` and
        // loses the upstream error information. Instead, forward the raw
        // error body through to the client — the upstream HTTP status
        // propagates via `response.status` below.
        //
        // Additionally: when `force_stream_aggregation` maps to the same
        // source protocol (e.g. codex `(GenerateContent, OpenAiResponse)`
        // upgraded to `(StreamGenerateContent, OpenAiResponse)`), the
        // stream-to-nonstream aggregation already produces a body in the
        // client's target shape. Running a further protocol transform
        // with `src == dst` has no matching arm and would error out, so
        // skip it.
        let needs_response_transform = needs_transform
            && (200..=299).contains(&response.status)
            && !(request.protocol == dst_proto
                && response_transform_dst_op == request.operation);
        let response_body = if needs_response_transform {
            tracing::debug!("transforming response");
            crate::transform_dispatch::transform_response(
                request.operation,
                request.protocol,
                response_transform_dst_op,
                dst_proto,
                normalized_nonstream_body,
            )?
        } else {
            normalized_nonstream_body
        };

        let latency_ms = start.elapsed().as_millis() as u64;

        let meta = if self.enable_upstream_log {
            let request_body_for_log = if self.enable_upstream_log_body {
                attempt_meta.request_body
            } else {
                None
            };
            Some(UpstreamRequestMeta {
                method: attempt_meta.method,
                url: attempt_meta.url,
                request_headers: attempt_meta.request_headers,
                request_body: request_body_for_log,
                response_status: Some(response.status),
                response_headers: response
                    .headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect(),
                response_body: raw_response_body_for_log,
                model: request.model,
                latency_ms,
                credential_index: Some(used_credential_index),
            })
        } else {
            None
        };

        Ok(ExecuteResult {
            status: response.status,
            headers: response.headers,
            body: ExecuteBody::Full(response_body),
            usage,
            cost,
            billing,
            billing_context,
            meta,
            credential_updates,
            credential_index: used_credential_index,
        })
    }

    async fn execute_stream_inner(
        &self,
        request: ExecuteRequest,
    ) -> Result<ExecuteResult, ExecuteError> {
        let provider = self.store.get_runtime(&request.provider).ok_or_else(|| {
            tracing::warn!(provider = %request.provider, "unknown provider");
            ExecuteError::bare(UpstreamError::Channel(format!(
                "unknown provider: {}",
                request.provider
            )))
        })?;

        let start = std::time::Instant::now();

        let src_key = crate::dispatch::RouteKey::new(request.operation, request.protocol);
        let route = provider
            .dispatch_table()
            .resolve(&src_key)
            .ok_or_else(|| {
                tracing::warn!(operation = %request.operation, protocol = %request.protocol, "route not found");
                UpstreamError::Channel(format!(
                    "unsupported route: ({}, {})",
                    request.operation, request.protocol
                ))
            })?
            .clone();

        let (dst_op, dst_proto, needs_transform) = match &route {
            crate::dispatch::RouteImplementation::Passthrough => {
                (request.operation, request.protocol, false)
            }
            crate::dispatch::RouteImplementation::TransformTo { destination } => {
                (destination.operation, destination.protocol, true)
            }
            crate::dispatch::RouteImplementation::Local => {
                let body = provider
                    .handle_local(request.operation, request.protocol, &request.body)
                    .unwrap_or_else(|| {
                        Err(UpstreamError::Channel("local route not implemented".into()))
                    })?;
                return Ok(ExecuteResult {
                    status: 200,
                    headers: http::HeaderMap::new(),
                    body: ExecuteBody::Full(body),
                    usage: None,
                    cost: None,
                    billing: None,
                    billing_context: None,
                    meta: None,
                    credential_updates: Vec::new(),
                    credential_index: 0,
                });
            }
            crate::dispatch::RouteImplementation::Unsupported => {
                return Err(ExecuteError::bare(UpstreamError::Channel(format!(
                    "unsupported: ({}, {})",
                    request.operation, request.protocol
                ))));
            }
        };

        let body = if needs_transform {
            crate::transform_dispatch::transform_request(
                request.operation,
                request.protocol,
                dst_op,
                dst_proto,
                request.body,
            )?
        } else {
            request.body
        };

        let method = operation_http_method(dst_op);
        let mut body = body;

        // Suffix processing: match protocol-level + channel-specific suffix groups
        let proto_groups = crate::suffix::suffix_groups_for_protocol(dst_proto);
        let channel_groups = provider.model_suffix_groups();
        let matched = request.model.as_ref().and_then(|model| {
            crate::suffix::match_suffix_groups_combined(model, proto_groups, channel_groups)
        });
        let (model, suffix_str) = if let Some(ref m) = matched {
            crate::suffix::strip_model_suffix_in_body(&mut body, &m.base_model);
            (Some(m.base_model.clone()), Some(m.combined_suffix.clone()))
        } else {
            (request.model.clone(), None)
        };
        let mut prepared = PreparedRequest {
            method,
            route: RouteKey::new(dst_op, dst_proto),
            model,
            body,
            headers: request.headers,
        };
        if let Some(ref m) = matched {
            for apply_fn in &m.apply_fns {
                apply_fn(&mut prepared);
            }
        }
        let prepared = provider.finalize_request(prepared)?;
        let affinity_hint = crate::affinity::cache_affinity_hint_for_request(dst_proto, &prepared);

        let forced_credential = request.forced_credential_index;

        let provider_outcome = provider
            .execute_stream(
                prepared.clone(),
                affinity_hint,
                forced_credential,
                &self.client,
                self.spoof_client.as_ref(),
            )
            .await;
        let provider_result = match provider_outcome.inner {
            Ok(r) => r,
            Err(error) => {
                return Err(build_execute_error(
                    error,
                    provider_outcome.failed_attempt,
                    prepared.model.clone(),
                    start,
                    self.enable_upstream_log,
                    self.enable_upstream_log_body,
                ));
            }
        };
        let response = provider_result.response;
        let credential_updates = provider_result.credential_updates;
        let used_credential_index = provider_result.credential_index;
        let attempt_meta = provider_result.attempt_meta;

        let meta = if self.enable_upstream_log {
            let request_body_for_log = if self.enable_upstream_log_body {
                attempt_meta.request_body
            } else {
                None
            };
            Some(UpstreamRequestMeta {
                method: attempt_meta.method,
                url: attempt_meta.url,
                request_headers: attempt_meta.request_headers,
                request_body: request_body_for_log,
                response_status: Some(response.status),
                response_headers: response
                    .headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect(),
                // Stream bodies cannot be captured here without consuming
                // the stream. Leaving as None — the stream contents are
                // forwarded to the client and not retained.
                response_body: None,
                model: request.model.clone(),
                latency_ms: start.elapsed().as_millis() as u64,
                credential_index: Some(used_credential_index),
            })
        } else {
            None
        };

        let body = if needs_transform {
            let transformer = crate::transform_dispatch::create_stream_response_transformer(
                request.operation,
                request.protocol,
                dst_op,
                dst_proto,
                Some({
                    let store = self.store.clone();
                    let provider_name = request.provider.clone();
                    let prepared = prepared.clone();
                    Arc::new(move |body: Vec<u8>| {
                        store
                            .get_runtime(&provider_name)
                            .map(|runtime| runtime.normalize_response(&prepared, body.clone()))
                            .unwrap_or(body)
                    })
                }),
            )?;

            let mut upstream = response.body;
            let suffix_to_rewrite = suffix_str.clone();
            // Helper that pins the try_stream's Ok type so the macro can
            // infer its error type from the `?` uses below. Without this,
            // the outer fn's new `ExecuteError` return type confuses
            // inference and the macro can't deduce `Result<Bytes, _>`.
            fn typed_stream(
                s: impl Stream<Item = Result<Bytes, UpstreamError>> + Send + 'static,
            ) -> ExecuteBodyStream {
                Box::pin(s)
            }
            let stream = typed_stream(try_stream! {
                let mut transformer = transformer;
                while let Some(chunk) = upstream.next().await {
                    let chunk = chunk?;
                    let mut out = transformer.push_chunk(&chunk)?;
                    if let Some(ref suffix) = suffix_to_rewrite {
                        crate::suffix::rewrite_model_suffix_in_body(&mut out, suffix);
                    }
                    if !out.is_empty() {
                        yield Bytes::from(out);
                    }
                }

                let mut tail = transformer.finish()?;
                if let Some(ref suffix) = suffix_to_rewrite {
                    crate::suffix::rewrite_model_suffix_in_body(&mut tail, suffix);
                }
                if !tail.is_empty() {
                    yield Bytes::from(tail);
                }
            });
            ExecuteBody::Stream(stream)
        } else if let Some(ref suffix) = suffix_str {
            // Passthrough with suffix rewriting
            let suffix = suffix.clone();
            let mut upstream = response.body;
            fn typed_stream(
                s: impl Stream<Item = Result<Bytes, UpstreamError>> + Send + 'static,
            ) -> ExecuteBodyStream {
                Box::pin(s)
            }
            let stream = typed_stream(try_stream! {
                while let Some(chunk) = upstream.next().await {
                    let chunk = chunk?;
                    let mut buf = chunk.to_vec();
                    crate::suffix::rewrite_model_suffix_in_body(&mut buf, &suffix);
                    yield Bytes::from(buf);
                }
            });
            ExecuteBody::Stream(stream)
        } else {
            ExecuteBody::Stream(response.body)
        };
        let billing_context = provider.build_billing_context(&prepared);

        Ok(ExecuteResult {
            status: response.status,
            headers: response.headers,
            body,
            usage: None,
            cost: None,
            billing: None,
            billing_context,
            meta,
            credential_updates,
            credential_index: used_credential_index,
        })
    }
}

fn parse_emulation(name: &str) -> wreq_util::Emulation {
    match name {
        "chrome_136" => wreq_util::Emulation::Chrome136,
        "chrome_135" => wreq_util::Emulation::Chrome135,
        "chrome_134" => wreq_util::Emulation::Chrome134,
        "chrome_133" => wreq_util::Emulation::Chrome133,
        "chrome_132" => wreq_util::Emulation::Chrome132,
        "chrome_131" => wreq_util::Emulation::Chrome131,
        "chrome_127" => wreq_util::Emulation::Chrome127,
        "safari_18" => wreq_util::Emulation::Safari18,
        "safari_18.2" => wreq_util::Emulation::Safari18_2,
        "safari_18.3" => wreq_util::Emulation::Safari18_3,
        "safari_18.5" => wreq_util::Emulation::Safari18_5,
        _ => wreq_util::Emulation::Chrome136,
    }
}

/// Metadata about the upstream WebSocket connection for logging.
#[derive(Debug, Clone)]
pub struct WsUpstreamMeta {
    /// The upstream WebSocket URL connected to.
    pub url: String,
    /// Request headers sent during the handshake.
    pub request_headers: Vec<(String, String)>,
    /// HTTP status code from the handshake response.
    pub response_status: u16,
    /// Index of the credential used.
    pub credential_index: usize,
}

/// Result of a WebSocket connection attempt.
pub enum WsConnectionResult {
    /// Direct passthrough — same protocol upstream and downstream.
    Connected(UpstreamWebSocket, WsUpstreamMeta),
    /// Cross-protocol bridge needed — upstream uses a different WS protocol.
    NeedsProtocolBridge {
        upstream: UpstreamWebSocket,
        src_protocol: ProtocolKind,
        dst_protocol: ProtocolKind,
        meta: WsUpstreamMeta,
    },
}

/// Determine HTTP method and base path for a given operation.
///
/// For most operations the engine historically used `POST /{op}`.
/// File and model endpoints require specific methods and real API paths.
/// Returns `(method, path)` where `path` may still need dynamic segments
/// (file_id, model_id, query params) appended by `build_operation_path`.
fn operation_http_method(operation: OperationFamily) -> http::Method {
    match operation {
        OperationFamily::FileList
        | OperationFamily::FileContent
        | OperationFamily::FileGet
        | OperationFamily::ModelList
        | OperationFamily::ModelGet => http::Method::GET,
        OperationFamily::FileDelete => http::Method::DELETE,
        _ => http::Method::POST,
    }
}

/// Returns true when the operation is one of the Files API endpoints.
pub fn is_file_operation(operation: OperationFamily) -> bool {
    matches!(
        operation,
        OperationFamily::FileUpload
            | OperationFamily::FileList
            | OperationFamily::FileContent
            | OperationFamily::FileGet
            | OperationFamily::FileDelete
    )
}

/// Returns true when the prepared request path belongs to a Files API endpoint.
pub fn is_file_operation_path(path: &str) -> bool {
    path.starts_with("/v1/files")
}

/// Determine the WS path for a given destination operation.
/// Returns `None` if the destination is not a WS-capable operation.
fn ws_path_for_operation<'a>(
    operation: &OperationFamily,
    _protocol: &ProtocolKind,
    model: Option<&'a str>,
) -> (Option<String>, Option<&'a str>) {
    match operation {
        OperationFamily::OpenAiResponseWebSocket => (Some("/v1/responses".to_string()), model),
        OperationFamily::GeminiLive => {
            let model_name = model.unwrap_or("unknown");
            let path = format!("/v1beta/models/{model_name}:streamGenerateContent");
            (Some(path), None) // model is in the path, not query
        }
        _ => (None, model),
    }
}

/// Wrapper around a wreq WebSocket connection to an upstream provider.
pub struct UpstreamWebSocket {
    inner: wreq::ws::WebSocket,
}

impl UpstreamWebSocket {
    /// Get a mutable reference to the inner wreq WebSocket.
    /// Use `futures_util::StreamExt` and `futures_util::SinkExt` for
    /// send/recv, or call `recv()` / `send()` directly.
    pub fn into_inner(self) -> wreq::ws::WebSocket {
        self.inner
    }

    /// Receive a message from the upstream WebSocket.
    pub async fn recv(&mut self) -> Option<Result<WsMessage, UpstreamError>> {
        self.inner
            .recv()
            .await
            .map(|r| r.map_err(|e| UpstreamError::Http(e.to_string())))
    }

    /// Send a message to the upstream WebSocket.
    pub async fn send(&mut self, msg: WsMessage) -> Result<(), UpstreamError> {
        self.inner
            .send(msg)
            .await
            .map_err(|e| UpstreamError::Http(e.to_string()))
    }
}

/// Re-export wreq WS message type.
pub use wreq::ws::message::Message as WsMessage;

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{is_stream_aggregation_route, validate_credential_json};
    use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

    #[test]
    fn validate_credential_json_accepts_valid_openai_credential() {
        let credential = json!({ "api_key": "sk-test" });
        assert!(validate_credential_json("openai", &credential).is_ok());
    }

    #[test]
    fn validate_credential_json_rejects_invalid_openai_credential() {
        let credential = json!({ "token": "sk-test" });
        let err = validate_credential_json("openai", &credential).unwrap_err();
        assert!(err.to_string().contains("invalid credential"));
    }

    #[test]
    fn stream_aggregation_treats_chat_to_responses_as_compatible() {
        assert!(is_stream_aggregation_route(
            OperationFamily::GenerateContent,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
            ProtocolKind::OpenAiResponse,
        ));
    }

    #[test]
    fn stream_aggregation_treats_responses_to_responses_as_compatible() {
        assert!(is_stream_aggregation_route(
            OperationFamily::GenerateContent,
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiResponse,
            ProtocolKind::OpenAiResponse,
        ));
    }
}
