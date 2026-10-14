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
    generate::{OwnedState, decode, namespace, stream_headers, stream_settings, transport},
};
use crate::{
    Core, CoreError, CoreResult, ProtocolState, RequestContext, StateScope, UsageState,
    WebSocketExecution,
    api::{Execution, UsageCompletion, lifecycle::now_ms},
    execute::Funnel,
};
use gproxy_protocol::{
    Dialect, Operation, OperationKey, WireRequest, WireResponse,
    adapt::{
        generate::{
            GenerationIdentity,
            chat_responses::ChatViaResponses,
            claude_responses::ClaudeViaResponses,
            gemini_responses::GeminiViaResponses,
            stream::{
                StreamSettings, StreamTarget,
                websocket::{GenerationWsConnect, connect},
            },
        },
        responses_ws::ResponsesWsLimits,
    },
    capability::UpstreamConnection,
    connection::{ByteStream, TransportError, WebSocket, WsFrame},
    transform::{TransformError, identity::IdentityTarget},
    wire::openai::{
        chat as h,
        responses::websocket::{ClientEvent, ErrorDetail, ErrorMessage, ErrorMessageType},
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
    let (media, media_scope) = call.resources();
    let resources = &call.generation_resources(&media, &media_scope);
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
            run!(@drive <$pair>::prepare_stream_with_capabilities(
                input, stream_target, settings, state, resources
            ))
        }};
        ($pair:ty, $input:ty, $ctx:expr) => {{
            let input: $input = decode(body, limits)?;
            let context = $ctx;
            run!(@drive <$pair>::prepare_stream_with_capabilities(
                input, stream_target, context, settings, state, resources
            ))
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

/// Responses WebSocket client over an HTTP upstream speaking `target`.
/// The session owns turn admission; HTTP segments reuse its admitted executor.
pub(crate) async fn serve<C: BatchConnectionTrait + Send + Sync + 'static>(
    core: &Core<C>,
    request: Arc<RequestContext>,
    wire: &WireRequest<()>,
    funnel: Arc<Funnel>,
    completion: UsageCompletion,
    target: Dialect,
    session: Option<crate::ResponsesSession>,
) -> CoreResult<WebSocketExecution> {
    let session = session.ok_or_else(|| {
        CoreError::InvalidTarget(
            "Responses HTTP bridging requires connect_responses_session".into(),
        )
    })?;
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
    session.bind(
        request.clone(),
        credential.clone(),
        target,
        None,
        selection.assignment.as_ref().map(|h| h.reference.clone()),
    );
    // The socket is fabricated locally; binding the session is the whole
    // preparation, so a reservation is active as soon as it exists.
    if let Some(handle) = &selection.assignment {
        core.settle_assignment(handle, crate::session::AssignmentOutcome::Activated, now)
            .await?;
    }
    let capability = limits.capability(None);
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
    if let Some(binding) = session.binding() {
        owned.expires_at = SystemTime::UNIX_EPOCH
            + std::time::Duration::from_millis(binding.expires_at_ms.max(0) as u64);
    }
    let codec = limits.codec();
    let settings = stream_settings(codec, Dialect::OpenAi, None);
    Ok(bridge::serve(
        bridge::Config {
            owned,
            settings,
            headers: wire.headers.clone(),
            cancellation: request.cancellation.clone(),
        },
        session,
        funnel,
        completion,
    ))
}
