//! A primary GPT-Live socket names its model in session.start. Authenticate
//! before upgrading, then admit one model-bound session before connecting upstream.
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use gproxy_protocol::{
    capability::UpstreamConnection,
    codec::read_http_body,
    connection::{TransportError, WebSocket, WsFrame},
};
use gproxy_seaorm::BatchConnectionTrait;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::{
    App, AppError, Caller, CaptureDirection, CaptureOutcome, CapturedFrame, DataPlaneRequest,
    DownstreamCapture,
};

impl<C: BatchConnectionTrait + Send + Sync + 'static> App<C> {
    pub fn open_live(
        self: &Arc<Self>,
        caller: Caller,
        request: DataPlaneRequest,
        prefix: Option<String>,
    ) -> Result<WebSocket, AppError> {
        crate::admission::permission::check_oauth_operation(
            &caller,
            request.operation.operation,
            &self.config().oauth.cli_client_ids,
        )?;
        let (input, receive) = mpsc::channel(16);
        let (output, send) = mpsc::channel(16);
        crate::rt::spawn(run(self.clone(), caller, request, prefix, receive, output));
        Ok(WebSocket {
            incoming: Box::pin(futures_util::stream::unfold(send, |mut rx| async move {
                rx.recv().await.map(|frame| (Ok(frame), rx))
            })),
            outgoing: Box::pin(futures_util::sink::unfold(input, |tx, frame| async move {
                tx.send(frame).await.map(|()| tx).map_err(|_| {
                    Box::new(std::io::Error::other("Live connection closed")) as TransportError
                })
            })),
        })
    }
}

async fn error(output: &mpsc::Sender<WsFrame>, message: String) {
    let _ = output
        .send(WsFrame::Text(
            json!({"type":"error", "error":{
                "type":"invalid_request_error", "message":message
            }})
            .to_string(),
        ))
        .await;
    let _ = output.send(WsFrame::Close(None)).await;
}

fn record(capture: &mut Option<DownstreamCapture>, direction: CaptureDirection, frame: &WsFrame) {
    if let Some(capture) = capture {
        let value = match frame {
            WsFrame::Text(text) => CapturedFrame::Text(text),
            WsFrame::Binary(bytes) => CapturedFrame::Binary(bytes),
            WsFrame::Ping(bytes) => CapturedFrame::Ping(bytes),
            WsFrame::Pong(bytes) => CapturedFrame::Pong(bytes),
            WsFrame::Close(value) => CapturedFrame::Close {
                code: value.as_ref().map(|v| v.code),
                reason: value
                    .as_ref()
                    .map(|v| v.reason.as_str())
                    .unwrap_or_default(),
            },
        };
        capture.record_frame(direction, value);
    }
}

