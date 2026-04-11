use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Extension, Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use futures_util::StreamExt;

use gproxy_sdk::protocol::openai::create_response::request::OpenAiCreateResponseRequest;
use gproxy_sdk::protocol::openai::create_response::websocket::types::OpenAiCreateResponseWebSocketClientMessage;
use gproxy_sdk::provider::engine::{
    ExecuteBody, ExecuteRequest, UpstreamWebSocket, WsConnectionResult, WsMessage, WsUpstreamMeta,
};
use gproxy_server::{AppState, OperationFamily, ProtocolKind};

use crate::auth::AuthenticatedUser;
use crate::error::HttpError;

#[derive(serde::Deserialize, Default)]
pub struct WsQueryParams {
    #[serde(default)]
    pub model: Option<String>,
}

#[derive(Debug, Clone)]
struct OpenAiWsModelSession {
    current_model: Option<String>,
}

impl OpenAiWsModelSession {
    fn new(initial_model: Option<String>) -> Self {
        Self {
            current_model: initial_model,
        }
    }

    fn active_model(&self) -> Option<String> {
        self.current_model.clone()
    }
}

struct AuthorizedOpenAiWsFrame {
    outbound_text: String,
    http_request_body: Vec<u8>,
    effective_model: String,
}

fn canonicalize_openai_ws_model(
    state: &AppState,
    provider_name: &str,
    model: &str,
) -> Result<String, HttpError> {
    let model = model.trim();
    if model.is_empty() {
        return Err(HttpError::bad_request("model must not be empty"));
    }

    if let Some(alias) = state.resolve_model_alias(model) {
        if alias.provider_name != provider_name {
            return Err(HttpError::bad_request(
                "websocket model alias resolves to a different provider",
            ));
        }
        return Ok(alias.model_id);
    }

    if let Some((prefixed_provider, model_id)) = model.split_once('/') {
        if prefixed_provider != provider_name {
            return Err(HttpError::bad_request(
                "websocket model provider prefix does not match route provider",
            ));
        }
        let model_id = model_id.trim();
        if model_id.is_empty() {
            return Err(HttpError::bad_request("model must not be empty"));
        }
        return Ok(model_id.to_string());
    }

    Ok(model.to_string())
}

fn authorize_openai_ws_client_frame(
    state: &AppState,
    user_id: i64,
    provider_name: &str,
    session: &mut OpenAiWsModelSession,
    raw_text: &str,
) -> Result<AuthorizedOpenAiWsFrame, HttpError> {
    let mut ws_message: OpenAiCreateResponseWebSocketClientMessage = serde_json::from_str(raw_text)
        .map_err(|e| HttpError::bad_request(format!("invalid OpenAI websocket frame: {e}")))?;

    let (effective_model, consume_request_budget) = match &mut ws_message {
        OpenAiCreateResponseWebSocketClientMessage::ResponseCreate(payload) => {
            let effective_model = match payload.request.model.as_deref() {
                Some(model) => canonicalize_openai_ws_model(state, provider_name, model)?,
                None => session.active_model().ok_or_else(|| {
                    HttpError::bad_request(
                        "missing model: provide ?model=... or include model in response.create",
                    )
                })?,
            };

            payload.request.model = Some(effective_model.clone());
            (effective_model, true)
        }
        OpenAiCreateResponseWebSocketClientMessage::ResponseAppend(_) => (
            session.active_model().ok_or_else(|| {
                HttpError::bad_request(
                    "missing active model: send response.create with a model first or connect with ?model=...",
                )
            })?,
            false,
        ),
    };

    if !state.check_model_permission(user_id, provider_name, &effective_model) {
        return Err(HttpError::forbidden("model not authorized for this user"));
    }

    let mut http_request = OpenAiCreateResponseRequest::try_from(&ws_message)
        .map_err(|e| HttpError::bad_request(format!("invalid OpenAI websocket frame: {e}")))?;
    http_request.body.model = Some(effective_model.clone());
    http_request.body.stream = Some(true);

    let http_request_body = serde_json::to_vec(&http_request.body)
        .map_err(|_| HttpError::bad_request("failed to encode OpenAI websocket request"))?;

    if consume_request_budget
        && let Err(_rejection) = state.check_rate_limit_request(
            user_id,
            &effective_model,
            super::handler::extract_requested_total_tokens(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
                &http_request_body,
            ),
        )
    {
        return Err(HttpError::too_many_requests(
            "rate limit exceeded".to_string(),
        ));
    }

    session.current_model = Some(effective_model.clone());

    let outbound_text = serde_json::to_string(&ws_message)
        .map_err(|_| HttpError::bad_request("failed to encode OpenAI websocket frame"))?;

    Ok(AuthorizedOpenAiWsFrame {
        outbound_text,
        http_request_body,
        effective_model,
    })
}

