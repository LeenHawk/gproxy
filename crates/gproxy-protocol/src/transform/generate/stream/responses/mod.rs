//! Bounded native Responses SSE lifecycle, collection and synthesis.

mod collector;
mod emit;
mod events;
mod items;
mod lifecycle;
mod parts;
mod snapshots;
mod status;
mod synthesize;
use crate::transform::TransformError;
use crate::wire::openai::responses::{input as i, response as r, stream as s};
pub use collector::{ResponsesStreamCollector, ResponsesStreamLimits};
pub use synthesize::synthesize_responses_stream;

fn invalid(message: &'static str) -> TransformError {
    TransformError::invalid_result("responses.stream", message)
}

fn limit() -> TransformError {
    TransformError::new(
        crate::transform::TransformErrorKind::Limit,
        "responses.stream",
        "Responses stream limit exceeded",
    )
}

fn bounded<T: serde::Serialize>(value: &T, cap: usize) -> Result<usize, TransformError> {
    crate::codec::encode_json(
        value,
        crate::codec::CodecLimits {
            max_body_bytes: cap as u64,
            max_value_bytes: cap as u64,
            max_buffer_bytes: cap as u64,
            max_line_bytes: cap as u64,
            max_part_bytes: cap as u64,
            max_parts: 0,
        },
    )
    .map(|v| v.len())
    .map_err(|e| {
        if e.kind() == crate::codec::CodecErrorKind::Limit {
            limit()
        } else {
            invalid("cannot encode declared stream value")
        }
    })
}
