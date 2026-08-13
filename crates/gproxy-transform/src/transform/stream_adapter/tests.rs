use serde_json::{Value, json};

use super::*;
use crate::protocol::{Operation, OperationKey};

fn transform_sse(
    upstream_kind: ContentGenerationKind,
    inbound_kind: ContentGenerationKind,
    input: &str,
) -> String {
    let upstream =
        OperationKey::content_generation(Operation::StreamGenerateContent, upstream_kind);
    let inbound = OperationKey::content_generation(Operation::StreamGenerateContent, inbound_kind);
    let pair = crate::transform::resolve(upstream, inbound).unwrap();
    let mut transformer =
        SseTransformer::new(pair, TransformContext::new(upstream, inbound)).unwrap();
    let mut out = transformer.push(input.as_bytes()).unwrap();
    out.extend(transformer.finish().unwrap());
    String::from_utf8(out).unwrap()
}

fn sse_values(text: &str) -> Vec<Value> {
    text.lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|data| *data != "[DONE]")
        .map(|data| serde_json::from_str(data).unwrap())
        .collect()
}

fn assert_claude_lifecycle(text: &str) {
    let events = sse_values(text);
    let mut started = false;
    let mut open_block = None;
    let mut stopped = false;
    for event in &events {
        match event["type"].as_str().unwrap() {
            "message_start" => {
                assert!(!started);
                started = true;
            }
            "content_block_start" => {
                assert!(started && open_block.is_none());
                open_block = event["index"].as_u64();
            }
            "content_block_delta" => {
                assert_eq!(open_block, event["index"].as_u64());
            }
            "content_block_stop" => {
                assert_eq!(open_block.take(), event["index"].as_u64());
            }
            "message_delta" => assert!(started && open_block.is_none()),
            "message_stop" => {
                assert!(started && open_block.is_none());
                stopped = true;
            }
            other => panic!("unexpected Claude event {other}"),
        }
    }
    assert!(stopped, "missing message_stop: {text}");
    assert_eq!(
        events.last().and_then(|event| event["type"].as_str()),
        Some("message_stop")
    );
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
fn streams_targeting_claude_have_complete_lifecycles() {
    let cases = [
        (
            ContentGenerationKind::OpenAiChatCompletions,
            concat!(
                "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"pong\"},\"finish_reason\":\"stop\"}]}\n\n",
                "data: [DONE]\n\n",
            ),
        ),
        (
            ContentGenerationKind::OpenAiResponses,
            concat!(
                "event: response.created\n",
                "data: {\"type\":\"response.created\",\"response\":{\"id\":\"r1\",\"created_at\":1,\"model\":\"m\",\"object\":\"response\",\"output\":[],\"status\":\"in_progress\"}}\n\n",
                "event: response.content_part.added\n",
                "data: {\"type\":\"response.content_part.added\",\"content_index\":0,\"item_id\":\"msg1\",\"output_index\":0,\"part\":{\"type\":\"output_text\",\"text\":\"\",\"logprobs\":[]}}\n\n",
                "event: response.output_text.delta\n",
                "data: {\"type\":\"response.output_text.delta\",\"content_index\":0,\"delta\":\"pong\",\"item_id\":\"msg1\",\"output_index\":0}\n\n",
                "event: response.completed\n",
                "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"created_at\":1,\"model\":\"m\",\"object\":\"response\",\"output\":[],\"status\":\"completed\"}}\n\n",
            ),
        ),
        (
            ContentGenerationKind::GeminiGenerateContent,
            "data: {\"responseId\":\"r1\",\"modelVersion\":\"m\",\"candidates\":[{\"index\":0,\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"pong\"}]},\"finishReason\":\"STOP\"}]}\n\n",
        ),
    ];

    for (source, input) in cases {
        let text = transform_sse(source, ContentGenerationKind::ClaudeMessages, input);
        assert!(text.contains("pong"), "{source:?}: {text}");
        assert_claude_lifecycle(&text);
    }
}

#[test]
fn responses_metadata_is_folded_into_chat_and_gemini_content_frames() {
    let input = concat!(
        "event: response.created\n",
        "data: {\"type\":\"response.created\",\"response\":{\"id\":\"r1\",\"created_at\":1,\"model\":\"m\",\"object\":\"response\",\"output\":[],\"status\":\"in_progress\"}}\n\n",
        "event: response.in_progress\n",
        "data: {\"type\":\"response.in_progress\",\"response\":{\"id\":\"r1\",\"created_at\":1,\"model\":\"m\",\"object\":\"response\",\"output\":[],\"status\":\"in_progress\"}}\n\n",
        "event: response.output_item.added\n",
        "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg1\",\"role\":\"assistant\",\"content\":[],\"status\":\"in_progress\"}}\n\n",
        "event: response.output_text.delta\n",
        "data: {\"type\":\"response.output_text.delta\",\"content_index\":0,\"delta\":\"pong\",\"item_id\":\"msg1\",\"output_index\":0}\n\n",
        "event: response.completed\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"r1\",\"created_at\":1,\"model\":\"m\",\"object\":\"response\",\"output\":[],\"status\":\"completed\"}}\n\n",
    );

    let chat = sse_values(&transform_sse(
        ContentGenerationKind::OpenAiResponses,
        ContentGenerationKind::OpenAiChatCompletions,
        input,
    ));
    assert!(
        chat.iter()
            .all(|chunk| !chunk["choices"].as_array().unwrap().is_empty())
    );
    assert_eq!(chat[0]["choices"][0]["delta"]["role"], "assistant");
    assert!(
        chat.iter()
            .all(|chunk| chunk["id"] == "r1" && chunk["model"] == "m")
    );

    let gemini = sse_values(&transform_sse(
        ContentGenerationKind::OpenAiResponses,
        ContentGenerationKind::GeminiGenerateContent,
        input,
    ));
    assert!(
        gemini
            .iter()
            .all(|chunk| !chunk["candidates"].as_array().unwrap().is_empty())
    );
    assert!(
        gemini
            .iter()
            .all(|chunk| { chunk["responseId"] == "r1" && chunk["modelVersion"] == "m" })
    );
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
fn skip_invalid_recognizes_terminal_envelope_on_the_cold_path() {
    let upstream = OperationKey::content_generation(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiResponses,
    );
    let inbound = OperationKey::content_generation(
        Operation::StreamGenerateContent,
        ContentGenerationKind::OpenAiChatCompletions,
    );
    let pair = crate::transform::resolve(upstream, inbound).unwrap();
    let options = StreamOptions {
        error_mode: StreamErrorMode::SkipInvalid,
        ..StreamOptions::default()
    };
    let mut transformer =
        SseTransformer::with_options(pair, TransformContext::new(upstream, inbound), options)
            .unwrap();

    // The type is a terminal event, but its body is deliberately too new for
    // the current strong type. SkipInvalid must retain the old terminal hint.
    let out = transformer
        .push(b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"future\":true}}\n\n")
        .unwrap();
    assert!(out.is_empty());
    assert_eq!(transformer.diagnostics().skipped_frames, 1);
    assert!(transformer.finish().is_ok());
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
