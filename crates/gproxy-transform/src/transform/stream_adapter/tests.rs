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
    let mut transformer = SseTransformer::new(
        pair,
        TransformContext::new(upstream, inbound),
        ContentGenerationKind::ClaudeMessages,
    );
    let chunk = br#"data: {"id":"c1","object":"chat.completion.chunk","created":0,"model":"m","choices":[{"index":0,"delta":{"role":"assistant","content":"he"},"finish_reason":null}]}"#;
    let mut out = transformer.push(chunk);
    out.extend(transformer.push(b"\n\ndata: [DONE]\n\n"));
    out.extend(transformer.finish());
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
    let out = aggregate_buffered(ContentGenerationKind::OpenAiChatCompletions, sse.as_bytes());
    let value: Value = serde_json::from_slice(&out).unwrap();
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
    let mut transformer = SseTransformer::new(
        pair,
        TransformContext::new(upstream, inbound),
        ContentGenerationKind::OpenAiResponses,
    );
    let mut out = transformer.push(br#"data: {"id":"c1","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call_123","type":"function","function":{"name":"echo_text","arguments":""}}]},"finish_reason":null}]}"#);
    out.extend(transformer.push(br#"

data: {"id":"c1","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"text\":\"hello\"}"}}]},"finish_reason":null}]}"#));
    out.extend(transformer.push(br#"

data: {"id":"c1","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}"#));
    out.extend(transformer.push(b"\n\ndata: [DONE]\n\n"));
    out.extend(transformer.finish());
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("event: response.function_call_arguments.done"));
    assert!(text.contains("event: response.output_item.done"));
    assert!(text.contains(r#""arguments":"{\"text\":\"hello\"}""#));
    assert!(!text.contains(r#""item_id":"fc_0""#));
    assert!(text.contains(r#""output":[{"arguments":"{\"text\":\"hello\"}""#));
}
