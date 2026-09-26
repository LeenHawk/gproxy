//! Incremental generation invocation over explicitly injected capabilities.

mod binding;
pub mod bridge;
pub mod event;
pub mod fanout;
mod history;
mod invoke;
pub(crate) mod ledger;
mod next;
mod output;
mod prepare;
pub mod reader;
mod resource_map;
pub mod synthesize;
pub mod websocket;

pub use history::ResponsesHistoryCache;
pub use invoke::{StreamChunk, StreamInvocation, StreamSettings, StreamStart, StreamTarget};
pub use prepare::{
    ChatViaGeminiStreamFacts, ClaudeViaGeminiStreamFacts, GeminiViaClaudeStreamFacts,
    ResponsesViaClaudeStreamFacts, ResponsesViaGeminiStreamFacts,
};

fn invalid(message: impl Into<String>) -> crate::transform::TransformError {
    crate::transform::TransformError::invalid_result("generation.stream", message)
}

fn conflict(message: impl Into<String>) -> crate::transform::TransformError {
    crate::transform::TransformError::new(
        crate::transform::TransformErrorKind::Conflict,
        "generation.stream",
        message,
    )
}

fn missing(message: impl Into<String>) -> crate::transform::TransformError {
    crate::transform::TransformError::new(
        crate::transform::TransformErrorKind::MissingState,
        "generation.stream",
        message,
    )
}

fn limit(message: impl Into<String>) -> crate::transform::TransformError {
    crate::transform::TransformError::new(
        crate::transform::TransformErrorKind::Limit,
        "generation.stream",
        message,
    )
}

fn codec_error(e: crate::codec::CodecError) -> crate::transform::TransformError {
    let kind = match e.kind() {
        crate::codec::CodecErrorKind::Limit => crate::transform::TransformErrorKind::Limit,
        crate::codec::CodecErrorKind::Transport => crate::transform::TransformErrorKind::Host,
        _ => crate::transform::TransformErrorKind::InvalidResult,
    };
    crate::transform::TransformError::with_source(kind, "generation.stream.codec", e.to_string(), e)
}
