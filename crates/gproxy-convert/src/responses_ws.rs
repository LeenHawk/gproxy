//! An HTTP streaming client (Chat, Claude, Gemini, Responses) whose upstream
//! speaks Responses over WebSocket: one connection, one `response.create`
//! turn, client-dialect bytes driven from a task that owns the socket.

mod responses_http;

use crate::{
    Call, Converted, Host,
    generate::{OwnedState, decode, namespace, stream_headers},
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
    connection::{ByteStream, TransportError},
    transform::TransformError,
    wire::openai::chat as h,
};
use http::{HeaderMap, StatusCode};
use tokio::sync::mpsc;

/// The upstream key for a Responses WebSocket generation.
pub const WS_KEY: OperationKey = OperationKey {
    operation: Operation::StreamGenerateContent,
    dialect: Dialect::OpenAiResponsesWebSocket,
};

/// Frames buffered between the conversion task and the caller.
pub const FRAME_QUEUE: usize = 16;

pub fn handshake_request() -> WireRequest<()> {
    WireRequest {
        method: http::Method::GET,
        path: "/v1/responses".into(),
        query: None,
        headers: HeaderMap::new(),
        body: (),
    }
}

/// HTTP streaming client over a Responses WebSocket upstream.
pub(crate) async fn over_websocket<H: Host>(
    call: &Call<'_, H>,
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
    let resources = &call.generation_resources();
    let endpoint = crate::generate_endpoint(Dialect::OpenAi, &state.target.model, true)?;
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
                let invocation = Box::new(Box::pin($prepare).await?);
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
                H::spawn(Box::pin(async move {
                    let mut invocation = invocation;
                    let mut session = session;
                    let access = owned.access();
                    let mut turn = match invocation.start_websocket(&mut session, &access).await {
                        Ok(turn) => turn,
                        Err(error) => {
                            let _ = tx.send(Err(crate::stream_error::converted(error, client, client_framing, 0))).await;
                            return;
                        }
                    };
                    let mut sequence = 0i64;
                    loop {
                        match turn.next(&access).await {
                            Ok(Some(chunk)) => {
                                sequence += i64::from(chunk.event.is_some());
                                if !chunk.bytes.is_empty()
                                    && tx.send(Ok(chunk.bytes)).await.is_err()
                                {
                                    return;
                                }
                            }
                            Ok(None) => return,
                            Err(error) => {
                                let _ = tx.send(Err(crate::stream_error::converted(error, client, client_framing, sequence))).await;
                                return;
                            }
                        }
                    }
                }));
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
