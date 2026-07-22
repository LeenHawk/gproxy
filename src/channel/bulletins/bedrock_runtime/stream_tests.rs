use base64::Engine;

use super::*;
use crate::channel::aws_eventstream::{build_frame, build_frame_with_headers};

#[test]
fn fragmented_eventstream_yields_claude_sse_as_frames_arrive() {
    let event =
        br#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}"#;
    let payload = json!({
        "bytes": base64::engine::general_purpose::STANDARD.encode(event)
    });
    let frame = build_frame("chunk", payload.to_string().as_bytes());
    let mut decoder = BedrockRuntimeStreamDecoder::new();
    let split = frame.len() / 2;
    assert!(decoder.push(&frame[..split]).is_empty());
    let output = String::from_utf8(decoder.push(&frame[split..])).unwrap();
    assert!(output.starts_with("event: content_block_delta\n"));
    assert!(output.contains("\"text\":\"hi\""));
    assert!(decoder.finish().is_empty());
}

#[test]
fn openai_sse_is_passed_through_byte_for_byte() {
    let first = b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n";
    let second = b"data: [DONE]\n\n";
    let mut decoder = BedrockRuntimeStreamDecoder::new();
    assert_eq!(decoder.push(first), first);
    assert_eq!(decoder.push(second), second);
    assert!(decoder.finish().is_empty());
}

#[test]
fn exception_frame_becomes_claude_error_event() {
    let frame = build_frame_with_headers(
        &[
            (":message-type", "exception"),
            (":exception-type", "throttlingException"),
        ],
        br#"{"message":"slow down"}"#,
    );
    let mut decoder = BedrockRuntimeStreamDecoder::new();
    let output = String::from_utf8(decoder.push(&frame)).unwrap();
    assert!(output.starts_with("event: error\n"));
    assert!(output.contains("throttlingException"));
    assert!(output.contains("slow down"));
}
