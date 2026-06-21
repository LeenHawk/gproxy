//! Per-family stream-frame helpers for §17 settlement: decoding buffered
//! relay bytes, harvesting the PRODUCED text (the stream-side counterpart of
//! [`crate::tokenize::harvest`]), and the protocol-shaped mid-stream error
//! frame emitted before an abnormal close (不裸断).

#[cfg(not(target_arch = "wasm32"))]
use bytes::Bytes;
use serde_json::Value;
#[cfg(not(target_arch = "wasm32"))]
use serde_json::json;

use crate::protocol::ContentGenerationKind;
use crate::transform::common::sse::{SseDecoder, SseFrame};

/// Decode buffered relayed bytes into SSE frames (tail drained).
pub fn decode(bytes: &[u8]) -> Vec<SseFrame> {
    let mut d = SseDecoder::new();
    let mut frames = d.push(bytes);
    if let Some(tail) = d.finish() {
        frames.push(tail);
    }
    frames
}

/// All produced text across buffered frames, in arrival order.
pub fn produced_text(kind: ContentGenerationKind, frames: &[SseFrame]) -> String {
    let mut out = String::new();
    for frame in frames {
        if let Some(t) = frame_text(kind, frame) {
            out.push_str(&t);
        }
    }
    out
}

/// Delta text of ONE chunk frame per the family's stream shape.
fn frame_text(kind: ContentGenerationKind, frame: &SseFrame) -> Option<String> {
    let v: Value = serde_json::from_str(&frame.data).ok()?;
    match kind {
        // content_block_delta carries `delta.text` / `delta.thinking`
        ContentGenerationKind::ClaudeMessages => {
            let delta = v.get("delta")?;
            delta
                .get("text")
                .or_else(|| delta.get("thinking"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        }
        ContentGenerationKind::OpenAiChatCompletions => {
            let mut out = String::new();
            for choice in v.get("choices")?.as_array()? {
                if let Some(s) = choice
                    .pointer("/delta/content")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                {
                    out.push_str(s);
                }
            }
            (!out.is_empty()).then_some(out)
        }
        // `delta` of response.output_text.delta events
        ContentGenerationKind::OpenAiResponses => {
            if v.get("type").and_then(Value::as_str) != Some("response.output_text.delta") {
                return None;
            }
            v.get("delta").and_then(Value::as_str).map(str::to_owned)
        }
        ContentGenerationKind::GeminiGenerateContent => {
            let mut out = String::new();
            for cand in v.get("candidates")?.as_array()? {
                let Some(parts) = cand.pointer("/content/parts").and_then(Value::as_array) else {
                    continue;
                };
                for part in parts {
                    if let Some(s) = part.get("text").and_then(Value::as_str) {
                        out.push_str(s);
                    }
                }
            }
            (!out.is_empty()).then_some(out)
        }
    }
}

/// ONE protocol-shaped error frame for a mid-stream upstream failure, in the
/// INBOUND family's wire shape, so the client sees a clean protocol-level end
/// instead of a bare transport break. Native-only (wasm never streams).
#[cfg(not(target_arch = "wasm32"))]
pub fn error_frame(kind: ContentGenerationKind) -> Bytes {
    const MESSAGE: &str = "upstream request failed";
    let encoded = match kind {
        ContentGenerationKind::ClaudeMessages => SseFrame::event(
            "error",
            json!({"type": "error", "error": {"type": "upstream_error", "message": MESSAGE}})
                .to_string(),
        )
        .encode(),
        ContentGenerationKind::OpenAiChatCompletions => {
            let err = SseFrame::data(
                json!({"error": {"type": "upstream_error", "message": MESSAGE}}).to_string(),
            )
            .encode();
            format!("{err}{}", SseFrame::data("[DONE]").encode())
        }
        ContentGenerationKind::OpenAiResponses => SseFrame::event(
            "response.failed",
            json!({
                "type": "response.failed",
                "response": {
                    "status": "failed",
                    "error": {"code": "upstream_error", "message": MESSAGE}
                }
            })
            .to_string(),
        )
        .encode(),
        ContentGenerationKind::GeminiGenerateContent => SseFrame::data(
            json!({"error": {"code": 502, "status": "UNAVAILABLE", "message": MESSAGE}})
                .to_string(),
        )
        .encode(),
    };
    Bytes::from(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produced_text_per_family() {
        let frames = decode(
            b"data: {\"choices\":[{\"delta\":{\"content\":\"he\"}}]}\n\n\
              data: {\"choices\":[{\"delta\":{\"content\":\"llo\"}}]}\n\ndata: [DONE]\n\n",
        );
        assert_eq!(
            produced_text(ContentGenerationKind::OpenAiChatCompletions, &frames),
            "hello"
        );

        let frames = decode(
            b"event: content_block_delta\n\
              data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
        );
        assert_eq!(
            produced_text(ContentGenerationKind::ClaudeMessages, &frames),
            "hi"
        );
    }

    #[test]
    fn error_frame_uses_generic_message() {
        let frame = error_frame(ContentGenerationKind::OpenAiChatCompletions);
        let text = String::from_utf8(frame.to_vec()).unwrap();
        assert!(text.contains("upstream request failed"));
        assert!(!text.contains("internal.example"));
    }
}
