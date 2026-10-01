//! Generation across the Responses WebSocket boundary, both ways.
//!
//! - An HTTP streaming client (Chat, Claude, Gemini) whose upstream speaks
//!   Responses over WebSocket: one connection, one `response.create` turn,
//!   client-dialect bytes driven from a task that owns the socket.
//! - A Responses WebSocket client whose upstream speaks Chat, Claude or
//!   Gemini over HTTP: core fabricates the accepted socket, decodes each
//!   `response.create` message, converts it as one HTTP stream and writes
//!   the Responses server events back as text frames on the same lane.
//!
//! HTTP segments reuse the admitted retry plan for explicit rejections. Once
//! accepted, the stream is never replayed after a transport or event failure.

mod bridge;
mod responses_http;

use super::{
    Call, Converted,
    generate::{
        OwnedState, decode, namespace, responses_over_chat_context, responses_over_claude_facts,
        responses_over_gemini_facts, stream_headers, stream_settings, transport,
    },
};
use crate::{
    AttemptContext, AttemptUpstream, Core, CoreError, CoreResult, ProtocolState, RequestContext,
    StateScope, TraceEvent, UsageState, WebSocketExecution,
    api::{Execution, UsageCompletion, lifecycle::now_ms},
    execute::Funnel,
};
use gproxy_protocol::{
    Dialect, Operation, OperationKey, WireRequest, WireResponse,
    adapt::{
        generate::{
            GenerationIdentity, GenerationStateAccess,
            chat_responses::{ChatViaResponses, ResponsesViaChat},
            claude_responses::{ClaudeViaResponses, ResponsesViaClaude},
            gemini_responses::{GeminiViaResponses, ResponsesViaGemini},
            stream::{
                StreamInvocation, StreamSettings, StreamStart, StreamTarget,
                bridge::StreamBridge,
                websocket::{GenerationWsConnect, connect, decode_message},
            },
        },
        responses_ws::ResponsesWsLimits,
    },
    capability::UpstreamConnection,
    connection::{ByteStream, TransportError, WebSocket, WsFrame},
    transform::{TransformError, identity::IdentityTarget},
    wire::openai::{
        chat as h,
        responses::{
            stream::StreamEvent,
            websocket::{ClientEvent, ErrorDetail, ErrorMessage, ErrorMessageType},
        },
    },
};
use gproxy_seaorm::BatchConnectionTrait;
use http::{HeaderMap, StatusCode};
use std::{collections::HashSet, sync::Arc, time::SystemTime};
use tokio::sync::mpsc;

/// The upstream key for a Responses WebSocket generation.
const WS_KEY: OperationKey = OperationKey {
    operation: Operation::StreamGenerateContent,
    dialect: Dialect::OpenAiResponsesWebSocket,
};

/// Frames buffered between the conversion task and the caller.
const FRAME_QUEUE: usize = 16;

fn handshake_request() -> WireRequest<()> {
    WireRequest {
        method: http::Method::GET,
        path: "/v1/responses".into(),
        query: None,
        headers: HeaderMap::new(),
        body: (),
    }
}

