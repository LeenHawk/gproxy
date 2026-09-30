use super::*;
use crate::{
    codec,
    connection::WsFrame,
    transform::generate::stream::responses::ResponsesStreamCollector,
    wire::{
        DeclaredFields,
        openai::responses::websocket::{
            ClientEvent, ErrorMessage, RequestMessage, ServerEvent, ServerMessage,
        },
    },
};
use futures_core::Stream;
use std::{
    pin::Pin,
    task::{Context, Poll},
};

pub struct ResponsesWsTurn<'a> {
    session: &'a mut ResponsesWsSession,
    socket: Option<WebSocket>,
    send: SendState,
    collector: Option<ResponsesStreamCollector>,
    received_bytes: usize,
    received_frames: usize,
    sent_bytes: usize,
    sent_frames: usize,
    terminal: bool,
    lane: Option<String>,
    failed: bool,
}

impl Unpin for ResponsesWsTurn<'_> {}
enum SendState {
    Ready(WsFrame),
    Flush,
    Done,
}

impl<'a> ResponsesWsTurn<'a> {
    pub(super) fn new(
        session: &'a mut ResponsesWsSession,
        mut request: RequestMessage,
    ) -> Result<Self, TransformError> {
        let ClientEvent::ResponseCreate(body) = &mut request.event else {
            return Err(TransformError::unsupported(
                "responses.websocket.control",
                "steering and injection require a duplex continuation driver, not a single response turn",
            ));
        };
        if body.background.flatten() == Some(true) {
            return Err(TransformError::unsupported(
                "responses.websocket.background",
                "WebSocket mode has no background generation control",
            ));
        }
        body.stream = None;
        body.background = None;
        let event = request.into_declared();
        let lane = event.stream_id.clone();
        let bytes = codec::encode_json(
            &event,
            codec_limits(session.bounds.send_event.min(session.bounds.send_bytes)),
        )
        .map_err(|e| codec_error(e, true))?;
        let len = bytes.len();
        let frame =
            WsFrame::Text(String::from_utf8(bytes.to_vec()).expect("JSON serialization is UTF-8"));
        let collector = ResponsesStreamCollector::new(session.bounds.collector);
        let socket = session
            .socket
            .take()
            .ok_or_else(|| invalid("socket unavailable"))?;
        // Closed-before-poll: even mem::forget(turn) cannot reopen this session.
        session.busy = true;
        Ok(Self {
            session,
            socket: Some(socket),
            send: SendState::Ready(frame),
            collector: Some(collector),
            received_bytes: 0,
            received_frames: 0,
            sent_bytes: len,
            sent_frames: 1,
            terminal: false,
            lane,
            failed: false,
        })
    }
    pub fn received_frames(&self) -> usize {
        self.received_frames
    }
    pub fn received_bytes(&self) -> usize {
        self.received_bytes
    }
    /// Reserved sends, including an uncertain pending readiness/flush operation.
    pub fn reserved_send_frames(&self) -> usize {
        self.sent_frames
    }
    pub fn reserved_send_bytes(&self) -> usize {
        self.sent_bytes
    }
    pub fn failure_event(&self) -> Option<&StreamEvent> {
        self.session.failure_event()
    }
    pub fn websocket_failure(&self) -> Option<&ErrorMessage> {
        self.session.websocket_failure()
    }
    fn fail(&mut self, error: TransformError) -> TransformError {
        self.failed = true;
        self.socket.take();
        self.session.poison();
        error
    }
    fn account(&mut self, frame: &WsFrame) -> Result<(), TransformError> {
        self.received_frames = self
            .received_frames
            .checked_add(1)
            .ok_or_else(|| limit("responses.websocket.frames", "receive frame count overflow"))?;
        let bytes = match frame {
            WsFrame::Text(v) => v.len(),
            WsFrame::Binary(v) | WsFrame::Ping(v) | WsFrame::Pong(v) => v.len(),
            WsFrame::Close(Some(v)) => v
                .reason
                .len()
                .checked_add(2)
                .ok_or_else(|| limit("responses.websocket.frame", "close payload overflow"))?,
            WsFrame::Close(None) => 0,
        };
        if bytes > self.session.bounds.receive_frame {
            return Err(limit(
                "responses.websocket.frame",
                "receive frame byte cap exceeded",
            ));
        }
        self.received_bytes = self
            .received_bytes
            .checked_add(bytes)
            .filter(|v| *v <= self.session.bounds.receive_bytes)
            .ok_or_else(|| {
                limit(
                    "responses.websocket.bytes",
                    "aggregate receive byte cap exceeded",
                )
            })?;
        if matches!(
            frame,
            WsFrame::Ping(_) | WsFrame::Pong(_) | WsFrame::Close(_)
        ) && bytes > 125
        {
            return Err(invalid("invalid oversized WebSocket control frame"));
        }
        Ok(())
    }
    fn pong(&mut self, payload: bytes::Bytes) -> Result<(), TransformError> {
        if payload.len() > self.session.bounds.send_frame {
            return Err(limit(
                "responses.websocket.pong",
                "Pong exceeds host send frame cap",
            ));
        }
        let bytes = self
            .sent_bytes
            .checked_add(payload.len())
            .filter(|v| *v <= self.session.bounds.send_bytes)
            .ok_or_else(|| {
                limit(
                    "responses.websocket.pong",
                    "Pong exceeds aggregate send byte cap",
                )
            })?;
        let frames = self
            .sent_frames
            .checked_add(1)
            .ok_or_else(|| limit("responses.websocket.pong", "send frame count overflow"))?;
        self.sent_bytes = bytes;
        self.sent_frames = frames;
        self.send = SendState::Ready(WsFrame::Pong(payload));
        Ok(())
    }
    fn decode(&mut self, bytes: &[u8]) -> Result<StreamEvent, TransformError> {
        if bytes.len() > self.session.bounds.receive_event {
            return Err(limit(
                "responses.websocket.event",
                "JSON event cap exceeded",
            ));
        }
        let limits = codec_limits(self.session.bounds.receive_event);
        // Known nested WS errors must not fall back to the SSE Error variant
        // and discard a malformed error object as an unknown extension.
        let probe: serde_json::Value =
            codec::decode_json(bytes, limits).map_err(|e| codec_error(e, false))?;
        if probe.get("type").and_then(serde_json::Value::as_str) == Some("error")
            && probe.get("error").is_some()
        {
            let error = codec::decode_json::<ErrorMessage>(bytes, limits)
                .map_err(|e| codec_error(e, false))?
                .into_declared();
            let lane_matches = error.stream_id.is_none() || error.stream_id == self.lane;
            self.session.websocket_failure = Some(Box::new(error));
            return Err(invalid(if lane_matches {
                "native WebSocket error; see websocket_failure receipt"
            } else {
                "WebSocket error belongs to another lane"
            }));
        }
        let message = codec::decode_json::<ServerMessage>(bytes, limits)
            .map_err(|e| codec_error(e, false))?
            .into_declared();
        if message.stream_id != self.lane {
            return Err(invalid("WebSocket event belongs to another lane"));
        }
        let ServerEvent::Response(event) = message.event else {
            return Err(TransformError::unsupported(
                "responses.websocket.control",
                "steering and injection events require a duplex continuation driver",
            ));
        };
        if matches!(event, StreamEvent::Failed(_) | StreamEvent::Error(_)) {
            self.session.failure = Some(Box::new(event.clone()));
        }
        let terminal = matches!(
            event,
            StreamEvent::Completed(_) | StreamEvent::Incomplete(_)
        );
        self.collector
            .as_mut()
            .ok_or_else(|| invalid("collector already consumed"))?
            .push(event.clone())?;
        if terminal {
            let complete = self
                .collector
                .take()
                .ok_or_else(|| invalid("collector already consumed"))?
                .finish()?;
            let socket = self
                .socket
                .take()
                .ok_or_else(|| invalid("terminal socket unavailable"))?;
            self.session.last_response_id = Some(complete.value.id);
            self.session.last_status = complete.value.status;
            self.session.last_report = complete.report;
            self.session.socket = Some(socket);
            self.session.busy = false;
            self.terminal = true;
        }
        Ok(event)
    }
}

