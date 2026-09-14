//! Native Responses WebSocket turns over a single host-authenticated connection.
//!
//! Only `connect` constructs a session. Its destination/authentication remains
//! bound to the host connection; this module never reconnects, reroutes, retries
//! a generation, or substitutes a `previous_response_id`. The host enforces
//! wall-clock/idle deadlines and WebSocket wire framing. No native clock is read.
//!
//! A turn exclusively borrows its session. Dropping it before a validated
//! terminal closes the owned socket handles and poisons reuse, including when
//! cancellation happens before polling, during send readiness/flush, or receive.
//! The host transport owns the concrete cleanup caused by dropping its handles;
//! an asynchronous close handshake cannot be promised from `Drop`.
//!
//! All 53 declared event shapes decode, but validated turns follow the native
//! collector's policy: four orphaned audio/transcript shapes are Unsupported
//! because the canonical response DTO has no audio item. There is no implicit
//! audio consumer, transcript substitution, or fabricated terminal response.
mod connect;
mod turn;
use crate::{
    WebSocket,
    connection::WsClose,
    transform::{
        Report, TransformError, TransformErrorKind,
        generate::stream::responses::ResponsesStreamLimits,
    },
    wire::openai::responses::{
        generate::GenerateContentRequestBody, response::ResponseStatus, stream::StreamEvent,
    },
};
pub use connect::{ResponsesWsConnect, ResponsesWsConnectError, connect};
pub use turn::ResponsesWsTurn;

#[derive(Debug, Clone, Copy)]
pub struct ResponsesWsLimits {
    /// Payload cap for each complete data message or control frame.
    pub max_frame_bytes: usize,
    /// JSON request/response event cap, additionally bounded by each direction.
    pub max_event_bytes: usize,
    /// Per-turn totals include all data and control frame payloads.
    pub max_receive_bytes: usize,
    pub max_send_bytes: usize,
    pub max_receive_frames: usize,
    pub max_send_frames: usize,
    /// Native Responses uses text messages. Binary JSON is an explicit opt-in.
    pub allow_binary: bool,
    pub collector: ResponsesStreamLimits,
}
impl Default for ResponsesWsLimits {
    fn default() -> Self {
        Self {
            max_frame_bytes: 1024 * 1024,
            max_event_bytes: 1024 * 1024,
            max_receive_bytes: 16 * 1024 * 1024,
            max_send_bytes: 16 * 1024 * 1024,
            max_receive_frames: 100_000,
            max_send_frames: 100_000,
            allow_binary: false,
            collector: ResponsesStreamLimits::default(),
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub(super) struct Bounds {
    send_frame: usize,
    receive_frame: usize,
    send_event: usize,
    receive_event: usize,
    send_bytes: usize,
    receive_bytes: usize,
    send_frames: usize,
    receive_frames: usize,
    allow_binary: bool,
    collector: ResponsesStreamLimits,
}
#[derive(Debug)]
pub struct ResponsesWsSession {
    socket: Option<WebSocket>,
    bounds: Bounds,
    poisoned: bool,
    busy: bool,
    failure: Option<Box<StreamEvent>>,
    peer_close: Option<WsClose>,
    last_response_id: Option<String>,
    last_status: Option<ResponseStatus>,
    last_report: Report,
}
impl ResponsesWsSession {
    pub(super) fn connected(socket: WebSocket, bounds: Bounds) -> Self {
        Self {
            socket: Some(socket),
            bounds,
            poisoned: false,
            busy: false,
            failure: None,
            peer_close: None,
            last_response_id: None,
            last_status: None,
            last_report: Report::default(),
        }
    }
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    pub fn is_busy(&self) -> bool {
        self.busy
    }
    /// A received native Error/Failed event remains available after poisoning.
    pub fn failure_event(&self) -> Option<&StreamEvent> {
        self.failure.as_deref()
    }
    pub fn peer_close(&self) -> Option<&WsClose> {
        self.peer_close.as_ref()
    }
    /// This is the real native ID, never an inferred continuation or alias.
    pub fn last_response_id(&self) -> Option<&str> {
        self.last_response_id.as_deref()
    }
    pub fn last_status(&self) -> Option<ResponseStatus> {
        self.last_status
    }
    pub fn last_report(&self) -> &Report {
        &self.last_report
    }
    pub fn turn(
        &mut self,
        request: GenerateContentRequestBody,
    ) -> Result<ResponsesWsTurn<'_>, TransformError> {
        if self.poisoned || self.busy || self.socket.is_none() {
            return Err(TransformError::new(
                TransformErrorKind::MissingState,
                "responses.websocket",
                "connection is closed or has an uncertain turn",
            ));
        }
        ResponsesWsTurn::new(self, request)
    }
    fn poison(&mut self) {
        self.poisoned = true;
        self.busy = false;
        self.socket.take();
    }
}
fn limit(field: &'static str, detail: &'static str) -> TransformError {
    TransformError::new(TransformErrorKind::Limit, field, detail)
}
fn invalid(detail: &'static str) -> TransformError {
    TransformError::invalid_result("responses.websocket", detail)
}
fn codec_limits(max: usize) -> crate::codec::CodecLimits {
    crate::codec::CodecLimits {
        max_buffer_bytes: max as u64,
        max_value_bytes: max as u64,
        max_body_bytes: max as u64,
        max_line_bytes: max as u64,
        max_part_bytes: max as u64,
        max_parts: 1,
    }
}
fn codec_error(error: crate::codec::CodecError, request: bool) -> TransformError {
    let kind = if error.kind() == crate::codec::CodecErrorKind::Limit {
        TransformErrorKind::Limit
    } else if request {
        TransformErrorKind::InvalidInput
    } else {
        TransformErrorKind::InvalidResult
    };
    TransformError::with_source(
        kind,
        if request {
            "responses.websocket.request"
        } else {
            "responses.websocket.event"
        },
        error.to_string(),
        error,
    )
}
fn host_error(error: crate::connection::TransportError) -> TransformError {
    TransformError::with_source(
        TransformErrorKind::Host,
        "responses.websocket",
        error.to_string(),
        error,
    )
}
