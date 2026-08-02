//! Downstream/upstream frame relay for an admitted Realtime session.

use std::time::Instant;

use axum::extract::ws::{Message, WebSocket};

use crate::http::client::{ConduitFrame, ConduitSocket};
use crate::pipeline::realtime::RealtimeSession;

pub(crate) fn is_path(path: &str) -> bool {
    matches!(path, "/v1/realtime" | "/v1/live") || scoped_path(path)
}

pub(crate) fn is_scoped_path(path: &str) -> bool {
    scoped_path(path)
}

fn scoped_path(path: &str) -> bool {
    let trimmed = path.trim_start_matches('/');
    let Some((provider, rest)) = trimmed.split_once('/') else {
        return false;
    };
    !provider.is_empty()
        && !matches!(provider, "v1" | "v1beta" | "console")
        && matches!(rest, "v1/realtime" | "v1/live")
}

pub(crate) async fn relay(mut downstream: WebSocket, mut session: RealtimeSession) {
    let started = Instant::now();
    let terminal = loop {
        tokio::select! {
            inbound = downstream.recv() => {
                match forward_downstream(inbound, session.socket.as_mut()).await {
                    Ok(Flow::Continue) => {}
                    Ok(Flow::Closed) => {
                        let _ = session.socket.close().await;
                        break "downstream_closed";
                    }
                    Err(error) => {
                        tracing::debug!(
                            request_id = %session.request_id,
                            error = %error,
                            "realtime downstream relay ended"
                        );
                        let _ = session.socket.close().await;
                        let _ = downstream.send(Message::Close(None)).await;
                        break "downstream_error";
                    }
                }
            }
            outbound = session.socket.recv_frame() => {
                match forward_upstream(outbound, &mut downstream).await {
                    Ok(Flow::Continue) => {}
                    Ok(Flow::Closed) => {
                        let _ = downstream.send(Message::Close(None)).await;
                        break "upstream_closed";
                    }
                    Err(error) => {
                        tracing::debug!(
                            request_id = %session.request_id,
                            error = %error,
                            "realtime upstream relay ended"
                        );
                        let _ = session.socket.close().await;
                        let _ = downstream.send(Message::Close(None)).await;
                        break "upstream_error";
                    }
                }
            }
        }
    };
    tracing::info!(
        request_id = %session.request_id,
        provider = %session.provider,
        channel = %session.channel,
        upstream_model = %session.model,
        duration_ms = started.elapsed().as_millis() as u64,
        ended = terminal,
        "realtime session ended"
    );
}

enum Flow {
    Continue,
    Closed,
}

async fn forward_downstream(
    message: Option<Result<Message, axum::Error>>,
    upstream: &mut dyn ConduitSocket,
) -> Result<Flow, String> {
    match message {
        Some(Ok(Message::Text(text))) => upstream
            .send_text(text.to_string())
            .await
            .map(|_| Flow::Continue)
            .map_err(|error| error.to_string()),
        Some(Ok(Message::Binary(bytes))) => upstream
            .send_binary(bytes)
            .await
            .map(|_| Flow::Continue)
            .map_err(|error| error.to_string()),
        Some(Ok(Message::Close(_))) | None => Ok(Flow::Closed),
        Some(Ok(Message::Ping(_) | Message::Pong(_))) => Ok(Flow::Continue),
        Some(Err(error)) => Err(error.to_string()),
    }
}

async fn forward_upstream(
    frame: Option<Result<ConduitFrame, crate::http::client::ClientError>>,
    downstream: &mut WebSocket,
) -> Result<Flow, String> {
    match frame {
        Some(Ok(ConduitFrame::Text(text))) => downstream
            .send(Message::Text(text.into()))
            .await
            .map(|_| Flow::Continue)
            .map_err(|error| error.to_string()),
        Some(Ok(ConduitFrame::Binary(bytes))) => downstream
            .send(Message::Binary(bytes))
            .await
            .map(|_| Flow::Continue)
            .map_err(|error| error.to_string()),
        Some(Ok(ConduitFrame::Close)) | None => Ok(Flow::Closed),
        Some(Err(error)) => Err(error.to_string()),
    }
}