impl Stream for ResponsesWsTurn<'_> {
    type Item = Result<StreamEvent, TransformError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.terminal || this.failed {
            return Poll::Ready(None);
        }
        let Some(socket) = this.socket.as_mut() else {
            return Poll::Ready(Some(Err(this.fail(invalid("socket closed")))));
        };
        match poll_send(socket, &mut this.send, cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(e)) => return Poll::Ready(Some(Err(this.fail(e)))),
            Poll::Ready(Ok(())) => {}
        }
        if this.received_bytes >= this.session.bounds.receive_bytes {
            return Poll::Ready(Some(Err(this.fail(limit(
                "responses.websocket.bytes",
                "receive byte budget exhausted before terminal",
            )))));
        }
        let frame = match socket.incoming.as_mut().poll_next(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(None) => {
                return Poll::Ready(Some(Err(
                    this.fail(invalid("socket ended before terminal response"))
                )));
            }
            Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(this.fail(host_error(e))))),
            Poll::Ready(Some(Ok(frame))) => frame,
        };
        if let Err(e) = this.account(&frame) {
            return Poll::Ready(Some(Err(this.fail(e))));
        }
        match frame {
            WsFrame::Ping(payload) => {
                if let Err(e) = this.pong(payload) {
                    return Poll::Ready(Some(Err(this.fail(e))));
                }
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            WsFrame::Pong(_) => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            WsFrame::Close(close) => {
                this.session.peer_close = close;
                Poll::Ready(Some(Err(
                    this.fail(invalid("socket closed before terminal response"))
                )))
            }
            WsFrame::Binary(_) if !this.session.bounds.allow_binary => Poll::Ready(Some(Err(this
                .fail(TransformError::unsupported(
                    "responses.websocket.frame",
                    "binary JSON server messages are disabled",
                ))))),
            WsFrame::Text(text) => {
                Poll::Ready(Some(this.decode(text.as_bytes()).map_err(|e| this.fail(e))))
            }
            WsFrame::Binary(bytes) => {
                Poll::Ready(Some(this.decode(&bytes).map_err(|e| this.fail(e))))
            }
        }
    }
}

impl Drop for ResponsesWsTurn<'_> {
    fn drop(&mut self) {
        if !self.terminal {
            self.socket.take();
            self.session.poison();
        }
    }
}

fn poll_send(
    socket: &mut WebSocket,
    state: &mut SendState,
    cx: &mut Context<'_>,
) -> Poll<Result<(), TransformError>> {
    loop {
        match state {
            SendState::Ready(_) => match socket.outgoing.as_mut().poll_ready(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(host_error(e))),
                Poll::Ready(Ok(())) => {
                    // Move this exact pending frame once. Readiness/flush Pending
                    // never recreates response.create or replaces a queued Pong.
                    let SendState::Ready(frame) = std::mem::replace(state, SendState::Flush) else {
                        // The enclosing arm already matched SendState::Ready.
                        unreachable!()
                    };
                    if let Err(e) = socket.outgoing.as_mut().start_send(frame) {
                        return Poll::Ready(Err(host_error(e)));
                    }
                }
            },
            SendState::Flush => match socket.outgoing.as_mut().poll_flush(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(e)) => return Poll::Ready(Err(host_error(e))),
                Poll::Ready(Ok(())) => *state = SendState::Done,
            },
            SendState::Done => return Poll::Ready(Ok(())),
        }
    }
}
