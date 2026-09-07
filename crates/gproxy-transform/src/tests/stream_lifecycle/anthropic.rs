use gproxy_protocol::{ContentGenerationKind as Kind, Operation};
use serde_json::json;

use super::super::content;
use super::super::support::{data_frames, drive};
use crate::ResponseStream;

#[test]
fn chat_stream_satisfies_anthropic_sdk_contract() {
    for timing in [
        "absent",
        "before_finish",
        "with_finish",
        "after_finish",
        "done_only",
    ] {
        let usage = json!({"prompt_tokens":12,"completion_tokens":7,"total_tokens":19,
            "prompt_tokens_details":{"cached_tokens":2,"cache_write_tokens":1}});
        let mut chunks = vec![
            (json!({"role":"assistant","content":""}), None, None),
            (
                json!({"reasoning_content":"think","content":"","refusal":""}),
                None,
                None,
            ),
            (
                json!({"reasoning_content":" more","content":""}),
                None,
                None,
            ),
            (
                json!({"reasoning_content":"","content":"answer","refusal":""}),
                None,
                None,
            ),
            (json!({"content":" ","reasoning_content":""}), None, None),
        ];
        if matches!(timing, "before_finish" | "done_only") {
            chunks.push((json!({}), None, Some(usage.clone())));
        }
        if timing != "done_only" {
            chunks.push((
                json!({}),
                Some("stop"),
                (timing == "with_finish").then(|| usage.clone()),
            ));
        }
        let mut wire = String::new();
        for (delta, finish, usage) in chunks {
            let chunk = json!({"id":"chat-test","object":"chat.completion.chunk","created":0,
                "model":"test-model","choices":[{"index":0,"delta":delta,"finish_reason":finish}],"usage":usage});
            wire.push_str(&format!("data: {chunk}\n\n"));
        }
        if timing == "after_finish" {
            let chunk = json!({"id":"chat-test","object":"chat.completion.chunk","created":0,
                "model":"test-model","choices":[],"usage":usage});
            wire.push_str(&format!("data: {chunk}\n\n"));
        }
        wire.push_str("data: [DONE]\n\n");
        let key = |kind| content(Operation::StreamGenerateContent, kind);
        let stream = ResponseStream::new(key(Kind::ClaudeMessages), key(Kind::OpenAiChat)).unwrap();
        let events = data_frames(&drive(stream, &wire, 17));
        for event in &events {
            serde_json::from_value::<gproxy_protocol::claude::StreamEvent>(event.clone()).unwrap();
        }
        assert_eq!(
            events[0]["message"]["usage"],
            json!({"input_tokens":0,"output_tokens":0})
        );
        let starts: Vec<_> = events
            .iter()
            .filter(|e| e["type"] == "content_block_start")
            .collect();
        assert_eq!(starts.len(), 2, "{timing}");
        assert_eq!(starts[0]["content_block"]["type"], "thinking");
        assert_eq!(starts[1]["content_block"]["type"], "text");
        assert_eq!(
            events
                .iter()
                .filter(|e| e["type"] == "content_block_stop")
                .count(),
            2
        );
        let deltas: Vec<_> = events
            .iter()
            .filter(|e| e["type"] == "message_delta")
            .collect();
        assert!(deltas.iter().all(|e| e["usage"]["output_tokens"].is_u64()));
        let expected_usage = if timing == "absent" {
            json!({"input_tokens":0,"output_tokens":0})
        } else {
            json!({"input_tokens":9,"output_tokens":7,"cache_read_input_tokens":2,"cache_creation_input_tokens":1})
        };
        assert_eq!(deltas.last().unwrap()["usage"], expected_usage, "{timing}");
        assert!(
            deltas
                .iter()
                .any(|e| e["delta"]["stop_reason"] == "end_turn")
        );
        assert_eq!(deltas.last().unwrap()["delta"]["stop_reason"], "end_turn");
        assert_eq!(events.last().unwrap()["type"], "message_stop");
        for (field, expected) in [("thinking", "think more"), ("text", "answer ")] {
            let actual: String = events
                .iter()
                .filter(|event| event["type"] == "content_block_delta")
                .filter_map(|event| event["delta"][field].as_str())
                .collect();
            assert_eq!(actual, expected, "{timing}: {field}");
        }
    }
}
