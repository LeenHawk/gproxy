//! Execution limits taken from the Setting row. Core has no unlimited mode:
//! every capability instance and codec receives finite values derived here.

use gproxy_protocol::{Operation, capability::CapabilityLimits, codec::CodecLimits};
use gproxy_store::entity::config::setting;
use std::time::Duration;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum LimitsError {
    #[error("setting `{0}` must be positive")]
    NotPositive(&'static str),
}

/// Finite bounds for one configuration snapshot. `Default` mirrors the Setting
/// column defaults so an uninitialized database behaves like a fresh row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionLimits {
    /// Until the upstream starts answering: its response head, or a converted
    /// non-streaming answer in full. After that `stream_idle_timeout` bounds
    /// the body; nothing bounds a whole stream, which may run for as long as
    /// it keeps producing.
    pub request_timeout: Duration,
    /// Maximum silence between stream progress events.
    pub stream_idle_timeout: Duration,
    pub max_request_body_bytes: u64,
    /// [`Operation::CreateFile`]'s request body, in place of
    /// `max_request_body_bytes`: see [`ExecutionLimits::for_operation`].
    pub max_upload_body_bytes: u64,
    pub max_response_body_bytes: u64,
    /// One decoded SSE event, JSON array element, NDJSON record or JSON value.
    pub max_stream_event_bytes: u64,
    pub max_ws_frame_bytes: u64,
    pub max_multipart_parts: usize,
}

impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_millis(1_200_000),
            stream_idle_timeout: Duration::from_millis(300_000),
            max_request_body_bytes: 50 * 1024 * 1024,
            max_upload_body_bytes: 512 * 1024 * 1024,
            max_response_body_bytes: 256 * 1024 * 1024,
            max_stream_event_bytes: 32 * 1024 * 1024,
            max_ws_frame_bytes: 32 * 1024 * 1024,
            max_multipart_parts: 64,
        }
    }
}

impl ExecutionLimits {
    pub fn from_settings(settings: &setting::Model) -> Result<Self, LimitsError> {
        fn positive_u64(value: i64, name: &'static str) -> Result<u64, LimitsError> {
            u64::try_from(value)
                .ok()
                .filter(|v| *v > 0)
                .ok_or(LimitsError::NotPositive(name))
        }
        fn positive_u32(value: u32, name: &'static str) -> Result<u32, LimitsError> {
            (value > 0)
                .then_some(value)
                .ok_or(LimitsError::NotPositive(name))
        }
        Ok(Self {
            request_timeout: Duration::from_millis(
                positive_u32(settings.request_timeout_ms, "request_timeout_ms")?.into(),
            ),
            stream_idle_timeout: Duration::from_millis(
                positive_u32(settings.stream_idle_timeout_ms, "stream_idle_timeout_ms")?.into(),
            ),
            max_request_body_bytes: positive_u64(
                settings.max_request_body_bytes,
                "max_request_body_bytes",
            )?,
            max_upload_body_bytes: positive_u64(
                settings.max_upload_body_bytes,
                "max_upload_body_bytes",
            )?,
            max_response_body_bytes: positive_u64(
                settings.max_response_body_bytes,
                "max_response_body_bytes",
            )?,
            max_stream_event_bytes: positive_u64(
                settings.max_stream_event_bytes,
                "max_stream_event_bytes",
            )?,
            max_ws_frame_bytes: positive_u64(settings.max_ws_frame_bytes, "max_ws_frame_bytes")?,
            max_multipart_parts: positive_u32(settings.max_multipart_parts, "max_multipart_parts")?
                as usize,
        })
    }

    /// These limits as `operation` sees them. A file upload is the one request
    /// whose body is meant to be large — provider file APIs take hundreds of
    /// megabytes — so it gets `max_upload_body_bytes` as its request cap.
    pub fn for_operation(self, operation: Operation) -> Self {
        match operation {
            Operation::CreateFile => Self {
                max_request_body_bytes: self.max_upload_body_bytes,
                ..self
            },
            _ => self,
        }
    }

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
