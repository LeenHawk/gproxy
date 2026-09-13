//! Bounded native Chat collection and response synthesis. `[DONE]` is framing.
mod choice;
mod collector;
pub(crate) mod logs;
mod synthesize;
pub(crate) mod usage;
pub use collector::{ChatStreamCollector, ChatStreamContext, ChatStreamLimits};
pub use synthesize::synthesize_chat_stream;
fn limit(field: &str) -> crate::transform::TransformError {
    crate::transform::TransformError::new(
        crate::transform::TransformErrorKind::Limit,
        format!("stream.{field}"),
        "Chat stream limit exceeded",
    )
}
fn encoded<T: serde::Serialize>(
    value: &T,
    remaining: usize,
) -> Result<usize, crate::transform::TransformError> {
    let n = remaining as u64;
    crate::codec::encode_json(
        value,
        crate::codec::CodecLimits {
            max_value_bytes: n,
            max_body_bytes: n,
            max_buffer_bytes: n,
            max_line_bytes: n,
            max_part_bytes: n,
            max_parts: usize::MAX,
        },
    )
    .map(|v| v.len())
    .map_err(|e| {
        if e.kind() == crate::codec::CodecErrorKind::Limit {
            limit("bytes")
        } else {
            crate::transform::TransformError::shape("stream", e.to_string())
        }
    })
}
