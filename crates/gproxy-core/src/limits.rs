//! Explicit bounds for library callers. The gateway adds no payload, event or
//! time budget; only a caller-supplied deadline bounds its operation.

use gproxy_protocol::{capability::CapabilityLimits, codec::CodecLimits};
use std::time::Duration;

/// Optional host policy expressed in the lower-level capability/codec contracts.
/// Integer maxima mean no additional bound; `Duration::MAX` disables timers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionLimits {
    /// Whole-operation wall clock including body transfer.
    pub request_timeout: Duration,
    /// Maximum silence between stream progress events.
    pub stream_idle_timeout: Duration,
    pub max_request_body_bytes: u64,
    pub max_response_body_bytes: u64,
    /// One decoded SSE event, JSON array element, NDJSON record or JSON value.
    pub max_stream_event_bytes: u64,
    pub max_ws_frame_bytes: u64,
    pub max_multipart_parts: usize,
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            request_timeout: Duration::MAX,
            stream_idle_timeout: Duration::MAX,
            max_request_body_bytes: u64::MAX,
            max_response_body_bytes: u64::MAX,
            max_stream_event_bytes: u64::MAX,
            max_ws_frame_bytes: u64::MAX,
            max_multipart_parts: usize::MAX,
        }
    }
}

impl ExecutionLimits {
    /// Limits for one capability instance. `remaining` is the time left before
    /// the request's deadline, which can only shorten the operation budget.
    pub fn capability(&self, remaining: Option<Duration>) -> CapabilityLimits {
        CapabilityLimits {
            operation_total: remaining
                .map_or(self.request_timeout, |left| left.min(self.request_timeout)),
            stream_idle: self.stream_idle_timeout,
            read_bytes: self.max_response_body_bytes,
            write_bytes: self.max_request_body_bytes,
            ws_frame_bytes: self.max_ws_frame_bytes,
        }
    }

    /// Codec limits for decoding upstream responses and encoding requests.
    pub fn codec(&self) -> CodecLimits {
        CodecLimits {
            max_buffer_bytes: self.max_stream_event_bytes,
            max_value_bytes: self.max_stream_event_bytes,
            max_body_bytes: self.max_response_body_bytes,
            max_line_bytes: self.max_stream_event_bytes,
            max_part_bytes: self.max_request_body_bytes,
            max_parts: self.max_multipart_parts,
        }
    }
}
