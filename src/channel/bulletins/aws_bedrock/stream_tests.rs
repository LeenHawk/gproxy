use super::*;
use crate::channel::aws_eventstream::{build_frame, build_frame_with_headers};

#[test]
fn converse_events_become_claude_sse() {
    let mut bytes = Vec::new();
    for (kind, payload) in [
        ("messageStart", r#"{"role":"assistant"}"#),
        (
            "contentBlockDelta",
            r#"{"contentBlockIndex":0,"delta":{"text":"OK"}}"#,
        ),
        ("contentBlockStop", r#"{"contentBlockIndex":0}"#),
        ("messageStop", r#"{"stopReason":"end_turn"}"#),
        (
            "metadata",
            r#"{"usage":{"inputTokens":9,"outputTokens":4,"totalTokens":13,"cacheReadInputTokens":2,"cacheWriteInputTokens":3,"cacheDetails":[{"inputTokens":1,"ttl":"5m"},{"inputTokens":2,"ttl":"1h"}]}}"#,
        ),
    ] {
        bytes.extend(build_frame(kind, payload.as_bytes()));
    }
    let mut decoder = ConverseStreamDecoder::new();
    let split = bytes.len() / 2;
    let mut output = decoder.push(&bytes[..split]);
    output.extend(decoder.push(&bytes[split..]));
    output.extend(decoder.finish());
    let frames = crate::pipeline::settle::frames::decode(&output);
    let usage = crate::usage::extract::from_stream_frames(
        crate::protocol::ContentGenerationKind::ClaudeMessages,
        &frames,
    )
    .unwrap();
    assert_eq!(usage.input, 9);
    assert_eq!(usage.output, 4);
    assert_eq!(usage.cache_read, 2);
    assert_eq!(usage.cache_creation_5m, 1);
    assert_eq!(usage.cache_creation_1h, 2);

    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("event: message_start"));
    assert!(output.contains("event: content_block_start"));
    assert!(output.contains(r#""type":"text_delta""#));
    assert!(output.contains(r#""text":"OK""#));
    assert!(output.contains(r#""cache_read_input_tokens":2"#));
    assert!(output.contains("event: message_stop"));
}

#[test]
fn missing_metadata_does_not_claim_zero_upstream_usage() {
    let mut decoder = ConverseStreamDecoder::new();
    let mut output = Vec::new();
    for (kind, payload) in [
        ("messageStart", r#"{"role":"assistant"}"#),
        (
            "contentBlockDelta",
            r#"{"contentBlockIndex":0,"delta":{"text":"partial"}}"#,
        ),
        ("messageStop", r#"{"stopReason":"end_turn"}"#),
    ] {
        output.extend(decoder.push(&build_frame(kind, payload.as_bytes())));
    }
    output.extend(decoder.finish());

    let frames = crate::pipeline::settle::frames::decode(&output);
    assert!(
        crate::usage::extract::from_stream_frames(
            crate::protocol::ContentGenerationKind::ClaudeMessages,
            &frames,
        )
        .is_none()
    );
}

#[test]
fn metadata_before_message_stop_is_deferred_and_preserved() {
    let mut decoder = ConverseStreamDecoder::new();
    let mut output = Vec::new();
    for (kind, payload) in [
        ("messageStart", r#"{"role":"assistant"}"#),
        (
            "metadata",
            r#"{"usage":{"inputTokens":7,"outputTokens":3,"totalTokens":10}}"#,
        ),
        ("messageStop", r#"{"stopReason":"max_tokens"}"#),
    ] {
        output.extend(decoder.push(&build_frame(kind, payload.as_bytes())));
    }
    output.extend(decoder.finish());

    let output_text = String::from_utf8(output.clone()).unwrap();
    assert!(output_text.contains(r#""stop_reason":"max_tokens""#));
    let frames = crate::pipeline::settle::frames::decode(&output);
    let usage = crate::usage::extract::from_stream_frames(
        crate::protocol::ContentGenerationKind::ClaudeMessages,
        &frames,
    )
    .unwrap();
    assert_eq!(usage.input, 7);
    assert_eq!(usage.output, 3);
}

#[test]
fn exception_frame_becomes_claude_error() {
    let frame = build_frame_with_headers(
        &[
            (":message-type", "exception"),
            (":exception-type", "throttlingException"),
        ],
        br#"{"message":"slow down"}"#,
    );
    let output = String::from_utf8(ConverseStreamDecoder::new().push(&frame)).unwrap();
    assert!(output.contains("event: error"));
    assert!(output.contains("throttlingException"));
    assert!(output.contains("slow down"));
}

#[test]
fn streamed_tool_input_is_emitted_complete() {
    let mut decoder = ConverseStreamDecoder::new();
    let mut output = Vec::new();
    for (kind, payload) in [
        ("messageStart", r#"{"role":"assistant"}"#),
        (
            "contentBlockStart",
            r#"{"contentBlockIndex":0,"start":{"toolUse":{"toolUseId":"tool_1","name":"get_weather"}}}"#,
        ),
        (
            "contentBlockDelta",
            r#"{"contentBlockIndex":0,"delta":{"toolUse":{"input":"{\"city\":"}}}"#,
        ),
        (
            "contentBlockDelta",
            r#"{"contentBlockIndex":0,"delta":{"toolUse":{"input":"\"Paris\"}"}}}"#,
        ),
        ("contentBlockStop", r#"{"contentBlockIndex":0}"#),
    ] {
        output.extend(decoder.push(&build_frame(kind, payload.as_bytes())));
    }
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains(r#""type":"tool_use""#));
    assert!(output.contains(r#""name":"get_weather""#));
    assert!(output.contains(r#""city":"Paris""#));
    assert!(!output.contains("input_json_delta"));
}
