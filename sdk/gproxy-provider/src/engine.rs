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
    /// When set, the engine replaces the `"model"` field in all non-ModelList
    /// responses with this value (both full and streaming). Used for alias
    /// rewriting so clients see the alias name they sent, not the resolved
    /// upstream model.
    pub response_model_override: Option<String>,
}

/// Result of an engine execution.
pub struct ExecuteResult {
    pub status: u16,
    pub headers: http::HeaderMap,
    pub body: ExecuteBody,
    pub usage: Option<Usage>,
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
    /// Count of server-side tool invocations per tool key. Populated by the
    /// usage extractor when the upstream response carries an explicit count
    /// (e.g. Claude's `server_tool_use`). An empty map means "no tools
    /// invoked" — not "unknown."
    pub tool_uses: std::collections::BTreeMap<String, i64>,
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
        let mut client_builder = wreq::Client::builder().http1_only();
        if let Some(proxy_url) = proxy
            && !proxy_url.is_empty()
            && let Ok(p) = wreq::Proxy::all(proxy_url)
        {
            client_builder = client_builder.proxy(p);
        }
        let client = match client_builder.build() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "failed to build http client, falling back to default");
                wreq::Client::default()
            }
        };

        let emu = parse_emulation(emulation.unwrap_or("chrome_136"));
        let mut spoof_builder = wreq::Client::builder().emulation(emu).http1_only();
        if let Some(proxy_url) = proxy
            && !proxy_url.is_empty()
            && let Ok(p) = wreq::Proxy::all(proxy_url)
        {
            spoof_builder = spoof_builder.proxy(p);
        }
        let spoof = match spoof_builder.build() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "failed to build spoof client, falling back to default");
                wreq::Client::default()
            }
        };

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

    /// Rebuild the HTTP clients with a new proxy and/or spoof emulation,
    /// returning a new engine that shares the same provider store.
    pub fn with_new_clients(&self, proxy: Option<&str>, emulation: Option<&str>) -> GproxyEngine {
        let mut client_builder = wreq::Client::builder();
        if let Some(proxy_url) = proxy
            && !proxy_url.is_empty()
            && let Ok(p) = wreq::Proxy::all(proxy_url)
        {
            client_builder = client_builder.proxy(p);
        }
        let client = match client_builder.build() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "failed to build http client in with_new_clients");
                wreq::Client::default()
            }
        };

        let emu = parse_emulation(emulation.unwrap_or("chrome_136"));
        let mut spoof_builder = wreq::Client::builder().emulation(emu);
        if let Some(proxy_url) = proxy
            && !proxy_url.is_empty()
            && let Ok(p) = wreq::Proxy::all(proxy_url)
        {
            spoof_builder = spoof_builder.proxy(p);
        }
        let spoof_client = match spoof_builder.build() {
            Ok(c) => Some(c),
            Err(e) => {
                tracing::warn!(error = %e, "failed to build spoof client in with_new_clients");
                None
            }
        };

        GproxyEngine {
            store: Arc::clone(&self.store),
            client,
            spoof_client,
            enable_usage: self.enable_usage,
            enable_upstream_log: self.enable_upstream_log,
            enable_upstream_log_body: self.enable_upstream_log_body,
        }
    }

    /// Create a new engine with updated operational settings and rebuilt
    /// HTTP clients. Used by the admin settings handler.
    pub fn with_settings(
        &self,
        proxy: Option<&str>,
        emulation: Option<&str>,
        enable_usage: bool,
        enable_upstream_log: bool,
        enable_upstream_log_body: bool,
    ) -> GproxyEngine {
        let mut engine = self.with_new_clients(proxy, emulation);
        engine.enable_usage = enable_usage;
        engine.enable_upstream_log = enable_upstream_log;
        engine.enable_upstream_log_body = enable_upstream_log_body;
        engine
    }

    pub fn store(&self) -> &Arc<ProviderStore> {
        &self.store
    }

    /// Get the rewrite rules for a named provider.
    pub fn rewrite_rules(&self, provider: &str) -> Vec<crate::utils::rewrite::RewriteRule> {
        self.store
            .get_runtime(provider)
            .map(|rt| rt.rewrite_rules())
            .unwrap_or_default()
    }

    /// Check whether the dispatch rule for this (provider, operation, protocol)
    /// resolves to the `Local` implementation.
    pub fn is_local_dispatch(
        &self,
        provider: &str,
        operation: OperationFamily,
        protocol: ProtocolKind,
    ) -> bool {
        let Some(runtime) = self.store.get_runtime(provider) else {
            return false;
        };
        let key = crate::dispatch::RouteKey::new(operation, protocol);
        matches!(
            runtime.dispatch_table().resolve(&key),
            Some(crate::dispatch::RouteImplementation::Local)
        )
    }

    /// Bootstrap a credential on upsert — runs any channel-specific IO
    /// that should happen once, right before the credential lands in
    /// the DB. Currently only `claudecode` has a non-trivial
    /// implementation (exchanging a Claude.ai sessionKey cookie for
    /// OAuth tokens so the first user request doesn't have to do the
    /// full cookie→token dance via `refresh_credential`).
    ///
    /// Returns:
    /// - `Ok(Some(updated_json))` — the caller should persist this
    ///   value instead of the original.
    /// - `Ok(None)` — nothing to bootstrap; store the original JSON.
    /// - `Err(..)` — bootstrap attempted and failed. The admin handler
    ///   surfaces this as a `400 Bad Request` so operators see the
    ///   real cause (invalid cookie, Cloudflare block, etc.) at the
    ///   moment of upsert rather than at the first chat request.
    pub async fn bootstrap_credential_on_upsert(
        &self,
        channel: &str,
        credential_json: &Value,
    ) -> Result<(Option<Value>, Vec<UpstreamRequestMeta>), (UpstreamError, Vec<UpstreamRequestMeta>)>
    {
        match channel {
            "claudecode" => {
                crate::channels::claudecode::bootstrap_credential_from_cookie(
                    &self.client,
                    self.spoof_client.as_ref(),
                    credential_json,
                )
                .await
            }
            "vertex" => {
                crate::channels::vertex::bootstrap_vertex_token(&self.client, credential_json).await
            }
            _ => Ok((None, Vec::new())),
        }
    }

    pub fn estimate_billing(
        &self,
        provider_name: &str,
        context: &crate::billing::BillingContext,
        usage: &Usage,
    ) -> Option<crate::billing::BillingResult> {
        self.store.estimate_billing(provider_name, context, usage)
    }

    /// Replace model pricing for a provider. Used by the host application
    /// to push DB-backed pricing into the billing engine after admin edits.
    ///
    /// Returns `false` if the provider is not registered.
    pub fn set_model_pricing(
        &self,
        provider_name: &str,
        prices: Vec<crate::billing::ModelPrice>,
    ) -> bool {
        self.store.set_model_pricing(provider_name, prices)
    }

    /// Build a [`BillingContext`] for a provider from the model name and
    /// raw request body, without requiring an engine-internal
    /// [`PreparedRequest`].
    pub fn build_billing_context(
        &self,
        provider_name: &str,
        model: Option<&str>,
        body: &[u8],
    ) -> Option<crate::billing::BillingContext> {
        self.store.build_billing_context(provider_name, model, body)
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
    ///
    /// If the initial request returns 401/403, attempts to refresh the
    /// credential and retries once. Returns the response together with
    /// any credential updates that should be persisted to the database,
    /// plus request metadata for upstream logging.
    pub async fn query_quota(
        &self,
        provider_name: &str,
        credential_index: Option<usize>,
    ) -> Result<
        (
            Option<crate::response::UpstreamResponse>,
            Vec<CredentialUpdate>,
            Option<UpstreamRequestMeta>,
        ),
        UpstreamError,
    > {
        let span = tracing::info_span!("engine.query_quota", provider = provider_name);
        async {
            let provider = self.store.get_runtime(provider_name).ok_or_else(|| {
                UpstreamError::Channel(format!("unknown provider: {provider_name}"))
            })?;
            let Some(http_request) = provider.prepare_quota_request(credential_index)? else {
                return Ok((None, Vec::new(), None));
            };

            let start = std::time::Instant::now();
            let mut meta = snapshot_request_meta(&http_request, credential_index);

            let response = crate::http_client::send_request(&self.client, http_request).await?;

            if matches!(response.status, 401 | 403) {
                tracing::warn!(
                    provider = provider_name,
                    status = response.status,
                    "quota request auth failed, attempting credential refresh"
                );
                if let Some(update) = provider
                    .refresh_credential_at(credential_index, &self.client)
                    .await?
                {
                    let updates = vec![update];
                    let Some(retry_request) = provider.prepare_quota_request(credential_index)?
                    else {
                        return Ok((None, updates, None));
                    };

                    let retry_start = std::time::Instant::now();
                    meta = snapshot_request_meta(&retry_request, credential_index);

                    let retry_response =
                        crate::http_client::send_request(&self.client, retry_request).await?;
                    fill_response_meta(&mut meta, &retry_response, retry_start);
                    return Ok((Some(retry_response), updates, Some(meta)));
                }
            }

            fill_response_meta(&mut meta, &response, start);
            Ok((Some(response), Vec::new(), Some(meta)))
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

        let body = inject_stream_flag(dst_op, dst_proto, body);
        let method = operation_http_method(dst_op);

        let prepared = PreparedRequest {
            method,
            route: RouteKey::new(dst_op, dst_proto),
            model: request.model.clone(),
            body,
            headers: request.headers,
        };

        let mut prepared = provider.finalize_request(prepared)?;

        // Sanitize request body text after finalize_request so channel-
        // specific normalization has already run. Dispatches to the correct
        // protocol walker based on the destination protocol.
        let rules = provider.sanitize_rules();
        if !rules.is_empty()
            && let Ok(mut body_json) = serde_json::from_slice::<serde_json::Value>(&prepared.body)
        {
            crate::utils::sanitize::apply_sanitize_rules(
                &mut body_json,
                prepared.route.protocol,
                &rules,
            );
            if let Ok(patched) = serde_json::to_vec(&body_json) {
                prepared.body = patched;
            }
        }

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
        let raw_response_body_for_log = if self.enable_upstream_log && self.enable_upstream_log_body
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
            && !(request.protocol == dst_proto && response_transform_dst_op == request.operation);
        let mut response_body = if needs_response_transform {
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

        // 3.6. Alias response rewriting: replace model field with the alias
        // name the client sent. Applies to all non-ModelList operations. For
        // ModelGet, rewrites the id/name field instead of the "model" key.
        if let Some(ref override_model) = request.response_model_override {
            match dst_op {
                OperationFamily::ModelList => {} // alias injection handled by caller
                OperationFamily::ModelGet => {
                    rewrite_model_id_in_body(&mut response_body, override_model, request.protocol);
                }
                _ => {
                    rewrite_model_field_in_body(&mut response_body, override_model);
                }
            }
        }

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

        let body = inject_stream_flag(dst_op, dst_proto, body);
        let method = operation_http_method(dst_op);

        let prepared = PreparedRequest {
            method,
            route: RouteKey::new(dst_op, dst_proto),
            model: request.model.clone(),
            body,
            headers: request.headers,
        };

        let mut prepared = provider.finalize_request(prepared)?;

        // Sanitize request body text after finalize_request so channel-
        // specific normalization has already run. Dispatches to the correct
        // protocol walker based on the destination protocol.
        let rules = provider.sanitize_rules();
        if !rules.is_empty()
            && let Ok(mut body_json) = serde_json::from_slice::<serde_json::Value>(&prepared.body)
        {
            crate::utils::sanitize::apply_sanitize_rules(
                &mut body_json,
                prepared.route.protocol,
                &rules,
            );
            if let Ok(patched) = serde_json::to_vec(&body_json) {
                prepared.body = patched;
            }
        }

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
            let model_override = request.response_model_override.clone();
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
                    if let Some(ref alias) = model_override {
                        rewrite_model_field_in_body(&mut out, alias);
                    }
                    if !out.is_empty() {
                        yield Bytes::from(out);
                    }
                }

                let mut tail = transformer.finish()?;
                if let Some(ref alias) = model_override {
                    rewrite_model_field_in_body(&mut tail, alias);
                }
                if !tail.is_empty() {
                    yield Bytes::from(tail);
                }
            });
            ExecuteBody::Stream(stream)
        } else if request.response_model_override.is_some() {
            // Passthrough with alias rewriting
            let model_override = request.response_model_override.clone();
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
                    if let Some(ref alias) = model_override {
                        rewrite_model_field_in_body(&mut buf, alias);
                    }
                    yield Bytes::from(buf);
                }
            });
            ExecuteBody::Stream(stream)
        } else {
            ExecuteBody::Stream(response.body)
        };

        Ok(ExecuteResult {
            status: response.status,
            headers: response.headers,
            body,
            usage: None,
            meta,
            credential_updates,
            credential_index: used_credential_index,
        })
    }
}

