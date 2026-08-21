use serde_json::Value;

use super::{field, frame_json, numeric};
use crate::transform::common::sse::SseFrame;
use crate::usage::NormalizedUsage;
use rust_decimal::Decimal;
use std::str::FromStr as _;

/// Extract token-billed transcription usage. Duration-billed Whisper usage is
/// intentionally not projected into tokens.
pub fn from_transcription_response(body: &Value) -> Option<NormalizedUsage> {
    let usage = body.get("usage").filter(|usage| usage.is_object())?;
    let usage_type = usage.get("type").and_then(Value::as_str);
    let tokens = usage_type != Some("duration")
        && numeric(usage, "input_tokens")
        && numeric(usage, "output_tokens");
    let seconds = usage.get("seconds").and_then(|value| {
        value
            .as_number()
            .and_then(|number| Decimal::from_str(&number.to_string()).ok())
    });
    if !tokens && seconds.is_none() {
        return None;
    }
    let mut normalized = NormalizedUsage {
        input: field(usage, "input_tokens"),
        output: field(usage, "output_tokens"),
        ..Default::default()
    };
    if let Some(seconds) = seconds {
        normalized.set_metric("audio_seconds", seconds);
    }
    Some(normalized)
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

        let duration = from_transcription_response(&json!({
            "usage": {"type": "duration", "seconds": 9}
        }))
        .expect("duration usage");
        assert_eq!(duration.metric("audio_seconds"), Decimal::from(9));
    }
}