/// Authorize a Gemini Live client frame.
///
/// Parses the JSON to detect `setup.model`. If present, validates the model
/// against user permissions and enforces rate limiting. Returns the model
/// name from the frame (if any) or Ok(None) for non-setup messages.
fn authorize_gemini_ws_client_frame(
    state: &AppState,
    user_id: i64,
    provider_name: &str,
    raw_text: &str,
) -> Result<Option<String>, HttpError> {
    // Quick check: does the frame contain "setup"?
    if !raw_text.contains("\"setup\"") {
        return Ok(None);
    }

    // Try to parse as a setup message
    let parsed: serde_json::Value = serde_json::from_str(raw_text)
        .map_err(|e| HttpError::bad_request(format!("invalid JSON: {e}")))?;

    let Some(setup) = parsed.get("setup") else {
        return Ok(None);
    };
    let Some(model_raw) = setup.get("model").and_then(|m| m.as_str()) else {
        return Ok(None);
    };

    // Gemini uses "models/{model}" format — strip the prefix
    let model = model_raw.strip_prefix("models/").unwrap_or(model_raw);

    // Permission check
    if !state.check_model_permission(user_id, provider_name, model) {
        return Err(HttpError::forbidden("model not authorized for this user"));
    }

    // Rate limit check (per-frame for Gemini Live setup messages)
    if let Err(_rejection) = state.check_rate_limit_request(user_id, model, None) {
        return Err(HttpError::too_many_requests(
            "rate limit exceeded".to_string(),
        ));
    }

    Ok(Some(model.to_string()))
}

/// OpenAI Responses WebSocket: `GET /{provider}/v1/responses`
pub async fn openai_responses_ws(
    State(state): State<Arc<AppState>>,
    Path(provider_name): Path<String>,
    Query(params): Query<WsQueryParams>,
    Extension(authenticated): Extension<AuthenticatedUser>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, HttpError> {
    let user_key = authenticated.0;
    let model = params
        .model
        .as_deref()
        .map(|model| canonicalize_openai_ws_model(&state, &provider_name, model))
        .transpose()?;

    // Permission check
    if let Some(ref m) = model
        && !state.check_model_permission(user_key.user_id, &provider_name, m)
    {
        return Err(HttpError::forbidden("model not authorized for this user"));
    }

    let user_id = user_key.user_id;
    let user_key_id = user_key.id;
    let headers_clone = headers.clone();

    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_openai_ws(
            state,
            provider_name,
            model,
            user_id,
            user_key_id,
            headers_clone,
            socket,
        )
        .await
        {
            tracing::warn!(error = %e, "openai responses websocket error");
        }
    }))
}

