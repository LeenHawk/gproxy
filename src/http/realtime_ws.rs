//! Downstream/upstream frame relay for an admitted Realtime session.

use std::time::Instant;

use axum::extract::ws::{Message, WebSocket};

use crate::http::client::{ConduitFrame, ConduitSocket};
use crate::pipeline::realtime::RealtimeSession;
use crate::usage::Ended;

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
                        break ("downstream_closed", Ended::Complete);
                    }
                    Ok(Flow::Interrupted) => {
                        let _ = session.socket.close().await;
                        break ("downstream_eof", Ended::Interrupted);
                    }
                    Err(error) => {
                        tracing::debug!(
                            request_id = %session.request_id,
                            error = %error,
                            "realtime downstream relay ended"
                        );
                        let _ = session.socket.close().await;
                        let _ = downstream.send(Message::Close(None)).await;
                        break ("downstream_error", Ended::Interrupted);
                    }
                }
            }
            outbound = session.socket.recv_frame() => {
                match forward_upstream(outbound, &mut downstream, Some(&mut session)).await {
                    Ok(Flow::Continue) => {}
                    Ok(Flow::Closed) => {
                        let _ = downstream.send(Message::Close(None)).await;
                        break ("upstream_closed", Ended::Complete);
                    }
                    Ok(Flow::Interrupted) => {
                        let _ = downstream.send(Message::Close(None)).await;
                        break ("upstream_eof", Ended::Interrupted);
                    }
                    Err(error) => {
                        tracing::debug!(
                            request_id = %session.request_id,
                            error = %error,
                            "realtime upstream relay ended"
                        );
                        let _ = session.socket.close().await;
                        let _ = downstream.send(Message::Close(None)).await;
                        break ("upstream_error", Ended::Interrupted);
                    }
                }
            }
        }
    };
    let duration_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    tracing::info!(
        request_id = %session.request_id,
        provider = %session.provider,
        channel = %session.channel,
        upstream_model = %session.model,
        duration_ms,
        ended = terminal.0,
        "realtime session ended"
    );
    session.record_usage(duration_ms, terminal.1).await;
}

pub(crate) async fn relay_raw(mut downstream: WebSocket, mut upstream: Box<dyn ConduitSocket>) {
    loop {
        tokio::select! {
            inbound = downstream.recv() => match forward_downstream(inbound, upstream.as_mut()).await {
                Ok(Flow::Continue) => {}
                _ => { let _ = upstream.close().await; return; }
            },
            outbound = upstream.recv_frame() => match forward_upstream(outbound, &mut downstream, None).await {
                Ok(Flow::Continue) => {}
                _ => { let _ = downstream.send(Message::Close(None)).await; return; }
            },
        }
    }
}

enum Flow {
    Continue,
    Closed,
    Interrupted,
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
        Some(Ok(Message::Close(_))) => Ok(Flow::Closed),
        None => Ok(Flow::Interrupted),
        Some(Ok(Message::Ping(_) | Message::Pong(_))) => Ok(Flow::Continue),
        Some(Err(error)) => Err(error.to_string()),
    }
}

async fn forward_upstream(
    frame: Option<Result<ConduitFrame, crate::http::client::ClientError>>,
    downstream: &mut WebSocket,
    session: Option<&mut RealtimeSession>,
) -> Result<Flow, String> {
    match frame {
        Some(Ok(ConduitFrame::Text(text))) => {
            let text = session.map_or(text.clone(), |session| session.decorate_usage(&text));
            downstream
                .send(Message::Text(text.into()))
                .await
                .map(|_| Flow::Continue)
                .map_err(|error| error.to_string())
        }
        Some(Ok(ConduitFrame::Binary(bytes))) => downstream
            .send(Message::Binary(bytes))
            .await
            .map(|_| Flow::Continue)
            .map_err(|error| error.to_string()),
        Some(Ok(ConduitFrame::Close)) => Ok(Flow::Closed),
        None => Ok(Flow::Interrupted),
        Some(Err(error)) => Err(error.to_string()),
    }
}
