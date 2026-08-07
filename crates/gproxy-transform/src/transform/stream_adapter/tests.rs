use serde_json::{Value, json};

use super::*;
use crate::protocol::{Operation, OperationKey};

#[test]
fn chat_chunks_to_claude_events() {
    let upstream = OperationKey::content_generation(
        Operation::GenerateContent,
        ContentGenerationKind::OpenAiChatCompletions,
    );
    let inbound = OperationKey::content_generation(
        Operation::GenerateContent,
        ContentGenerationKind::ClaudeMessages,
    );
    let pair = crate::transform::resolve(upstream, inbound).unwrap();
    let mut transformer =
        SseTransformer::new(pair, TransformContext::new(upstream, inbound)).unwrap();
    let chunk = br#"data: {"id":"c1","object":"chat.completion.chunk","created":0,"model":"m","choices":[{"index":0,"delta":{"role":"assistant","content":"he"},"finish_reason":null}]}"#;
    let mut out = transformer.push(chunk).unwrap();
    out.extend(transformer.push(b"\n\ndata: [DONE]\n\n").unwrap());
    out.extend(transformer.finish().unwrap());
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("event: "));
    assert!(!text.contains("[DONE]"));
    for line in text.lines().filter(|line| line.starts_with("data: ")) {
        let value: Value = serde_json::from_str(&line[6..]).unwrap();
        assert!(value.get("type").is_some());
    }
}

#[test]
fn aggregate_buffered_collapses_chat() {
    let sse = concat!(
        "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"he\"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"llo\"},\"finish_reason\":\"stop\"}]}\n\n",
        "data: [DONE]\n\n",
    );
    let out =
        aggregate_buffered(ContentGenerationKind::OpenAiChatCompletions, sse.as_bytes()).unwrap();
    let value: Value = serde_json::from_slice(&out.body).unwrap();
    assert_eq!(value["object"], "chat.completion");
    assert_eq!(value["choices"][0]["message"]["content"], "hello");
}

#[test]
fn complete_responses_object_emits_deltas_tools_and_completed() {
    let response = json!({"id":"resp_1","object":"response","status":"completed","model":"m","output":[
        {"id":"msg_1","type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":"hello","annotations":[]}]},
        {"id":"fc_1","type":"function_call","status":"completed","call_id":"call_1","name":"echo","arguments":"{\"text\":\"hi\"}"}
    ]});
    let out = synthesize_sse(
        ContentGenerationKind::OpenAiResponses,
        response.to_string().as_bytes(),
    )
    .unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("event: response.output_text.delta"));
    assert!(text.contains("event: response.function_call_arguments.done"));
    assert!(text.contains("event: response.completed"));
}