/// Gemini Live WebSocket: `GET /{provider}/v1beta/models/{target}`
pub async fn gemini_live(
    State(state): State<Arc<AppState>>,
    Path((provider_name, target)): Path<(String, String)>,
    Extension(authenticated): Extension<AuthenticatedUser>,
    _headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, HttpError> {
    let user_key = authenticated.0;

    // Extract model from path (e.g. "gemini-2.0-flash:streamGenerateContent")
    let model = target.split(':').next().map(String::from);

    // Permission check
    if let Some(ref m) = model
        && !state.check_model_permission(user_key.user_id, &provider_name, m)
    {
        return Err(HttpError::forbidden("model not authorized for this user"));
    }

    // Rate limit check
    if let Some(ref m) = model
        && let Err(_rejection) = state.check_rate_limit_request(user_key.user_id, m, None)
    {
        return Err(HttpError::too_many_requests(
            "rate limit exceeded".to_string(),
        ));
    }

    let user_id = user_key.user_id;
    let user_key_id = user_key.id;
    let path = format!("/v1beta/models/{target}");

    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_gemini_live_ws(
            state,
            provider_name,
            model,
            user_id,
            user_key_id,
            path,
            socket,
        )
        .await
        {
            tracing::warn!(error = %e, "gemini live websocket error");
        }
    }))
}

/// OpenAI Responses WebSocket (unscoped): `GET /v1/responses`
pub async fn openai_responses_ws_unscoped(
    State(state): State<Arc<AppState>>,
    Query(params): Query<WsQueryParams>,
    Extension(authenticated): Extension<AuthenticatedUser>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, HttpError> {
    let user_key = authenticated.0;
    let model = params.model.clone();

    let Some(model_name) = &model else {
        return Err(HttpError::bad_request(
            "missing model query parameter for unscoped websocket",
        ));
    };

    // Resolve provider from model (alias or provider/model format)
    let (target_provider, target_model) = if let Some(alias) = state.resolve_model_alias(model_name)
    {
        (alias.provider_name, Some(alias.model_id))
    } else if let Some((provider, model)) = model_name.split_once('/') {
        (provider.to_string(), Some(model.to_string()))
    } else {
        return Err(HttpError::bad_request(
            "model must have provider prefix (provider/model) or match an alias",
        ));
    };

    // Permission check
    if let Some(ref m) = target_model
        && !state.check_model_permission(user_key.user_id, &target_provider, m)
    {
        return Err(HttpError::forbidden("model not authorized for this user"));
    }

    let user_id = user_key.user_id;
    let user_key_id = user_key.id;
    let headers_clone = headers.clone();

    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(e) = handle_openai_ws(
            state,
            target_provider,
            target_model,
            user_id,
            user_key_id,
            headers_clone,
            socket,
        )
        .await
        {
            tracing::warn!(error = %e, "openai responses websocket error (unscoped)");
        }
    }))
}

// ---------------------------------------------------------------------------
// OpenAI: try WS → fallback to HTTP SSE
// ---------------------------------------------------------------------------

