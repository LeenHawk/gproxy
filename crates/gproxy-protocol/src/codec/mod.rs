//! Bounded, runtime-neutral wire codecs.
//!
//! These codecs only understand transport framing and JSON syntax. They do not
//! decode vendor objects, perform conversion, or preserve unknown application
//! fields as a common representation.

mod body;
mod json;
mod multipart;
mod sse;

use std::{error::Error, fmt, future::Future, pin::Pin};

pub use body::read_http_body;
pub use json::{
    JsonArrayDecoder, JsonArrayEncoder, JsonDecoder, NdjsonDecoder, NdjsonEncoder, decode_json,
    encode_json,
};
pub use multipart::{MultipartDecoder, MultipartEncoder};
pub use sse::{SseDecoder, SseEncoder, SseEvent, SseFrame, encode_sse_done, encode_sse_event};

/// A boxed codec operation future. Native futures are `Send`; wasm follows the
/// connection stream contract and does not require `Send`.
#[cfg(not(target_arch = "wasm32"))]
pub type CodecFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[cfg(target_arch = "wasm32")]
pub type CodecFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

/// Explicit bounds required by every decoder and encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodecLimits {
    /// Maximum incomplete input retained by a decoder.
    pub max_buffer_bytes: u64,
    /// Maximum encoded JSON value or SSE data event.
    pub max_value_bytes: u64,
    /// Maximum aggregate HTTP body bytes consumed or produced.
    pub max_body_bytes: u64,
    /// Maximum one line in NDJSON/SSE or one multipart header line.
    pub max_line_bytes: u64,
    /// Maximum one multipart part body.
    pub max_part_bytes: u64,
    /// Maximum multipart parts.
    pub max_parts: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum CodecErrorKind {
    Invalid,
    UnexpectedEof,
    Utf8,
    Json,
    Multipart,
    Limit,
    Transport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum CodecErrorStage {
    Start,
    Body,
    Stream,
    Finish,
}

/// A codec failure with an optional lower-level source.
#[derive(Debug)]
pub struct CodecError {
    kind: CodecErrorKind,
    stage: CodecErrorStage,
    message: String,
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl CodecError {
    pub fn new(kind: CodecErrorKind, stage: CodecErrorStage, message: impl Into<String>) -> Self {
        Self {
            kind,
            stage,
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(
        kind: CodecErrorKind,
        stage: CodecErrorStage,
        message: impl Into<String>,
        source: impl Into<Box<dyn Error + Send + Sync + 'static>>,
    ) -> Self {
        Self {
            kind,
            stage,
            message: message.into(),
            source: Some(source.into()),
        }
    }

    pub fn kind(&self) -> CodecErrorKind {
        self.kind
    }

    pub fn stage(&self) -> CodecErrorStage {
        self.stage
    }

    pub fn source_error(&self) -> Option<&(dyn Error + Send + Sync + 'static)> {
        self.source.as_deref()
    }
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({:?}/{:?})", self.message, self.kind, self.stage)
    }
}

impl Error for CodecError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}