/// HTTP streaming client over a Responses WebSocket upstream.
pub(crate) async fn over_websocket<C: BatchConnectionTrait + Send + Sync + 'static>(
    call: &Call<'_, C>,
    settings: StreamSettings,
) -> Result<Converted, TransformError> {
    if call.client.dialect == Dialect::OpenAi {
        return responses_http::over_websocket(call, settings).await;
    }
    let client = call.client.dialect;
    let upstream = call.upstream;
    let body = call.body();
    let limits = call.limits;
    // Identities and continuation state are Responses'; the socket is transport.
    let state = &call.generation_state_for(Dialect::OpenAi)?;
    let endpoint = super::generate_endpoint(Dialect::OpenAi, &state.target.model, true)?;
    let identities = GenerationIdentity::new(namespace(), namespace(), client, Dialect::OpenAi)?;
    let stream_target = StreamTarget {
        endpoint,
        identities,
    };
    let client_framing = settings.client_framing;
    macro_rules! run {
        ($pair:ty, $input:ty) => {{
            let input: $input = decode(body, limits)?;
            run!(@drive <$pair>::prepare_stream(input, stream_target, settings, state))
        }};
        ($pair:ty, $input:ty, $ctx:expr) => {{
            let input: $input = decode(body, limits)?;
            let context = $ctx;
            run!(@drive <$pair>::prepare_stream(input, stream_target, context, settings, state))
        }};
        (@drive $prepare:expr) => {{
            Box::pin(async move {
                let invocation = $prepare.await?;
                let session = match connect(
                    upstream,
                    &WS_KEY,
                    handshake_request(),
                    ResponsesWsLimits::default(),
                    state,
                )
                .await
                {
                    Ok(GenerationWsConnect::Connected { session, .. }) => session,
                    Ok(GenerationWsConnect::Rejected(response)) => {
                        return Ok(Converted::Rejected(response));
                    }
                    Err(error) => return Err(error.error),
                };
                let owned = OwnedState::capture(call, state);
                let (tx, rx) = mpsc::channel::<Result<_, TransportError>>(FRAME_QUEUE);
                // The turn borrows the invocation and the session, so a task
                // owns all three and feeds the client stream through a queue.
                // The caller dropping the stream closes the queue, which ends
                // the task and releases the socket.
                crate::rt::spawn(async move {
                    let mut invocation = invocation;
                    let mut session = session;
                    let access = owned.access();
                    let mut turn = match invocation.start_websocket(&mut session, &access).await {
                        Ok(turn) => turn,
                        Err(error) => {
                            let _ = tx.send(Err(transport(error))).await;
                            return;
                        }
                    };
                    loop {
                        match turn.next(&access).await {
                            Ok(Some(chunk)) => {
                                if !chunk.bytes.is_empty()
                                    && tx.send(Ok(chunk.bytes)).await.is_err()
                                {
                                    return;
                                }
                            }
                            Ok(None) => return,
                            Err(error) => {
                                let _ = tx.send(Err(transport(error))).await;
                                return;
                            }
                        }
                    }
                });
                let stream: ByteStream = Box::pin(futures_util::stream::unfold(
                    rx,
                    |mut rx| async move { rx.recv().await.map(|item| (item, rx)) },
                ));
                Ok(Converted::Stream(WireResponse {
                    status: StatusCode::OK,
                    headers: stream_headers(client_framing),
                    body: stream,
                }))
            })
            .await
        }};
    }
    match client {
        Dialect::OpenAiChat => run!(ChatViaResponses, h::GenerateContentRequestBody),
        Dialect::Claude => run!(
            ClaudeViaResponses,
            gproxy_protocol::wire::claude::generate_content::GenerateContentRequestBody,
            gproxy_protocol::transform::generate::claude_responses::stream::ResponsesToClaudeContext::default()
        ),
        Dialect::Gemini => run!(
            GeminiViaResponses,
            gproxy_protocol::wire::gemini::GenerateContentRequestBody,
            gproxy_protocol::transform::generate::gemini_responses::stream::ResponsesToGeminiContext::default()
        ),
        other => {
            Err(TransformError::unsupported(
                "generate.websocket",
                format!("no generation from {other:?} over a Responses WebSocket upstream"),
            ))
        }
    }
}

fn error_frame(status: u16, lane: Option<String>, message: String) -> WsFrame {
    let message = ErrorMessage {
        type_: ErrorMessageType::Error,
        status,
        stream_id: lane,
        error: ErrorDetail {
            type_: Some("invalid_request_error".into()),
            code: None,
            message,
            param: None,
        },
    };
    WsFrame::Text(serde_json::to_string(&message).unwrap_or_default())
}

fn closed() -> TransportError {
    Box::new(std::io::Error::other("client connection closed"))
}

type Outgoing = mpsc::Sender<Result<WsFrame, TransportError>>;

/// One converted turn: start the HTTP stream and forward each Responses
/// server event as a text frame. `Ok(false)` means the client went away.
async fn turn<B, C>(
    mut invocation: StreamInvocation<B>,
    upstream: &AttemptUpstream,
    key: &OperationKey,
    access: &GenerationStateAccess<'_, ProtocolState<C>>,
    lane: Option<String>,
    out: &Outgoing,
) -> Result<bool, TransformError>
where
    B: StreamBridge<ClientEvent = StreamEvent>,
    C: BatchConnectionTrait + Send + Sync,
{
    match invocation.start(upstream, key, access).await? {
        StreamStart::Rejected(response) => {
            let frame = error_frame(
                response.status.as_u16(),
                lane,
                String::from_utf8_lossy(&response.body).into_owned(),
            );
            Ok(out.send(Ok(frame)).await.is_ok())
        }
        StreamStart::Streaming(_) => {
            while let Some(chunk) = invocation.next(access).await? {
                if chunk.bytes.is_empty() {
                    continue;
                }
                let text = String::from_utf8(chunk.bytes.to_vec())
                    .map_err(|_| TransformError::shape("responses.websocket", "non-UTF-8 event"))?;
                if out.send(Ok(WsFrame::Text(text))).await.is_err() {
                    return Ok(false);
                }
            }
            Ok(true)
        }
    }
}