async fn handle_openai_ws(
    state: Arc<AppState>,
    provider_name: String,
    model: Option<String>,
    user_id: i64,
    user_key_id: i64,
    headers: HeaderMap,
    mut downstream: WebSocket,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let trace_id = super::handler::generate_trace_id();
    // Try upstream WebSocket via SDK
    let ctx = WsBridgeContext {
        state: state.clone(),
        provider_name: provider_name.clone(),
        user_id,
        user_key_id,
        model: model.clone(),
        credential_index: None,
        operation: OperationFamily::OpenAiResponseWebSocket,
        protocol: ProtocolKind::OpenAi,
        trace_id,
    };

    match state
        .engine()
        .connect_upstream_ws(
            &provider_name,
            OperationFamily::OpenAiResponseWebSocket,
            ProtocolKind::OpenAi,
            "/v1/responses",
            model.as_deref(),
        )
        .await
    {
        Ok(WsConnectionResult::Connected(mut upstream, ws_meta)) => {
            tracing::info!(trace_id, provider = %provider_name, "websocket bridge active (passthrough)");
            record_ws_upstream_log(&state, trace_id, &provider_name, &ws_meta).await;
            let ctx = WsBridgeContext {
                credential_index: Some(ws_meta.credential_index),
                ..ctx
            };
            let mut bridge = super::ws_bridge::PassthroughBridge::new("openai");
            run_ws_bridge_with_protocol(&mut downstream, &mut upstream, &mut bridge, &ctx).await;
        }
        Ok(WsConnectionResult::NeedsProtocolBridge {
            mut upstream,
            dst_protocol,
            meta: ws_meta,
            ..
        }) => {
            tracing::info!(trace_id, provider = %provider_name, dst = %dst_protocol, "websocket bridge active (cross-protocol)");
            record_ws_upstream_log(&state, trace_id, &provider_name, &ws_meta).await;
            let ctx = WsBridgeContext {
                credential_index: Some(ws_meta.credential_index),
                ..ctx
            };
            let mut bridge: Box<dyn super::ws_bridge::WsProtocolBridge> = match dst_protocol {
                ProtocolKind::Gemini => {
                    Box::new(super::ws_bridge::OpenAiToGeminiBridge::new(model.clone()))
                }
                _ => {
                    tracing::warn!(dst = %dst_protocol, "unsupported cross-protocol WS bridge");
                    return Ok(());
                }
            };
            run_ws_bridge_with_protocol(&mut downstream, &mut upstream, bridge.as_mut(), &ctx)
                .await;
        }
        Err(e) => {
            tracing::info!(trace_id, provider = %provider_name, error = %e, "WS failed, HTTP SSE fallback");
            run_http_sse_fallback(&ctx, headers, &mut downstream).await?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Gemini Live: WS only (no HTTP fallback)
// ---------------------------------------------------------------------------

async fn handle_gemini_live_ws(
    state: Arc<AppState>,
    provider_name: String,
    model: Option<String>,
    user_id: i64,
    user_key_id: i64,
    path: String,
    mut downstream: WebSocket,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let trace_id = super::handler::generate_trace_id();
    let ctx = WsBridgeContext {
        state: state.clone(),
        provider_name: provider_name.clone(),
        user_id,
        user_key_id,
        model: model.clone(),
        credential_index: None,
        operation: OperationFamily::GeminiLive,
        protocol: ProtocolKind::Gemini,
        trace_id,
    };

    let result = state
        .engine()
        .connect_upstream_ws(
            &provider_name,
            OperationFamily::GeminiLive,
            ProtocolKind::Gemini,
            &path,
            model.as_deref(),
        )
        .await
        .map_err(|e| format!("gemini live connect failed: {e}"))?;

    match result {
        WsConnectionResult::Connected(mut upstream, ws_meta) => {
            tracing::info!(trace_id, provider = %provider_name, "gemini live websocket bridge active (passthrough)");
            record_ws_upstream_log(&state, trace_id, &provider_name, &ws_meta).await;
            let ctx = WsBridgeContext {
                credential_index: Some(ws_meta.credential_index),
                ..ctx
            };
            let mut bridge = super::ws_bridge::PassthroughBridge::new("gemini");
            run_ws_bridge_with_protocol(&mut downstream, &mut upstream, &mut bridge, &ctx).await;
        }
        WsConnectionResult::NeedsProtocolBridge {
            mut upstream,
            dst_protocol,
            meta: ws_meta,
            ..
        } => {
            tracing::info!(trace_id, provider = %provider_name, dst = %dst_protocol, "gemini live websocket bridge active (cross-protocol)");
            record_ws_upstream_log(&state, trace_id, &provider_name, &ws_meta).await;
            let ctx = WsBridgeContext {
                credential_index: Some(ws_meta.credential_index),
                ..ctx
            };
            let mut bridge: Box<dyn super::ws_bridge::WsProtocolBridge> = match dst_protocol {
                ProtocolKind::OpenAi | ProtocolKind::OpenAiResponse => {
                    Box::new(super::ws_bridge::GeminiToOpenAiBridge::new(model.clone()))
                }
                _ => {
                    tracing::warn!(dst = %dst_protocol, "unsupported cross-protocol WS bridge");
                    return Ok(());
                }
            };
            run_ws_bridge_with_protocol(&mut downstream, &mut upstream, bridge.as_mut(), &ctx)
                .await;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Bidirectional WS bridge with protocol conversion and usage tracking
// ---------------------------------------------------------------------------

struct WsBridgeContext {
    state: Arc<AppState>,
    provider_name: String,
    user_id: i64,
    user_key_id: i64,
    model: Option<String>,
    credential_index: Option<usize>,
    operation: OperationFamily,
    protocol: ProtocolKind,
    trace_id: i64,
}

async fn run_ws_bridge_with_protocol(
    downstream: &mut WebSocket,
    upstream: &mut UpstreamWebSocket,
    bridge: &mut dyn super::ws_bridge::WsProtocolBridge,
    ctx: &WsBridgeContext,
) {
    let mut openai_session = matches!(ctx.operation, OperationFamily::OpenAiResponseWebSocket)
        .then(|| OpenAiWsModelSession::new(ctx.model.clone()));

    // Per-model usage segments: when the model changes, we snapshot the
    // accumulated usage for the old model and start fresh for the new one.
    let mut usage_segments: Vec<(Option<String>, gproxy_sdk::provider::engine::Usage)> = Vec::new();

    // Collect WS messages for logging (only if downstream log + body enabled)
    let collect_body = {
        let cfg = ctx.state.config();
        cfg.enable_downstream_log && cfg.enable_downstream_log_body
    };
    let mut ds_messages: Vec<String> = Vec::new(); // client → server (request body)
    let mut us_messages: Vec<String> = Vec::new(); // server → client (response body)

    loop {
        tokio::select! {
            ds_msg = downstream.recv() => {
                match ds_msg {
                    Some(Ok(Message::Text(t))) => {
                        let mut outbound_text = t.to_string();
                        if let Some(session) = openai_session.as_mut() {
                            let old_model = session.active_model();
                            match authorize_openai_ws_client_frame(
                                &ctx.state,
                                ctx.user_id,
                                &ctx.provider_name,
                                session,
                                &outbound_text,
                            ) {
                                Ok(frame) => {
                                    // On model change, snapshot usage for the old model.
                                    let new_model = session.active_model();
                                    if old_model != new_model && let Some(usage) = bridge.take_accumulated_usage() {
                                        usage_segments.push((old_model, usage));
                                    }
                                    outbound_text = frame.outbound_text;
                                }
                                Err(err) => {
                                    send_ws_error(downstream, &err.message).await;
                                    break;
                                }
                            }
                        } else {
                            // Gemini Live: authorize setup frames (model switch + rate limit)
                            match authorize_gemini_ws_client_frame(
                                &ctx.state,
                                ctx.user_id,
                                &ctx.provider_name,
                                &outbound_text,
                            ) {
                                Ok(Some(model)) => {
                                    tracing::debug!(
                                        user_id = ctx.user_id,
                                        %model,
                                        "gemini live setup frame authorized"
                                    );
                                }
                                Ok(None) => {} // non-setup frame, pass through
                                Err(err) => {
                                    send_ws_error(downstream, &err.message).await;
                                    break;
                                }
                            }
                        }

                        if collect_body { ds_messages.push(outbound_text.clone()); }
                        match bridge.convert_client_message(&outbound_text) {
                            Ok(msgs) => {
                                for msg in msgs {
                                    if upstream.send(WsMessage::text(msg)).await.is_err() {
                                        return;
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "ws bridge: client message conversion failed");
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Binary(b))) => {
                        if upstream.send(WsMessage::binary(b.to_vec())).await.is_err() { break; }
                    }
                    Some(Ok(Message::Ping(p))) => {
                        if upstream.send(WsMessage::ping(p.to_vec())).await.is_err() { break; }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => continue,
                }
            }
            us_msg = upstream.recv() => {
                match us_msg {
                    Some(Ok(WsMessage::Text(t))) => {
                        let text = t.to_string();
                        if collect_body { us_messages.push(text.clone()); }
                        match bridge.convert_server_message(&text) {
                            Ok((msgs, _usage)) => {
                                for msg in msgs {
                                    if downstream.send(Message::Text(msg.into())).await.is_err() {
                                        return;
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "ws bridge: server message conversion failed");
                                break;
                            }
                        }
                    }
                    Some(Ok(WsMessage::Binary(b))) => {
                        if downstream.send(Message::Binary(b)).await.is_err() { break; }
                    }
                    Some(Ok(WsMessage::Ping(p))) => {
                        if downstream.send(Message::Ping(p)).await.is_err() { break; }
                    }
                    Some(Ok(WsMessage::Close(_))) | None => break,
                    _ => continue,
                }
            }
        }
    }

    // Push the final model segment (remaining accumulated usage).
    if let Some(usage) = bridge.final_usage() {
        let model = openai_session
            .as_ref()
            .and_then(|session| session.active_model())
            .or_else(|| ctx.model.clone());
        usage_segments.push((model, usage));
    }

    // Record one usage entry per model used in this session.
    for (model, usage) in &usage_segments {
        let usage_ctx = super::handler::UsageRecordContext {
            state: ctx.state.clone(),
            user_id: ctx.user_id,
            user_key_id: ctx.user_key_id,
            provider_name: ctx.provider_name.clone(),
            credential_index: ctx.credential_index,
            precomputed_cost: None,
            model: model.clone(),
            billing_context: None,
            operation: ctx.operation,
            protocol: ctx.protocol,
            downstream_trace_id: Some(ctx.trace_id),
        };
        super::handler::record_usage(&usage_ctx, usage).await;
    }

    // Record downstream log for WS session (request = client messages, response = server messages)
    let config = ctx.state.config();
    if config.enable_downstream_log {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let (request_body, response_body) = if config.enable_downstream_log_body {
            (
                serde_json::to_vec(&ds_messages).ok(),
                serde_json::to_vec(&us_messages).ok(),
            )
        } else {
            (None, None)
        };
        let _ = ctx
            .state
            .storage()
            .apply_write_event(gproxy_storage::StorageWriteEvent::UpsertDownstreamRequest(
                gproxy_storage::DownstreamRequestWrite {
                    trace_id: ctx.trace_id,
                    at_unix_ms: now_ms,
                    internal: false,
                    user_id: Some(ctx.user_id),
                    user_key_id: Some(ctx.user_key_id),
                    request_method: "WEBSOCKET".to_string(),
                    request_headers_json: String::new(),
                    request_path: format!("ws://{}/{}", ctx.operation, ctx.protocol),
                    request_query: None,
                    request_body,
                    response_status: Some(101),
                    response_headers_json: String::new(),
                    response_body,
                },
            ))
            .await;
    }

    // Record upstream WS session log with accumulated messages
    if config.enable_upstream_log {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let provider_id = ctx.state.provider_id_for_name(&ctx.provider_name);
        let credential_id = ctx
            .credential_index
            .and_then(|i| ctx.state.credential_id_for_index(&ctx.provider_name, i));
        let (req_body, resp_body) = if config.enable_upstream_log_body {
            (
                serde_json::to_vec(&ds_messages).ok(),
                serde_json::to_vec(&us_messages).ok(),
            )
        } else {
            (None, None)
        };
        let _ = ctx
            .state
            .storage()
            .apply_write_event(gproxy_storage::StorageWriteEvent::UpsertUpstreamRequest(
                gproxy_storage::UpstreamRequestWrite {
                    downstream_trace_id: Some(ctx.trace_id),
                    at_unix_ms: now_ms,
                    internal: false,
                    provider_id,
                    credential_id,
                    request_method: "WEBSOCKET".to_string(),
                    request_headers_json: "[]".to_string(),
                    request_url: None,
                    request_body: req_body,
                    response_status: Some(101),
                    response_headers_json: "[]".to_string(),
                    response_body: resp_body,
                },
            ))
            .await;
    }
}

// ---------------------------------------------------------------------------
// HTTP SSE fallback
// ---------------------------------------------------------------------------

async fn run_http_sse_fallback(
    ctx: &WsBridgeContext,
    headers: HeaderMap,
    downstream: &mut WebSocket,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut model_session = OpenAiWsModelSession::new(ctx.model.clone());

    // Active stream from a previous request (if any)
    let mut active_stream: Option<(
        gproxy_sdk::provider::engine::ExecuteBodyStream,
        gproxy_sdk::protocol::stream::SseToNdjsonRewriter,
    )> = None;

    loop {
        // If we have an active stream, select between downstream messages and upstream chunks
        if let Some((ref mut stream, ref mut decoder)) = active_stream {
            tokio::select! {
                // New downstream message — abort current stream, process new request
                ds_msg = downstream.recv() => {
                    let text = match ds_msg {
                        Some(Ok(Message::Text(t))) => t.to_string(),
                        Some(Ok(Message::Binary(b))) => String::from_utf8_lossy(&b).into_owned(),
                        Some(Ok(Message::Close(_))) | None => break,
                        Some(Ok(Message::Ping(p))) => {
                            let _ = downstream.send(Message::Pong(p)).await;
                            continue;
                        }
                        _ => continue,
                    };
                    // Drop current stream, start new request
                    drop(active_stream.take());
                    active_stream =
                        start_http_request(ctx, &headers, &text, downstream, &mut model_session)
                            .await?;
                }
                // Upstream chunk — forward to downstream
                chunk = stream.next() => {
                    match chunk {
                        Some(Ok(chunk)) => {
                            for event in split_sse_events(&decoder.push_chunk(&chunk)) {
                                if downstream
                                    .send(Message::Text(
                                        String::from_utf8_lossy(&event).into_owned().into(),
                                    ))
                                    .await
                                    .is_err()
                                {
                                    return Ok(());
                                }
                            }
                        }
                        Some(Err(e)) => {
                            send_ws_error(downstream, &e.to_string()).await;
                            active_stream = None;
                        }
                        None => {
                            // Stream finished — flush remaining events
                            for event in split_sse_events(&decoder.finish()) {
                                if downstream
                                    .send(Message::Text(
                                        String::from_utf8_lossy(&event).into_owned().into(),
                                    ))
                                    .await
                                    .is_err()
                                {
                                    return Ok(());
                                }
                            }
                            active_stream = None;
                        }
                    }
                }
            }
        } else {
            // No active stream — wait for a downstream message
            let text = match downstream.recv().await {
                Some(Ok(Message::Text(t))) => t.to_string(),
                Some(Ok(Message::Binary(b))) => String::from_utf8_lossy(&b).into_owned(),
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(Message::Ping(p))) => {
                    let _ = downstream.send(Message::Pong(p)).await;
                    continue;
                }
                _ => continue,
            };
            active_stream =
                start_http_request(ctx, &headers, &text, downstream, &mut model_session).await?;
        }
    }
    Ok(())
}

/// Parse a downstream WS message, execute via engine, and return the active stream (if streaming).
/// Full (non-streaming) responses are sent directly and return None.
async fn start_http_request(
    ctx: &WsBridgeContext,
    headers: &HeaderMap,
    text: &str,
    downstream: &mut WebSocket,
    model_session: &mut OpenAiWsModelSession,
) -> Result<
    Option<(
        gproxy_sdk::provider::engine::ExecuteBodyStream,
        gproxy_sdk::protocol::stream::SseToNdjsonRewriter,
    )>,
    Box<dyn std::error::Error + Send + Sync>,
> {
    let frame = match authorize_openai_ws_client_frame(
        &ctx.state,
        ctx.user_id,
        &ctx.provider_name,
        model_session,
        text,
    ) {
        Ok(frame) => frame,
        Err(err) => {
            send_ws_error(downstream, &err.message).await;
            return Ok(None);
        }
    };
    let AuthorizedOpenAiWsFrame {
        http_request_body,
        effective_model,
        ..
    } = frame;

    let operation = OperationFamily::StreamGenerateContent;
    let protocol = ProtocolKind::OpenAiResponse;

    let result = ctx
        .state
        .engine()
        .execute(ExecuteRequest {
            provider: ctx.provider_name.clone(),
            operation,
            protocol,
            body: http_request_body,
            headers: headers.clone(),
            model: Some(effective_model.clone()),
            forced_credential_index: None,
            response_model_override: None,
        })
        .await;

    match result {
        Ok(result) => {
            if let Some(ref usage) = result.usage {
                let usage_ctx = super::handler::UsageRecordContext {
                    state: ctx.state.clone(),
                    user_id: ctx.user_id,
                    user_key_id: ctx.user_key_id,
                    provider_name: ctx.provider_name.clone(),
                    credential_index: Some(result.credential_index),
                    precomputed_cost: result.cost,
                    model: Some(effective_model.clone()),
                    billing_context: result.billing_context.clone(),
                    operation,
                    protocol,
                    downstream_trace_id: Some(ctx.trace_id),
                };
                super::handler::record_usage(&usage_ctx, usage).await;
            }

            match result.body {
                ExecuteBody::Full(body) => {
                    let mut decoder = gproxy_sdk::protocol::stream::SseToNdjsonRewriter::default();
                    let mut chunks = Vec::new();
                    chunks.extend(split_sse_events(&decoder.push_chunk(&body)));
                    chunks.extend(split_sse_events(&decoder.finish()));
                    for chunk in chunks {
                        if downstream
                            .send(Message::Text(
                                String::from_utf8_lossy(&chunk).into_owned().into(),
                            ))
                            .await
                            .is_err()
                        {
                            return Ok(None);
                        }
                    }
                    Ok(None)
                }
                ExecuteBody::Stream(stream) => {
                    let decoder = gproxy_sdk::protocol::stream::SseToNdjsonRewriter::default();
                    Ok(Some((stream, decoder)))
                }
            }
        }
        Err(e) => {
            send_ws_error(downstream, &e.to_string()).await;
            Ok(None)
        }
    }
}

async fn send_ws_error(socket: &mut WebSocket, message: &str) {
    let error = serde_json::json!({
        "type": "error",
        "error": {
            "type": "server_error",
            "code": "websocket_proxy_error",
            "message": message,
        }
    });
    let _ = socket.send(Message::Text(error.to_string().into())).await;
}

use gproxy_sdk::protocol::stream::split_lines_owned as split_sse_events;

async fn record_ws_upstream_log(
    state: &AppState,
    trace_id: i64,
    provider_name: &str,
    meta: &WsUpstreamMeta,
) {
    if !state.config().enable_upstream_log {
        return;
    }
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let headers_json =
        serde_json::to_string(&meta.request_headers).unwrap_or_else(|_| "[]".to_string());
    let provider_id = state.provider_id_for_name(provider_name);
    let credential_id = state.credential_id_for_index(provider_name, meta.credential_index);
    let _ = state
        .storage()
        .apply_write_event(gproxy_storage::StorageWriteEvent::UpsertUpstreamRequest(
            gproxy_storage::UpstreamRequestWrite {
                downstream_trace_id: Some(trace_id),
                at_unix_ms: now_ms,
                internal: false,
                provider_id,
                credential_id,
                request_method: "WEBSOCKET".to_string(),
                request_headers_json: headers_json,
                request_url: Some(meta.url.clone()),
                request_body: None,
                response_status: Some(meta.response_status as i32),
                response_headers_json: "[]".to_string(),
                response_body: None,
            },
        ))
        .await;
}