#[test]
fn complete_chat_response_becomes_one_chunk_and_done() {
    let response = json!({
        "id":"chat_1","object":"chat.completion","created":1,"model":"m",
        "choices":[{"index":0,"message":{"role":"assistant","content":"hello"},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}
    });
    let out = synthesize_sse(
        ContentGenerationKind::OpenAiChatCompletions,
        response.to_string().as_bytes(),
    )
    .unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("chat.completion.chunk"));
    assert!(text.contains(r#""content":"hello""#));
    assert!(text.ends_with("data: [DONE]\n\n"));
}

#[test]
fn complete_claude_response_preserves_text_and_tool_input() {
    let response = json!({
        "id":"msg_1","type":"message","role":"assistant","model":"m",
        "content":[
            {"type":"text","text":"hello"},
            {"type":"tool_use","id":"tool_1","name":"echo","input":{"text":"hi"}}
        ],
        "stop_reason":"tool_use","stop_sequence":null,
        "usage":{"input_tokens":1,"output_tokens":2}
    });
    let out = synthesize_sse(
        ContentGenerationKind::ClaudeMessages,
        response.to_string().as_bytes(),
    )
    .unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("event: message_start"));
    assert!(text.contains(r#""text":"hello""#));
    assert!(text.contains(r#""type":"text_delta""#));
    assert!(text.contains(r#""partial_json":"{\"text\":\"hi\"}""#));
    assert!(text.ends_with("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"));
}

#[test]
fn chat_tool_call_stream_finishes_responses_item() {
    let upstream = OperationKey::content_generation(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiChatCompletions,
    );
    let inbound = OperationKey::content_generation(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiResponses,
    );
    let pair = crate::transform::resolve(upstream, inbound).unwrap();
    let mut transformer =
        SseTransformer::new(pair, TransformContext::new(upstream, inbound)).unwrap();
    let mut out = transformer.push(br#"data: {"id":"c1","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call_123","type":"function","function":{"name":"echo_text","arguments":""}}]},"finish_reason":null}]}"#).unwrap();
    out.extend(transformer.push(br#"

data: {"id":"c1","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"text\":\"hello\"}"}}]},"finish_reason":null}]}"#).unwrap());
    out.extend(transformer.push(br#"

data: {"id":"c1","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#).unwrap());
    out.extend(transformer.push(b"\n\ndata: [DONE]\n\n").unwrap());
    out.extend(transformer.finish().unwrap());
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("event: response.function_call_arguments.done"));
    assert!(text.contains("event: response.output_item.done"));
    assert!(text.contains(r#""arguments":"{\"text\":\"hello\"}""#));
    assert!(!text.contains(r#""item_id":"fc_0""#));
    let completed = text
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter_map(|data| serde_json::from_str::<Value>(data).ok())
        .find(|value| value["type"] == "response.completed")
        .expect("response.completed frame");
    let item = &completed["response"]["output"][0];
    assert_eq!(item["type"], "function_call");
    assert_eq!(item["arguments"], "{\"text\":\"hello\"}");
}

#[test]
fn gemini_frame_preserves_all_parts_and_finish_reason() {
    let upstream = OperationKey::content_generation(
        Operation::StreamGenerateContent,
        ContentGenerationKind::GeminiGenerateContent,
    );
    let inbound = OperationKey::content_generation(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiResponses,
    );
    let pair = crate::transform::resolve(upstream, inbound).unwrap();
    let mut transformer =
        SseTransformer::new(pair, TransformContext::new(upstream, inbound)).unwrap();
    let frame = serde_json::json!({
        "responseId": "r1",
        "modelVersion": "gemini-test",
        "candidates": [{
            "index": 0,
            "content": {"parts": [
                {"text": "thinking", "thought": true},
                {"text": "answer"},
                {"functionCall": {"id": "call_1", "name": "echo", "args": {"x": 1}}}
            ]},
            "finishReason": "STOP"
        }]
    });
    let input = format!("data: {frame}\n\n");
    let mut out = transformer.push(input.as_bytes()).unwrap();
    out.extend(transformer.finish().unwrap());
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("response.reasoning_text.delta"));
    assert!(text.contains("response.output_text.delta"));
    assert!(text.contains("response.output_item.added"));
    assert!(text.contains("response.completed"));
    assert!(text.contains("thinking"));
    assert!(text.contains("answer"));
    assert!(text.contains("call_1"));
}

#[test]
fn chat_frame_preserves_content_reasoning_tool_and_finish() {
    let upstream = OperationKey::content_generation(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiChatCompletions,
    );
    let inbound = OperationKey::content_generation(
        Operation::StreamGenerateContent,
        ContentGenerationKind::ClaudeMessages,
    );
    let pair = crate::transform::resolve(upstream, inbound).unwrap();
    let mut transformer =
        SseTransformer::new(pair, TransformContext::new(upstream, inbound)).unwrap();
    let chunk = serde_json::json!({
        "id": "c1", "object": "chat.completion.chunk", "created": 1, "model": "m",
        "choices": [{
            "index": 0,
            "delta": {
                "content": "answer",
                "reasoning_content": "thinking",
                "tool_calls": [{
                    "index": 1, "id": "call_1", "type": "function",
                    "function": {"name": "echo", "arguments": "{\"x\":1}"}
                }]
            },
            "finish_reason": "tool_calls"
        }]
    });
    let input = format!("data: {chunk}\n\ndata: [DONE]\n\n");
    let mut out = transformer.push(input.as_bytes()).unwrap();
    out.extend(transformer.finish().unwrap());
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains(r#""text":"answer""#));
    assert!(text.contains(r#""thinking":"thinking""#));
    assert!(text.contains(r#""name":"echo""#));
    assert!(text.contains(r#""partial_json":"{\"x\":1}""#));
    assert!(text.contains(r#""stop_reason":"tool_use""#));
}

#[test]
fn strict_stream_rejects_bad_frame_and_does_not_finish() {
    let upstream = OperationKey::content_generation(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiChatCompletions,
    );
    let inbound = OperationKey::content_generation(
        Operation::StreamGenerateContent,
        ContentGenerationKind::GeminiGenerateContent,
    );
    let pair = crate::transform::resolve(upstream, inbound).unwrap();
    let mut transformer =
        SseTransformer::new(pair, TransformContext::new(upstream, inbound)).unwrap();
    assert!(transformer.push(b"data: {bad json}\n\n").is_err());
    assert!(transformer.finish().is_err());
}

#[test]
fn strict_stream_rejects_unexpected_eof() {
    let upstream = OperationKey::content_generation(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiChatCompletions,
    );
    let inbound = OperationKey::content_generation(
        Operation::StreamGenerateContent,
        ContentGenerationKind::GeminiGenerateContent,
    );
    let pair = crate::transform::resolve(upstream, inbound).unwrap();
    let mut transformer =
        SseTransformer::new(pair, TransformContext::new(upstream, inbound)).unwrap();
    let chunk =
        br#"data: {"id":"c1","object":"chat.completion.chunk","created":0,"model":"m","choices":[]}

"#;
    transformer.push(chunk).unwrap();
    assert!(matches!(
        transformer.finish(),
        Err(crate::transform::TransformError::UnexpectedEof { .. })
    ));
}

#[test]
fn buffered_aggregation_rejects_invalid_frames() {
    let input = b"data: {bad json}\n\ndata: [DONE]\n\n";
    assert!(aggregate_buffered(ContentGenerationKind::OpenAiChatCompletions, input).is_err());
}

#[test]
fn responses_normalizer_finish_flushes_lifecycle() {
    let mut normalizer = ResponsesStreamNormalizer::new();
    let input = concat!(
        "event: response.output_text.delta\n",
        "data: {\"type\":\"response.output_text.delta\",\"content_index\":0,\"delta\":\"hello\",\"item_id\":\"msg_1\",\"output_index\":0}\n\n"
    );
    let mut out = normalizer.push(input.as_bytes()).unwrap();
    out.extend(normalizer.finish().unwrap());
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("response.output_text.done"));
    assert!(text.contains("response.output_item.done"));
    assert!(text.contains("response.completed"));
}

#[test]
fn responses_normalizer_finish_flushes_tool_only_stream() {
    let mut normalizer = ResponsesStreamNormalizer::new();
    let input = concat!(
        "event: response.function_call_arguments.delta\n",
        "data: {\"type\":\"response.function_call_arguments.delta\",\"delta\":\"{\\\"x\\\":1}\",\"item_id\":\"fc_1\",\"output_index\":0}\n\n"
    );
    let mut out = normalizer.push(input.as_bytes()).unwrap();
    out.extend(normalizer.finish().unwrap());
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("response.function_call_arguments.done"));
    assert!(text.contains("response.output_item.done"));
    assert!(text.contains("response.completed"));
}

/// Regression: an upstream that sends real `output_item.done` items but an
/// empty `response.completed.output` must have that array filled with *those*
/// items. Re-synthesising them drops `encrypted_content` and invents
/// `status`, which the ChatGPT backend rejects with a fatal 400 when the
/// client replays `response.output` into the next turn.
#[test]
fn responses_normalizer_completed_output_reuses_upstream_items() {
    let mut normalizer = ResponsesStreamNormalizer::new();
    let input = concat!(
        "event: response.output_item.done\n",
        "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"reasoning\",\"id\":\"rs_1\",\"summary\":[],\"encrypted_content\":\"CIPHER\"}}\n\n",
        "event: response.completed\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"created_at\":0,\"object\":\"response\",\"output\":[],\"status\":\"completed\"}}\n\n"
    );
    let mut out = normalizer.push(input.as_bytes()).unwrap();
    out.extend(normalizer.finish().unwrap());
    let text = String::from_utf8(out).unwrap();
    let completed = text
        .split("data: ")
        .find(|frame| frame.contains("\"response.completed\""))
        .expect("completed frame");
    let event: serde_json::Value = serde_json::from_str(completed.trim()).unwrap();
    let item = &event["response"]["output"][0];
    assert_eq!(item["encrypted_content"], "CIPHER", "{item}");
    assert!(item.get("status").is_none(), "{item}");
}

#[test]
fn responses_normalizer_passthroughs_unparseable_frame_and_continues() {
    let mut normalizer = ResponsesStreamNormalizer::new();
    let input = concat!(
        "event: response.created\n",
        "data: {bad json}\n\n",
        "event: response.output_text.delta\n",
        "data: {\"type\":\"response.output_text.delta\",\"content_index\":0,\"delta\":\"hello\",\"item_id\":\"msg_1\",\"output_index\":0}\n\n"
    );

    let mut out = normalizer.push(input.as_bytes()).unwrap();
    assert!(out.starts_with(b"event: response.created\ndata: {bad json}\n\n"));
    out.extend(normalizer.finish().unwrap());

    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("response.output_text.delta"));
    assert!(text.contains("response.completed"));
}