async fn run<C: BatchConnectionTrait + Send + Sync + 'static>(
    app: Arc<App<C>>,
    caller: Caller,
    mut request: DataPlaneRequest,
    prefix: Option<String>,
    mut input: mpsc::Receiver<WsFrame>,
    output: mpsc::Sender<WsFrame>,
) {
    let cancellation = request.cancellation.clone().unwrap_or_default();
    let limits = app.gproxy().core().snapshot().limits;
    let first = tokio::select! {
        () = cancellation.cancelled() => return,
        first = crate::rt::timeout(limits.stream_idle_timeout, input.recv()) => {
            match first {
                Some(Some(WsFrame::Text(text))) => text,
                Some(Some(WsFrame::Close(_))) | Some(None) => return,
                _ => { error(&output, "first Live message must be session.start".into()).await; return }
            }
        }
    };
    if first.len() as u64 > limits.max_ws_frame_bytes {
        error(&output, "Live message exceeds the frame limit".into()).await;
        return;
    }
    let value: Value = match serde_json::from_str(&first) {
        Ok(value) => value,
        Err(_) => {
            error(&output, "invalid Live message JSON".into()).await;
            return;
        }
    };
    let model = value
        .pointer("/session/model")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty());
    let Some(model) = model.filter(|_| value["type"] == "session.start") else {
        error(&output, "session.start requires session.model".into()).await;
        return;
    };
    request.model = Some(match prefix {
        Some(prefix)
            if request.provider_id.is_some() || !model.starts_with(&format!("{prefix}/")) =>
        {
            format!("{prefix}/{model}")
        }
        _ => model.to_owned(),
    });
    request.body = first.as_bytes().to_vec().into();
    let outcome = match app.connect(&caller, request).await {
        Ok(outcome) => outcome,
        Err(reason) => {
            error(&output, reason.to_string()).await;
            return;
        }
    };
    let crate::ConnectOutcome {
        execution,
        mut admitted,
        mut capture,
    } = outcome;
    let (connection, usage) = execution.into_parts();
    let mut outcome = CaptureOutcome::Complete;
    match connection {
        UpstreamConnection::Rejected(response) => {
            let status = response.status;
            if let Some(capture) = &mut capture {
                capture.record_response_head(status, &response.headers);
            }
            let bytes = read_http_body(response.body, limits.codec()).await.ok();
            if let (Some(capture), Some(bytes)) = (&mut capture, &bytes) {
                capture.record_response_chunk(bytes);
            }
            let message = bytes
                .and_then(|bytes| String::from_utf8(bytes.to_vec()).ok())
                .unwrap_or_else(|| format!("Live handshake rejected: {status}"));
            error(&output, message.clone()).await;
            outcome = CaptureOutcome::Failed { error: message };
        }
        UpstreamConnection::Connected {
            handshake,
            mut socket,
        } => {
            if let Some(capture) = &mut capture {
                capture.record_response_head(
                    http::StatusCode::SWITCHING_PROTOCOLS,
                    &handshake.headers,
                );
            }
            let first = WsFrame::Text(first);
            record(&mut capture, CaptureDirection::Request, &first);
            if let Err(reason) = socket.outgoing.send(first).await {
                outcome = CaptureOutcome::Failed {
                    error: reason.to_string(),
                };
            } else {
                loop {
                    tokio::select! {
                        () = cancellation.cancelled() => { outcome = CaptureOutcome::Cancelled; break }
                        frame = input.recv() => {
                            let Some(frame) = frame else { cancellation.cancel(); outcome = CaptureOutcome::Cancelled; break };
                            record(&mut capture, CaptureDirection::Request, &frame);
                            let closing = matches!(frame, WsFrame::Close(_));
                            if let Err(reason) = socket.outgoing.send(frame).await {
                                outcome = CaptureOutcome::Failed { error: reason.to_string() }; break;
                            }
                            if closing { break }
                        }
                        frame = socket.incoming.next() => {
                            match frame {
                                Some(Ok(WsFrame::Ping(bytes))) => {
                                    if socket.outgoing.send(WsFrame::Pong(bytes)).await.is_err() { break }
                                }
                                Some(Ok(frame)) => {
                                    record(&mut capture, CaptureDirection::Response, &frame);
                                    let closing = matches!(frame, WsFrame::Close(_));
                                    if output.send(frame).await.is_err() { cancellation.cancel(); outcome = CaptureOutcome::Cancelled; break }
                                    if closing { break }
                                }
                                Some(Err(reason)) => { error(&output, reason.to_string()).await; outcome = CaptureOutcome::Failed { error: reason.to_string() }; break }
                                None => break,
                            }
                        }
                    }
                }
            }
            let _ = crate::rt::timeout(std::time::Duration::from_secs(5), socket.outgoing.close())
                .await;
            drop(socket);
        }
    }
    if let Some(capture) = capture {
        let _ = capture.settle(app.gproxy().store(), outcome, usage).await;
    } else {
        let _ = usage.await;
    }
    admitted.release().await;
}
