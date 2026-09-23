//! HTTP and WebSocket operations, request shaping and turn state.

use super::common::invalid_config;
use super::headers::{Account, account, backend_headers, base_urls};
use super::identity::{Identity, RequestKind};
use super::{
    CLI_HEADERS, Codex, CodexConfig, ID, default_connection, identity, models, realtime, shape, sse,
};
use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ChannelServices,
    ChannelState, ConfigKey, ConfigKeyKind, CredentialRefresh, HOST_CONFIG_KEYS, HeaderAllowlist,
    LoginMode, OAuthAuthorizationCode, OAuthDeviceCode, OperationContext, OperationFuture,
    PrepareContext, ProviderView, QuotaHeaders, QuotaModel, QuotaQuery, UsageExtractor,
    UsageStream,
};
use crate::channels::shared::cache;
use crate::channels::shared::services_common::unix_now_ms;
use gproxy_client::ConnectionConfig;
use gproxy_protocol::capability::{StateWrite, UpstreamConnection, Version};
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse};
use http::{HeaderMap, HeaderName, HeaderValue, header};

const RESPONSES_WS_BETA: &str = "responses_websockets=2026-02-06";

/// The backend path for an operation; the client's native path is not
/// trusted because the Codex backend mounts Responses under its own prefix.
fn operation_path(operation: Operation) -> Result<&'static str, ChannelError> {
    Ok(match operation {
        Operation::GenerateContent | Operation::StreamGenerateContent => "/responses",
        Operation::CompactContent => "/responses/compact",
        Operation::CreateImage => "/images/generations",
        Operation::EditImage => "/images/edits",
        Operation::SummarizeMemory => "/memories/trace_summarize",
        Operation::CreateRealtimeCall => "/realtime/calls",
        Operation::WebSearch => "/alpha/search",
        other => {
            return Err(ChannelError::UnsupportedOperation(
                gproxy_protocol::OperationKey {
                    operation: other,
                    dialect: Dialect::OpenAi,
                },
            ));
        }
    })
}

/// The Responses operations that carry the CLI's identity and whose replies
/// may carry a turn-state token.
fn request_kind(operation: Operation) -> Option<RequestKind> {
    match operation {
        Operation::GenerateContent | Operation::StreamGenerateContent => Some(RequestKind::Turn),
        Operation::CompactContent => Some(RequestKind::Compaction),
        _ => None,
    }
}

/// A client that sends its own turn metadata is a real Codex session
/// managing its own identity and turn state; the channel stays out of it.
fn client_managed(headers: &HeaderMap) -> bool {
    headers.contains_key("x-codex-turn-metadata")
}

