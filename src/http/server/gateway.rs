//! Gateway handlers: read the inbound request, run the pipeline, relay the
//! upstream response. Aggregated (`/v1/...`) and scoped (`/{provider}/v1/...`).

use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{FromRequestParts, OptionalFromRequestParts, Request, State};
use axum::http::StatusCode;
use axum::http::header::{CONNECTION, UPGRADE};
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt as _;
use http::request::Parts;

use crate::app::AppState;
use crate::config::MAX_BODY_BYTES;
use crate::http::responses_ws::{ResponsesWsRequestBase, WsFrameError};
use crate::http::server::extract::build_ctx;
use crate::pipeline;
use crate::pipeline::outcome::{ExecOutcome, ResponseBody};
use crate::transform::generate_content::openai_responses_websocket::ResponseWebSocketSseDecoder;

/// `/v1/{*rest}` — model name resolves to a route.
pub async fn aggregated(
    State(state): State<AppState>,
    ws: Option<OptionalWsUpgrade>,
    req: Request,
) -> Response {
    handle(state, ws, req, false).await
}

/// `/{provider}/v1/{*rest}` — bypass routing, hit the named provider directly.
pub async fn scoped(
    State(state): State<AppState>,
    ws: Option<OptionalWsUpgrade>,
    req: Request,
) -> Response {
    handle(state, ws, req, true).await
}

async fn handle(
    state: AppState,
    ws: Option<OptionalWsUpgrade>,
    req: Request,
    scoped: bool,
) -> Response {
    if let Some(OptionalWsUpgrade(ws)) = ws {
        return handle_websocket(state, ws, req, scoped);
    }

    let (parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, MAX_BODY_BYTES).await {
        Ok(b) => b,
        Err(_) => {
            return (StatusCode::PAYLOAD_TOO_LARGE, "request body too large").into_response();
        }
    };
    let ctx = match build_ctx(parts, bytes, scoped) {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let request_id = ctx.request_id.clone();
    match pipeline::execute(&state, ctx).await {
        Ok(outcome) => egress(outcome, &request_id),
        Err(e) => e.into_response(),
    }
}

fn handle_websocket(state: AppState, ws: WebSocketUpgrade, req: Request, scoped: bool) -> Response {
    let path = req.uri().path();
    if !crate::http::responses_ws::is_responses_websocket_path(path)
        || scoped != crate::http::responses_ws::is_scoped_responses_websocket_path(path)
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let (parts, _body) = req.into_parts();
    let base = ResponsesWsRequestBase::from_parts(&parts);
    ws.on_upgrade(move |socket| serve_websocket(socket, state, base, scoped))
}

async fn serve_websocket(
    mut socket: WebSocket,
    state: AppState,
    base: ResponsesWsRequestBase,
    scoped: bool,
) {
    while let Some(message) = socket.recv().await {
        let frame = match message {
            Ok(Message::Text(text)) => text.to_string(),
            Ok(Message::Binary(bytes)) => match String::from_utf8(bytes.to_vec()) {
                Ok(text) => text,
                Err(_) => {
                    if send_frame(
                        &mut socket,
                        WsFrameError::plain(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            "binary websocket frame is not UTF-8 JSON",
                        )
                        .to_frame(),
                    )
                    .await
                    .is_err()
                    {
                        return;
                    }
                    continue;
                }
            },
            Ok(Message::Close(_)) => return,
            Ok(Message::Ping(_) | Message::Pong(_)) => continue,
            Err(_) => return,
        };

        if relay_frame_to_websocket(&mut socket, &state, &base, scoped, &frame)
            .await
            .is_err()
        {
            return;
        }
    }
}

async fn relay_frame_to_websocket(
    socket: &mut WebSocket,
    state: &AppState,
    base: &ResponsesWsRequestBase,
    scoped: bool,
    frame: &str,
) -> Result<(), axum::Error> {
    let outcome = match crate::http::responses_ws::execute_frame(state, base, scoped, frame).await {
        Ok(outcome) => outcome,
        Err(error) => {
            return send_frame(socket, error.to_frame()).await;
        }
    };

    if !outcome.status.is_success() {
        return send_collected_outcome(socket, outcome).await;
    }

    match outcome.body {
        ResponseBody::Full(_) => send_collected_outcome(socket, outcome).await,
        ResponseBody::Stream(stream) => stream_outcome_to_websocket(socket, stream).await,
    }
}

async fn send_collected_outcome(
    socket: &mut WebSocket,
    outcome: ExecOutcome,
) -> Result<(), axum::Error> {
    let messages = match crate::http::responses_ws::outcome_to_messages(outcome).await {
        Ok(messages) => messages,
        Err(error) => vec![error.to_frame()],
    };
    for message in messages {
        send_frame(socket, message).await?;
    }
    Ok(())
}

async fn stream_outcome_to_websocket(
    socket: &mut WebSocket,
    mut stream: crate::pipeline::outcome::ByteStream,
) -> Result<(), axum::Error> {
    let mut decoder = ResponseWebSocketSseDecoder::new();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(error) => {
                return send_frame(
                    socket,
                    WsFrameError::plain(StatusCode::BAD_GATEWAY, &error.to_string()).to_frame(),
                )
                .await;
            }
        };
        for message in decoder.push(&chunk) {
            send_frame(socket, message).await?;
        }
    }
    for message in decoder.finish() {
        send_frame(socket, message).await?;
    }
    Ok(())
}

async fn send_frame(socket: &mut WebSocket, message: String) -> Result<(), axum::Error> {
    socket.send(Message::Text(message.into())).await
}

/// Map an [`ExecOutcome`] to the client response: status + hop-by-hop-sanitized
/// headers + the buffered or (native) streamed body, plus the request id for
/// correlation.
fn egress(outcome: ExecOutcome, request_id: &str) -> Response {
    let metadata = crate::http::egress::metadata(&outcome, request_id);
    let body = match outcome.body {
        ResponseBody::Full(b) => Body::from(b),
        #[cfg(not(target_arch = "wasm32"))]
        ResponseBody::Stream(s) => Body::from_stream(s),
    };
    let mut response = Response::new(body);
    *response.status_mut() = metadata.status;
    *response.headers_mut() = metadata.headers;
    response
}

pub struct OptionalWsUpgrade(WebSocketUpgrade);

impl<S> OptionalFromRequestParts<S> for OptionalWsUpgrade
where
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        if !looks_like_websocket(parts) {
            return Ok(None);
        }
        WebSocketUpgrade::from_request_parts(parts, state)
            .await
            .map(|ws| Some(Self(ws)))
            .map_err(IntoResponse::into_response)
    }
}

fn looks_like_websocket(parts: &Parts) -> bool {
    parts
        .headers
        .get(UPGRADE)
        .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"websocket"))
        && parts
            .headers
            .get(CONNECTION)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.to_ascii_lowercase().contains("upgrade"))
}