/// Responses WebSocket client over an HTTP upstream speaking `target`.
/// One credential is selected for the whole connection.
pub(crate) async fn serve<C: BatchConnectionTrait + Send + Sync + 'static>(
    core: &Core<C>,
    request: Arc<RequestContext>,
    wire: &WireRequest<()>,
    funnel: Arc<Funnel>,
    completion: UsageCompletion,
    target: Dialect,
    session: Option<crate::ResponsesSession>,
) -> CoreResult<WebSocketExecution> {
    if !matches!(
        target,
        Dialect::OpenAi | Dialect::OpenAiChat | Dialect::Claude | Dialect::Gemini
    ) {
        funnel.finish(UsageState::Failed).await;
        return Err(CoreError::Transform(TransformError::unsupported(
            "responses.websocket",
            format!("no generation from a Responses WebSocket client to {target:?}"),
        )));
    }
    let now = now_ms();
    let selection = match core.select_credential(&request, &HashSet::new(), now).await {
        Ok(selection) => selection,
        Err(error) => {
            funnel.finish(UsageState::Failed).await;
            return Err(error);
        }
    };
    let snapshot = request.snapshot.clone();
    let limits = snapshot.limits;
    let provider = request.target.provider.clone();
    let credential = selection.credential;
    if let Some(session) = &session {
        session.bind(
            request.clone(),
            credential.clone(),
            target,
            None,
            selection.assignment.as_ref().map(|h| h.reference.clone()),
        );
    }
    // The socket is fabricated locally; binding the session is the whole
    // preparation, so a reservation is active as soon as it exists.
    if let Some(handle) = &selection.assignment {
        core.settle_assignment(handle, crate::session::AssignmentOutcome::Activated, now)
            .await?;
    }
    let version = credential.state.load();
    let attempt = Arc::new(AttemptContext {
        attempt_id: format!("{}-1", request.request_id),
        request: request.clone(),
        ordinal: 1,
        credential: credential.clone(),
        credential_version: version,
        agent_assignment: selection.assignment.as_ref().map(|h| h.reference.clone()),
    });
    funnel.trace(TraceEvent::AttemptStarted(&attempt));
    let capability = limits.capability(None);
    let channel_state: Arc<dyn gproxy_channel::channel::ChannelState> =
        Arc::new(crate::ChannelStateStore::new(
            ProtocolState::new(core, capability),
            &provider.entity.id,
            &credential.id,
        ));
    let upstream = AttemptUpstream::new(
        funnel.clone(),
        attempt,
        wire.headers.clone(),
        capability,
        channel_state.clone(),
        core.instance_id().clone(),
    );
    let Some(model) = request.target.upstream_model.clone() else {
        funnel.finish(UsageState::Failed).await;
        return Err(CoreError::Transform(TransformError::missing_metadata(
            "upstream_model",
        )));
    };
    let identity_target = IdentityTarget::new(&model, target)
        .and_then(|t| t.with_origin(&provider.entity.id))
        .map_err(|e| {
            CoreError::Transform(TransformError::shape("identity.target", e.to_string()))
        })?;
    let scope = StateScope {
        scope: request.scope.clone(),
        provider_id: provider.entity.id.clone(),
        conversation: request
            .session
            .as_ref()
            .filter(|s| s.is_stable())
            .map(|s| s.id.clone()),
    };
    let conversation_key = scope
        .conversation
        .clone()
        .unwrap_or_else(|| request.request_id.clone());
    let mut owned = OwnedState {
        store: ProtocolState::new(core, capability),
        scope,
        target: identity_target,
        conversation_key,
        // Millisecond precision: the state store persists expiry as ms and the
        // identity records written under it are compared against the read-back value.
        expires_at: SystemTime::UNIX_EPOCH
            + std::time::Duration::from_millis(now.max(0) as u64)
            + super::call::STATE_TTL,
    };
    if let Some(binding) = session.as_ref().and_then(|s| s.binding()) {
        owned.expires_at = SystemTime::UNIX_EPOCH
            + std::time::Duration::from_millis(binding.expires_at_ms.max(0) as u64);
    }
    let codec = limits.codec();
    let settings = stream_settings(codec, Dialect::OpenAi, None);
    let key = OperationKey {
        operation: Operation::StreamGenerateContent,
        dialect: target,
    };
    let endpoint = super::generate_endpoint(target, &model, true).map_err(CoreError::Transform)?;

    if let Some(session) = session {
        return Ok(bridge::serve(
            bridge::Config {
                owned,
                settings,
                headers: wire.headers.clone(),
                cancellation: request.cancellation.clone(),
            },
            session,
            funnel,
            completion,
        ));
    }

    let (to_client, from_task) = mpsc::channel::<Result<WsFrame, TransportError>>(FRAME_QUEUE);
    let (to_task, mut from_client) = mpsc::channel::<WsFrame>(FRAME_QUEUE);
    let task_funnel = funnel.clone();
    let cancellation = request.cancellation.clone();
    crate::rt::spawn(async move {
        let out = to_client;
        let mut alive = true;
        loop {
            let frame = tokio::select! {
                biased;
                () = cancellation.cancelled() => { alive = false; break; }
                frame = from_client.recv() => frame,
            };
            let Some(frame) = frame else { break };
            let text = match frame {
                WsFrame::Text(text) => text,
                WsFrame::Binary(_) => {
                    let frame = error_frame(400, None, "binary frames are not accepted".into());
                    if out.send(Ok(frame)).await.is_err() {
                        alive = false;
                        break;
                    }
                    continue;
                }
                WsFrame::Ping(payload) => {
                    if out.send(Ok(WsFrame::Pong(payload))).await.is_err() {
                        alive = false;
                        break;
                    }
                    continue;
                }
                WsFrame::Pong(_) => continue,
                WsFrame::Close(close) => {
                    let _ = out.send(Ok(WsFrame::Close(close))).await;
                    break;
                }
            };
            let message = match decode_message(text.as_bytes(), codec) {
                Ok(message) => message,
                Err(error) => {
                    let frame = error_frame(400, None, error.to_string());
                    if out.send(Ok(frame)).await.is_err() {
                        alive = false;
                        break;
                    }
                    continue;
                }
            };
            let lane = message.stream_id.clone();
            if message.generate == Some(false) {
                let frame = error_frame(
                    400,
                    lane,
                    "warmup (generate=false) has no equivalent on this upstream".into(),
                );
                if out.send(Ok(frame)).await.is_err() {
                    alive = false;
                    break;
                }
                continue;
            }
            let ClientEvent::ResponseCreate(input) = message.event else {
                let _ = out
                    .send(Ok(error_frame(
                        400,
                        lane,
                        "steering and injection have no equivalent cross-protocol generation operation".into(),
                    )))
                    .await;
                continue;
            };
            let access = owned.access();
            let created = now_ms().div_euclid(1000);
            let stream_target = StreamTarget {
                endpoint: endpoint.clone(),
                identities: match GenerationIdentity::new(
                    namespace(),
                    namespace(),
                    Dialect::OpenAi,
                    target,
                ) {
                    Ok(identities) => identities,
                    Err(error) => {
                        let _ = out
                            .send(Ok(error_frame(500, lane, error.to_string())))
                            .await;
                        continue;
                    }
                },
            };
            macro_rules! serve_turn {
                ($pair:ty, $ctx:expr) => {{
                    let context = $ctx;
                    let prepared =
                        <$pair>::prepare_stream(input, stream_target, context, settings, &access)
                            .await;
                    match prepared {
                        Ok(mut invocation) => {
                            match invocation.responses_websocket_output_on_lane(lane.clone()) {
                                Ok(()) => {
                                    turn(invocation, &upstream, &key, &access, lane.clone(), &out)
                                        .await
                                }
                                Err(error) => Err(error),
                            }
                        }
                        Err(error) => Err(error),
                    }
                }};
            }
            let result = match target {
                Dialect::OpenAi => {
                    responses_http::over_http(input, &upstream, &model, lane.clone(), codec, &out)
                        .await
                }
                Dialect::OpenAiChat => {
                    serve_turn!(ResponsesViaChat, responses_over_chat_context(&input))
                }
                Dialect::Claude => {
                    serve_turn!(
                        ResponsesViaClaude,
                        responses_over_claude_facts(&input, created)
                    )
                }
                _ => serve_turn!(
                    ResponsesViaGemini,
                    responses_over_gemini_facts(&input, created, &model)
                ),
            };
            match result {
                Ok(true) => {}
                Ok(false) => {
                    alive = false;
                    break;
                }
                Err(error) => {
                    let frame = error_frame(500, lane, error.to_string());
                    if out.send(Ok(frame)).await.is_err() {
                        alive = false;
                        break;
                    }
                }
            }
        }
        task_funnel
            .finish(if alive {
                UsageState::Completed
            } else {
                UsageState::Cancelled
            })
            .await;
    });

    let incoming = Box::pin(futures_util::stream::unfold(
        from_task,
        |mut rx| async move { rx.recv().await.map(|item| (item, rx)) },
    ));
    let outgoing = Box::pin(futures_util::sink::unfold(
        to_task,
        |tx, frame: WsFrame| async move { tx.send(frame).await.map(|()| tx).map_err(|_| closed()) },
    ));
    let handshake = WireResponse {
        status: StatusCode::SWITCHING_PROTOCOLS,
        headers: HeaderMap::new(),
        body: (),
    };
    let settled = funnel.arm();
    Ok(Execution::new(
        UpstreamConnection::Connected {
            handshake,
            socket: WebSocket { incoming, outgoing },
        },
        completion,
        settled,
    ))
}