fn snapshot_request_meta(
    req: &http::Request<Vec<u8>>,
    credential_index: Option<usize>,
) -> UpstreamRequestMeta {
    UpstreamRequestMeta {
        method: req.method().as_str().to_string(),
        url: req.uri().to_string(),
        request_headers: req
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect(),
        request_body: Some(req.body().clone()),
        response_status: None,
        response_headers: Vec::new(),
        response_body: None,
        model: None,
        latency_ms: 0,
        credential_index,
    }
}

fn fill_response_meta(
    meta: &mut UpstreamRequestMeta,
    response: &crate::response::UpstreamResponse,
    start: std::time::Instant,
) {
    meta.response_status = Some(response.status);
    meta.response_headers = response
        .headers
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    meta.response_body = Some(response.body.clone());
    meta.latency_ms = start.elapsed().as_millis() as u64;
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

/// Replace the `"model"` field in a JSON response body with `new_model`.
/// Used for alias rewriting on chat/messages responses.
fn rewrite_model_field_in_body(body: &mut Vec<u8>, new_model: &str) {
    let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(body) else {
        return;
    };
    if v.get("model").is_some() {
        v["model"] = serde_json::Value::String(new_model.to_string());
        if let Ok(b) = serde_json::to_vec(&v) {
            *body = b;
        }
    }
}

/// Replace the model identifier field in a model-get response body.
/// OpenAI/Claude use `"id"`, Gemini uses `"name"`.
fn rewrite_model_id_in_body(body: &mut Vec<u8>, new_id: &str, protocol: ProtocolKind) {
    let Ok(mut v) = serde_json::from_slice::<serde_json::Value>(body) else {
        return;
    };
    match protocol {
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
            if v.get("name").is_some() {
                v["name"] = serde_json::Value::String(format!("models/{new_id}"));
            }
            if v.get("baseModelId").is_some() {
                v["baseModelId"] = serde_json::Value::String(new_id.to_string());
            }
        }
        _ => {
            if v.get("id").is_some() {
                v["id"] = serde_json::Value::String(new_id.to_string());
            }
        }
    }
    if let Ok(b) = serde_json::to_vec(&v) {
        *body = b;
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

/// Inject or overwrite the `"stream"` flag in an already-serialized JSON
/// request body so it matches the resolved operation family.
///
/// Protocol transforms (Response→Chat, Gemini→Chat, Gemini→Claude, etc.)
/// produce bodies without a correct `stream` flag because they don't know
/// whether the engine picked `GenerateContent` or `StreamGenerateContent`.
/// This helper runs **after** transform but **before** suffix processing
/// and `finalize_request`, covering every channel + transform combination
/// without per-channel patches.
///
/// Only touches OpenAI-family and Claude protocols; Gemini uses URL-based
/// stream selection and has no body-level flag.
fn inject_stream_flag(dst_op: OperationFamily, dst_proto: ProtocolKind, body: Vec<u8>) -> Vec<u8> {
    if !matches!(
        dst_proto,
        ProtocolKind::OpenAiChatCompletion
            | ProtocolKind::OpenAiResponse
            | ProtocolKind::OpenAi
            | ProtocolKind::Claude
    ) {
        return body;
    }
    // Only generate-content operations carry a stream flag.
    if !matches!(
        dst_op,
        OperationFamily::GenerateContent | OperationFamily::StreamGenerateContent
    ) {
        return body;
    }
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return body;
    };
    let Some(map) = value.as_object_mut() else {
        return body;
    };
    let should_stream = dst_op == OperationFamily::StreamGenerateContent;
    map.insert("stream".to_string(), serde_json::Value::Bool(should_stream));
    serde_json::to_vec(&value).unwrap_or(body)
}
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
