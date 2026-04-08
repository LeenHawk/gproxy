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

use crate::health::ModelCooldownHealth;
use crate::request::PreparedRequest;
use crate::response::UpstreamError;
use crate::store::{CredentialUpdate, ProviderStore, ProviderStoreBuilder};
use crate::Channel;

fn is_stream_aggregation_route(
    src_operation: OperationFamily,
    dst_operation: OperationFamily,
    src_protocol: ProtocolKind,
    dst_protocol: ProtocolKind,
) -> bool {
    src_operation == OperationFamily::GenerateContent
        && dst_operation == OperationFamily::StreamGenerateContent
        && src_protocol == dst_protocol
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
        self.store_builder = self
            .store_builder
            .add_provider_with_dispatch(name, channel, settings, credentials, dispatch_override);
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
                            UpstreamError::Channel(format!(
                                "invalid dispatch for '{}': {e}",
                                name
                            ))
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
    pub async fn execute(&self, request: ExecuteRequest) -> Result<ExecuteResult, UpstreamError> {
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

    async fn execute_inner(&self, request: ExecuteRequest) -> Result<ExecuteResult, UpstreamError> {
        let provider = self.store.get_runtime(&request.provider).ok_or_else(|| {
            tracing::warn!(provider = %request.provider, "unknown provider");
            UpstreamError::Channel(format!("unknown provider: {}", request.provider))
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
                return Err(UpstreamError::Channel(format!(
                    "unsupported: ({}, {})",
                    request.operation, request.protocol
                )));
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
        let path = build_operation_path(dst_op, dst_proto, model.as_deref(), &mut body)?;

        let mut prepared = PreparedRequest {
            method,
            path,
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

        let provider_result = provider
            .execute(
                prepared.clone(),
                affinity_hint,
                forced_credential,
                &self.client,
                self.spoof_client.as_ref(),
            )
            .await?;
        let response = provider_result.response;
        let credential_updates = provider_result.credential_updates;
        let used_credential_index = provider_result.credential_index;

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

        // 3. Transform response if needed (cross-protocol)
        let response_body = if needs_transform {
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
            Some(UpstreamRequestMeta {
                method: "POST".to_string(),
                url: String::new(),
                request_headers: Vec::new(),
                request_body: None,
                response_status: Some(response.status),
                response_headers: response
                    .headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect(),
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
    ) -> Result<ExecuteResult, UpstreamError> {
        let provider = self.store.get_runtime(&request.provider).ok_or_else(|| {
            tracing::warn!(provider = %request.provider, "unknown provider");
            UpstreamError::Channel(format!("unknown provider: {}", request.provider))
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
                return Err(UpstreamError::Channel(format!(
                    "unsupported: ({}, {})",
                    request.operation, request.protocol
                )));
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
        let path = build_operation_path(dst_op, dst_proto, model.as_deref(), &mut body)?;

        let mut prepared = PreparedRequest {
            method,
            path,
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

        let provider_result = provider
            .execute_stream(
                prepared.clone(),
                affinity_hint,
                forced_credential,
                &self.client,
                self.spoof_client.as_ref(),
            )
            .await?;
        let response = provider_result.response;
        let credential_updates = provider_result.credential_updates;
        let used_credential_index = provider_result.credential_index;

        let meta = if self.enable_upstream_log {
            Some(UpstreamRequestMeta {
                method: "POST".to_string(),
                url: String::new(),
                request_headers: Vec::new(),
                request_body: None,
                response_status: Some(response.status),
                response_headers: response
                    .headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect(),
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
            let stream = try_stream! {
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
            };
            ExecuteBody::Stream(Box::pin(stream))
        } else if let Some(ref suffix) = suffix_str {
            // Passthrough with suffix rewriting
            let suffix = suffix.clone();
            let mut upstream = response.body;
            let stream = try_stream! {
                while let Some(chunk) = upstream.next().await {
                    let chunk = chunk?;
                    let mut buf = chunk.to_vec();
                    crate::suffix::rewrite_model_suffix_in_body(&mut buf, &suffix);
                    yield Bytes::from(buf);
                }
            };
            ExecuteBody::Stream(Box::pin(stream))
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

/// Build the full upstream path for an operation.
///
/// This maps the canonical `(operation, protocol)` pair to the real upstream
/// HTTP endpoint. File and model list routes additionally consume encoded
/// `path/query` data from the body prepared by the API layer.
fn build_operation_path(
    operation: OperationFamily,
    protocol: ProtocolKind,
    model: Option<&str>,
    body: &mut Vec<u8>,
) -> Result<String, UpstreamError> {
    let path = match operation {
        OperationFamily::FileUpload => "/v1/files".to_string(),
        OperationFamily::FileList => {
            let path = build_file_list_path(body);
            *body = Vec::new();
            path
        }
        OperationFamily::FileContent => {
            let file_id = extract_path_param(body, "file_id");
            *body = Vec::new();
            format!("/v1/files/{}/content", file_id)
        }
        OperationFamily::FileGet => {
            let file_id = extract_path_param(body, "file_id");
            *body = Vec::new();
            format!("/v1/files/{}", file_id)
        }
        OperationFamily::FileDelete => {
            let file_id = extract_path_param(body, "file_id");
            *body = Vec::new();
            format!("/v1/files/{}", file_id)
        }
        OperationFamily::ModelList => build_model_list_path(protocol, body),
        OperationFamily::ModelGet => build_model_get_path(protocol, model)?,
        OperationFamily::CountToken => match protocol {
            ProtocolKind::OpenAi => "/v1/responses/input_tokens/count".to_string(),
            ProtocolKind::Claude => "/v1/messages/count_tokens".to_string(),
            ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
                build_gemini_model_action_path(model, "countTokens", None)?
            }
            _ => return unsupported_path(operation, protocol),
        },
        OperationFamily::Compact => match protocol {
            ProtocolKind::OpenAi => "/v1/responses/compact".to_string(),
            _ => return unsupported_path(operation, protocol),
        },
        OperationFamily::GenerateContent => match protocol {
            ProtocolKind::OpenAiResponse => "/v1/responses".to_string(),
            ProtocolKind::OpenAiChatCompletion => "/v1/chat/completions".to_string(),
            ProtocolKind::Claude => "/v1/messages".to_string(),
            ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
                build_gemini_model_action_path(model, "generateContent", None)?
            }
            _ => return unsupported_path(operation, protocol),
        },
        OperationFamily::StreamGenerateContent => match protocol {
            ProtocolKind::OpenAiResponse => "/v1/responses".to_string(),
            ProtocolKind::OpenAiChatCompletion => "/v1/chat/completions".to_string(),
            ProtocolKind::Claude => "/v1/messages".to_string(),
            ProtocolKind::Gemini => {
                build_gemini_model_action_path(model, "streamGenerateContent", Some("alt=sse"))?
            }
            ProtocolKind::GeminiNDJson => {
                build_gemini_model_action_path(model, "streamGenerateContent", None)?
            }
            _ => return unsupported_path(operation, protocol),
        },
        OperationFamily::CreateImage | OperationFamily::StreamCreateImage => match protocol {
            ProtocolKind::OpenAi => "/v1/images/generations".to_string(),
            _ => return unsupported_path(operation, protocol),
        },
        OperationFamily::CreateImageEdit | OperationFamily::StreamCreateImageEdit => match protocol
        {
            ProtocolKind::OpenAi => "/v1/images/edits".to_string(),
            _ => return unsupported_path(operation, protocol),
        },
        OperationFamily::OpenAiResponseWebSocket => "/v1/responses".to_string(),
        OperationFamily::GeminiLive => {
            build_gemini_model_action_path(model, "streamGenerateContent", Some("alt=sse"))?
        }
        OperationFamily::Embedding => match protocol {
            ProtocolKind::OpenAi => "/v1/embeddings".to_string(),
            ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
                build_gemini_model_action_path(model, "embedContent", None)?
            }
            _ => return unsupported_path(operation, protocol),
        },
    };

    Ok(path)
}

fn unsupported_path(
    operation: OperationFamily,
    protocol: ProtocolKind,
) -> Result<String, UpstreamError> {
    Err(UpstreamError::Channel(format!(
        "no upstream path mapping for ({operation}, {protocol})"
    )))
}

fn build_model_list_path(protocol: ProtocolKind, body: &[u8]) -> String {
    match protocol {
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
            build_query_path("/v1beta/models", body, &["pageSize", "pageToken"])
        }
        ProtocolKind::Claude => {
            build_query_path("/v1/models", body, &["after_id", "before_id", "limit"])
        }
        ProtocolKind::OpenAi => "/v1/models".to_string(),
        _ => "/v1/models".to_string(),
    }
}

fn build_model_get_path(
    protocol: ProtocolKind,
    model: Option<&str>,
) -> Result<String, UpstreamError> {
    let model = require_model_segment(model)?;
    Ok(match protocol {
        ProtocolKind::Gemini | ProtocolKind::GeminiNDJson => {
            format!("/v1beta/{}", gemini_model_resource(model))
        }
        ProtocolKind::OpenAi | ProtocolKind::Claude => format!("/v1/models/{model}"),
        _ => {
            return Err(UpstreamError::Channel(format!(
                "no model_get path mapping for protocol: {protocol}"
            )));
        }
    })
}

fn build_gemini_model_action_path(
    model: Option<&str>,
    action: &str,
    query: Option<&str>,
) -> Result<String, UpstreamError> {
    let resource = gemini_model_resource(require_model_segment(model)?);
    let mut path = format!("/v1beta/{resource}:{action}");
    if let Some(query) = query {
        path.push('?');
        path.push_str(query);
    }
    Ok(path)
}

fn require_model_segment(model: Option<&str>) -> Result<String, UpstreamError> {
    let value = model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| UpstreamError::Channel("missing model for upstream path".into()))?;
    Ok(url::form_urlencoded::byte_serialize(value.as_bytes()).collect())
}

fn gemini_model_resource(model: String) -> String {
    if model.starts_with("models%2F") || model.starts_with("models/") {
        model
    } else {
        format!("models/{model}")
    }
}

fn build_query_path(base: &str, body: &[u8], allowed_keys: &[&str]) -> String {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return base.to_string();
    };
    let Some(query) = value.get("query").and_then(|query| query.as_object()) else {
        return base.to_string();
    };

    let mut params = Vec::new();
    for key in allowed_keys {
        let Some(value) = query.get(*key) else {
            continue;
        };
        let encoded = match value {
            serde_json::Value::String(text) => {
                url::form_urlencoded::byte_serialize(text.as_bytes()).collect::<String>()
            }
            serde_json::Value::Number(number) => number.to_string(),
            _ => continue,
        };
        params.push(format!("{key}={encoded}"));
    }

    if params.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{}", params.join("&"))
    }
}

/// Extract a path parameter from a JSON body like `{"path":{"file_id":"..."}}`
fn extract_path_param(body: &[u8], param: &str) -> String {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.pointer(&format!("/path/{}", param))
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .unwrap_or_default()
}

/// Build `/v1/files?after_id=...&before_id=...&limit=...` from the body JSON.
fn build_file_list_path(body: &[u8]) -> String {
    let mut base = "/v1/files".to_string();
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(body) else {
        return base;
    };
    let query = v.get("query");
    let mut params = Vec::new();
    if let Some(q) = query {
        if let Some(s) = q.get("after_id").and_then(|v| v.as_str()) {
            params.push(format!("after_id={}", s));
        }
        if let Some(s) = q.get("before_id").and_then(|v| v.as_str()) {
            params.push(format!("before_id={}", s));
        }
        if let Some(n) = q.get("limit").and_then(|v| v.as_u64()) {
            params.push(format!("limit={}", n));
        }
    }
    if !params.is_empty() {
        base.push('?');
        base.push_str(&params.join("&"));
    }
    base
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

    use super::validate_credential_json;

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
}