fn client_header(headers: &HeaderMap, name: &'static str) -> Option<String> {
    headers
        .get(name)?
        .to_str()
        .ok()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn expires_in(ttl_ms: i64) -> std::time::SystemTime {
    std::time::SystemTime::UNIX_EPOCH
        + std::time::Duration::from_millis(u64::try_from(unix_now_ms() + ttl_ms).unwrap_or(0))
}

/// The state versions an identity was read against; the writes after the
/// call expect them, so a concurrent call in the same thread loses cleanly.
struct Remembered {
    window: Option<Version>,
    turn_state: Option<Version>,
}

impl Codex {
    /// The identity of a request that is not a CLI's own: the client's
    /// `session-id` and `thread-id` when it sent them, everything else
    /// derived from the account and the body; the installation id, window
    /// number and turn state come from channel state. A host without state
    /// gets a fresh window 0 and no turn state every call.
    async fn identity(
        &self,
        state: &dyn ChannelState,
        account: &Account<'_>,
        headers: &HeaderMap,
        body: Option<&[u8]>,
        kind: Option<RequestKind>,
        streaming: bool,
    ) -> (Identity, Remembered) {
        let facts = body
            .map(identity::body_facts)
            .unwrap_or_else(|| identity::body_facts(b""));
        let account_key =
            identity::account_key(account.account_id.as_deref(), account.access_token);
        let session_id = client_header(headers, "session-id")
            .unwrap_or_else(|| identity::session_id(&account_key, &facts.session_key));
        let thread_id = client_header(headers, "thread-id").unwrap_or_else(|| session_id.clone());
        let turn_id = identity::turn_id(&session_id, facts.user_text_turns);

        let persisted = state
            .get(identity::INSTALLATION_KEY)
            .await
            .ok()
            .flatten()
            .and_then(|entry| String::from_utf8(entry.payload.to_vec()).ok())
            .filter(|value| identity::is_uuid(value));
        let installation_id = match persisted {
            Some(id) => id,
            None => {
                let id = identity::installation_id(&account_key);
                // Written once; a conflict means another call wrote the same
                // derived value first, a stateless host simply derives again.
                let _ = state
                    .compare_exchange(
                        identity::INSTALLATION_KEY,
                        None,
                        Some(StateWrite {
                            payload: Bytes::copy_from_slice(id.as_bytes()),
                            expires_at: None,
                        }),
                    )
                    .await;
                id
            }
        };

        let window = state
            .get(&identity::window_key(&thread_id))
            .await
            .ok()
            .flatten();
        let window_number = window
            .as_ref()
            .and_then(|entry| std::str::from_utf8(&entry.payload).ok()?.parse().ok())
            .unwrap_or(0);
        let turn_state = match kind {
            Some(_) => state
                .get(&identity::turn_state_key(&thread_id, &turn_id))
                .await
                .ok()
                .flatten(),
            None => None,
        };
        let token = turn_state
            .as_ref()
            .and_then(|entry| String::from_utf8(entry.payload.to_vec()).ok())
            .filter(|value| !value.is_empty());
        (
            Identity {
                session_id,
                thread_id,
                turn_id,
                installation_id,
                window_number,
                turn_state: token,
                routing_hint: facts.routing_hint,
                kind,
                streaming,
            },
            Remembered {
                window: window.map(|entry| entry.version),
                turn_state: turn_state.map(|entry| entry.version),
            },
        )
    }

    /// After a successful Responses call: keep the backend's turn-state
    /// token for the rest of the turn, and open the next window after a
    /// compaction. Losing a CAS race, or a host without state, only loses
    /// one replay.
    async fn remember(
        &self,
        state: &dyn ChannelState,
        identity: &Identity,
        remembered: Remembered,
        response: &HeaderMap,
    ) {
        if let Some(token) = client_header(response, "x-codex-turn-state")
            && identity.turn_state.as_deref() != Some(token.as_str())
        {
            let _ = state
                .compare_exchange(
                    &identity::turn_state_key(&identity.thread_id, &identity.turn_id),
                    remembered.turn_state,
                    Some(StateWrite {
                        payload: Bytes::from(token),
                        expires_at: Some(expires_in(identity::TURN_STATE_TTL_MS)),
                    }),
                )
                .await;
        }
        if identity.kind == Some(RequestKind::Compaction) {
            let next = identity.window_number.saturating_add(1).to_string();
            let _ = state
                .compare_exchange(
                    &identity::window_key(&identity.thread_id),
                    remembered.window,
                    Some(StateWrite {
                        payload: Bytes::from(next),
                        expires_at: Some(expires_in(identity::WINDOW_TTL_MS)),
                    }),
                )
                .await;
        }
    }

    /// The HTTP Responses path with the identity read before the call and
    /// the turn state and window written after it (as `claudecode` does
    /// with its previous request id).
    async fn responses_http(
        &self,
        operation: Operation,
        ctx: OperationContext<'_>,
    ) -> Result<WireResponse<HttpBody>, ChannelError> {
        let config = CodexConfig::from_view(ctx.provider)?;
        let OperationContext {
            provider,
            credential,
            dialect,
            request,
            client,
            state,
            endpoint_override,
            ..
        } = ctx;
        let resolved = if config.synthesize_cli_identity && !client_managed(&request.headers) {
            let account = account(&credential)?;
            let body = match &request.body {
                HttpBody::Bytes(bytes) => Some(bytes.as_ref()),
                HttpBody::Stream(_) => None,
            };
            Some(
                self.identity(
                    state.as_ref(),
                    &account,
                    &request.headers,
                    body,
                    request_kind(operation),
                    operation == Operation::StreamGenerateContent,
                )
                .await,
            )
        } else {
            None
        };
        let (prepared, aliases) = self.prepare_with(
            PrepareContext {
                provider,
                credential,
                operation: OperationKey { operation, dialect },
                request,
                endpoint_override,
            },
            resolved.as_ref().map(|(identity, _)| identity),
        )?;
        let mut response = client.send(prepared).await?;
        if let Some((identity, remembered)) = resolved
            && response.status.is_success()
        {
            self.remember(state.as_ref(), &identity, remembered, &response.headers)
                .await;
        }
        if let Some(aliases) = aliases
            && response.status.is_success()
            && (matches!(response.body, HttpBody::Stream(_))
                || response
                    .headers
                    .get(header::CONTENT_TYPE)
                    .and_then(|h| h.to_str().ok())
                    .is_some_and(|mime| {
                        mime.split(';').next().is_some_and(|mime| {
                            mime.trim().eq_ignore_ascii_case("text/event-stream")
                        })
                    }))
        {
            response.headers.remove(header::CONTENT_LENGTH);
            response.body = sse::normalize(response.body, aliases);
        }
        Ok(response)
    }

    /// The WebSocket handshake carries the session, thread, installation and
    /// window as the CLI's `build_websocket_headers` does, but no turn
    /// metadata: the turn lives in the request frames, which the channel
    /// does not see, so no turn state is replayed or captured here.
    async fn responses_websocket(
        &self,
        operation: Operation,
        ctx: OperationContext<'_, ()>,
    ) -> Result<UpstreamConnection, ChannelError> {
        let config = CodexConfig::from_view(ctx.provider)?;
        let OperationContext {
            provider,
            credential,
            dialect,
            request,
            client,
            state,
            endpoint_override,
            ..
        } = ctx;
        let resolved = if config.synthesize_cli_identity && !client_managed(&request.headers) {
            let account = account(&credential)?;
            Some(
                self.identity(
                    state.as_ref(),
                    &account,
                    &request.headers,
                    None,
                    None,
                    false,
                )
                .await,
            )
        } else {
            None
        };
        let (builder, _) = self.build(
            PrepareContext {
                provider,
                credential,
                operation: OperationKey { operation, dialect },
                request,
                endpoint_override,
            },
            true,
            resolved.as_ref().map(|(identity, _)| identity),
        )?;
        let request = builder
            .body(())
            .map_err(|error| invalid_config(error.to_string()))?;
        Ok(client.connect(request).await?)
    }

    fn prepare_with(
        &self,
        ctx: PrepareContext<'_>,
        identity: Option<&Identity>,
    ) -> Result<(http::Request<HttpBody>, Option<shape::tools::Aliases>), ChannelError> {
        let mut ctx = ctx;
        if ctx.operation.operation == Operation::CreateRealtimeCall {
            realtime::prepare_buffered(&mut ctx)?;
        }
        let config = CodexConfig::from_view(ctx.provider)?;
        let shape = !client_managed(&ctx.request.headers)
            && matches!(
                ctx.operation.operation,
                Operation::GenerateContent | Operation::StreamGenerateContent
            );
        let rules = config
            .enable_openai_magic_cache
            .then_some(cache::Rules::OpenAiResponses);
        let operation = ctx.operation.operation;
        let responses = request_kind(operation).is_some();
        let (mut builder, request) = self.build(ctx, false, identity)?;
        let mut aliases = None;
        let body = match request.body {
            HttpBody::Bytes(bytes) if responses => {
                let bytes = cache::shape(bytes, rules);
                HttpBody::Bytes(if shape {
                    let (bytes, mapping) = shape::request(&bytes)?;
                    aliases = Some(mapping);
                    bytes
                } else {
                    bytes
                })
            }
            HttpBody::Bytes(bytes)
                if matches!(operation, Operation::CreateImage | Operation::EditImage) =>
            {
                let bytes = if operation == Operation::CreateImage {
                    shape::images::create(&bytes)?
                } else {
                    shape::images::edit(&request.headers, &bytes)?
                };
                if let Some(headers) = builder.headers_mut() {
                    headers.insert(
                        header::CONTENT_TYPE,
                        HeaderValue::from_static("application/json"),
                    );
                    headers.remove(header::CONTENT_LENGTH);
                }
                HttpBody::Bytes(bytes)
            }
            other => other,
        };
        let request = builder
            .body(body)
            .map_err(|error| invalid_config(error.to_string()))?;
        Ok((request, aliases))
    }

    fn build<B>(
        &self,
        ctx: PrepareContext<'_, B>,
        websocket: bool,
        identity: Option<&Identity>,
    ) -> Result<(http::request::Builder, WireRequest<B>), ChannelError> {
        let config = CodexConfig::from_view(ctx.provider)?;
        let account = account(&ctx.credential)?;
        let mut request = ctx.request;
        let url = match ctx.endpoint_override {
            Some(url) => url.to_owned(),
            None => {
                let (base, _) = base_urls(ctx.provider);
                format!("{base}{}", operation_path(ctx.operation.operation)?)
            }
        };
        let url = if websocket {
            url.replacen("https://", "wss://", 1)
                .replacen("http://", "ws://", 1)
        } else {
            url
        };
        let query = request.query.take().filter(|q| !q.is_empty());
        let uri = match query {
            Some(q) => format!("{url}?{q}"),
            None => url,
        };
        let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLI_HEADERS)?;
        let mut headers = backend_headers(
            &config,
            &account,
            Some((&request.headers, allowlist.as_ref())),
        )?;
        if websocket {
            headers.insert(
                HeaderName::from_static("openai-beta"),
                HeaderValue::from_static(RESPONSES_WS_BETA),
            );
        }
        if let Some(identity) = identity {
            identity::apply(&mut headers, identity);
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

impl BaseChannel for Codex {
    fn id(&self) -> &'static str {
        ID
    }

    /// A ChatGPT account: both OAuth flows, a refreshable token, account
    /// limits at `/wham/usage`, the CLI's own backend endpoints, and Responses
    /// over a WebSocket as well as HTTP.
    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "OpenAI Codex (ChatGPT)",
            login_modes: vec![LoginMode::AuthorizationCode, LoginMode::DeviceCode],
            capabilities: ChannelCapabilities {
                refresh: true,
                quota_query: true,
                quota_reset: false,
                services: true,
                websocket: true,
            },
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Codex backend origin and prefix; defaults to https://chatgpt.com/backend-api/codex. Provider column, not config JSON.",
                ),
                ConfigKey::optional(
                    "issuer",
                    ConfigKeyKind::String,
                    "OAuth issuer used for login and refresh; defaults to https://auth.openai.com.",
                ),
                ConfigKey::optional(
                    "originator",
                    ConfigKeyKind::String,
                    "The `originator` the backend sees; defaults to codex_cli_rs.",
                ),
                ConfigKey::optional(
                    "user_agent",
                    ConfigKeyKind::String,
                    "Replaces the CLI-shaped User-Agent.",
                ),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static headers added to every backend request.",
                ),
                ConfigKey::optional(
                    "enable_openai_magic_cache",
                    ConfigKeyKind::Bool,
                    "Turn a client's magic cache string in a Responses body into prompt_cache_breakpoint.",
                ),
                ConfigKey::optional(
                    "synthesize_cli_identity",
                    ConfigKeyKind::Bool,
                    "Give non-CLI clients the CLI's session, thread, window and turn identity headers. On by default.",
                ),
            ]
            .into_iter()
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    fn default_connection(&self) -> Option<ConnectionConfig> {
        Some(default_connection())
    }

    fn native_dialects(&self, _provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                vec![Dialect::OpenAi, Dialect::OpenAiResponsesWebSocket]
            }
            Operation::ListModels
            | Operation::GetModel
            | Operation::CompactContent
            | Operation::SummarizeMemory
            | Operation::CreateRealtimeCall
            | Operation::ConnectRealtime
            | Operation::WebSearch
            | Operation::CreateImage
            | Operation::EditImage => vec![Dialect::OpenAi],
            _ => Vec::new(),
        }
    }

    /// Buffered Responses bodies are shaped for the magic cache strings; a
    /// streamed request body passes through untouched, as in `claudecode`.
    /// Plain preparation is stateless and synthesizes no identity; the
    /// Responses operations below read and write channel state around it.
    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        if matches!(
            ctx.operation.operation,
            Operation::ListModels | Operation::GetModel
        ) {
            return models::prepare(ctx);
        }
        self.prepare_with(ctx, None).map(|(request, _)| request)
    }

    fn prepare_connect(
        &self,
        ctx: PrepareContext<'_, ()>,
    ) -> Result<http::Request<()>, ChannelError> {
        if ctx.operation.operation == Operation::ConnectRealtime {
            return realtime::prepare_connect(ctx);
        }
        let (builder, _) = self.build(ctx, true, None)?;
        builder
            .body(())
            .map_err(|error| invalid_config(error.to_string()))
    }

    fn list_models<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(models::invoke(Operation::ListModels, context))
    }
    fn get_model<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(models::invoke(Operation::GetModel, context))
    }

    fn generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(self.responses_http(Operation::GenerateContent, context))
    }

    fn stream_generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(self.responses_http(Operation::StreamGenerateContent, context))
    }

    fn compact_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(self.responses_http(Operation::CompactContent, context))
    }

    fn generate_content_websocket<'a>(
        &'a self,
        context: OperationContext<'a, ()>,
    ) -> OperationFuture<'a, UpstreamConnection> {
        Box::pin(self.responses_websocket(Operation::GenerateContent, context))
    }

    fn stream_generate_content_websocket<'a>(
        &'a self,
        context: OperationContext<'a, ()>,
    ) -> OperationFuture<'a, UpstreamConnection> {
        Box::pin(self.responses_websocket(Operation::StreamGenerateContent, context))
    }

    fn create_realtime_call<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(realtime::create_call(self, context))
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
    }
    fn oauth_authorization_code(&self) -> Option<&dyn OAuthAuthorizationCode> {
        Some(self)
    }
    fn oauth_device_code(&self) -> Option<&dyn OAuthDeviceCode> {
        Some(self)
    }
    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }
    fn quota_model(&self) -> Option<&dyn QuotaModel> {
        Some(self)
    }
    fn quota_headers(&self) -> Option<&dyn QuotaHeaders> {
        Some(self)
    }
    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        Some(self)
    }
    fn usage_stream(&self) -> Option<&dyn UsageStream> {
        Some(self)
    }
    fn services(&self) -> Option<&dyn ChannelServices> {
        Some(self)
    }
}
