use serde_json::Value;

use super::{field, frame_json, numeric};
use crate::transform::common::sse::SseFrame;
use crate::usage::NormalizedUsage;

/// Extract token-billed transcription usage. Duration-billed Whisper usage is
/// intentionally not projected into tokens.
pub fn from_transcription_response(body: &Value) -> Option<NormalizedUsage> {
    let usage = body.get("usage").filter(|usage| usage.is_object())?;
    let usage_type = usage.get("type").and_then(Value::as_str);
    (usage_type != Some("duration")
        && numeric(usage, "input_tokens")
        && numeric(usage, "output_tokens"))
    .then(|| NormalizedUsage {
        input: field(usage, "input_tokens"),
        output: field(usage, "output_tokens"),
        ..Default::default()
    })
}

pub fn from_transcription_stream_frames(frames: &[SseFrame]) -> Option<NormalizedUsage> {
    frames.iter().rev().find_map(|frame| {
        let body = frame_json(frame)?;
        (body.get("type").and_then(Value::as_str) == Some("transcript.text.done"))
            .then(|| from_transcription_response(&body))
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn handles_buffered_streamed_and_duration_shapes() {
        let buffered = json!({"usage": {
            "type": "tokens", "input_tokens": 14, "output_tokens": 45
        }});
        let usage = from_transcription_response(&buffered).unwrap();
        assert_eq!((usage.input, usage.output), (14, 45));

        let frames = [SseFrame::data(
            json!({
                "type": "transcript.text.done",
                "text": "hello",
                "usage": {"input_tokens": 7, "output_tokens": 3, "total_tokens": 10}
            })
            .to_string(),
        )];
        let usage = from_transcription_stream_frames(&frames).unwrap();
        assert_eq!((usage.input, usage.output), (7, 3));

        assert!(
            from_transcription_response(&json!({
                "usage": {"type": "duration", "seconds": 9}
            }))
            .is_none()
        );
    }
}
