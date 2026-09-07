use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind as Kind, Operation};
use serde_json::Value;

use super::super::content;
use crate::ResponseStream;

fn frame_kind(frame: &Bytes) -> String {
    let wire = String::from_utf8_lossy(frame);
    let data = wire
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .collect::<Vec<_>>()
        .join("\n");
    if data == "[DONE]" {
        return data;
    }
    let value: Value = serde_json::from_str(&data).unwrap();
    if value["object"] == "chat.completion.chunk" {
        let choice = &value["choices"][0];
        if choice["delta"]["role"] == "assistant" {
            return "chat.role".into();
        }
        if choice["delta"]["content"].is_string() {
            return "chat.text".into();
        }
        if choice["finish_reason"].is_string() {
            return "chat.finish".into();
        }
    }
    value["type"].as_str().unwrap_or("chat.empty").into()
}

fn push(stream: &mut ResponseStream, event: Option<&str>, data: &str) -> Vec<String> {
    let wire = event.map_or_else(
        || format!("data: {data}\n\n"),
        |event| format!("event: {event}\ndata: {data}\n\n"),
    );
    stream
        .push(Bytes::from(wire))
        .unwrap()
        .iter()
        .map(frame_kind)
        .collect()
}

#[test]
fn responses_to_chat_forwards_source_order_without_terminal_replay() {
    let mut stream = ResponseStream::new(
        content(Operation::StreamGenerateContent, Kind::OpenAiChat),
        content(Operation::StreamGenerateContent, Kind::OpenAiResponses),
    )
    .unwrap();
    let inputs = [
        (
            "response.created",
            r#"{"type":"response.created","response":{"id":"resp_1","object":"response","created_at":1,"model":"gpt","status":"in_progress","output":[]}}"#,
        ),
        (
            "response.output_item.added",
            r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"message","id":"msg_1","role":"assistant","status":"in_progress","content":[]}}"#,
        ),
        (
            "response.output_text.delta",
            r#"{"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"content_index":0,"delta":"hello"}"#,
        ),
        (
            "response.completed",
            r#"{"type":"response.completed","response":{"id":"resp_1","object":"response","created_at":1,"model":"gpt","status":"completed","output":[{"type":"message","id":"msg_1","role":"assistant","status":"completed","content":[{"type":"output_text","text":"hello","annotations":[]}]}]}}"#,
        ),
    ];
    let mut actual = Vec::new();
    for (event, data) in inputs {
        actual.extend(push(&mut stream, Some(event), data));
    }
    actual.extend(stream.finish().unwrap().iter().map(frame_kind));
    assert_eq!(actual, ["chat.role", "chat.text", "chat.finish", "[DONE]"]);
}

#[test]
fn responses_sparse_tool_arguments_are_recovered_from_the_done_item() {
    let cases = [
        (Kind::OpenAiChat, "chat.completion.chunk"),
        (Kind::ClaudeMessages, "input_json_delta"),
    ];
    for (target, marker) in cases {
        let mut stream = ResponseStream::new(
            content(Operation::StreamGenerateContent, target),
            content(Operation::StreamGenerateContent, Kind::OpenAiResponses),
        )
        .unwrap();
        let inputs = [
            r#"{"type":"response.created","response":{"id":"resp_tool","object":"response","created_at":1,"model":"gpt","status":"in_progress","output":[]}}"#,
            r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"fc_1","call_id":"call_1","name":"get_weather","arguments":"","status":"in_progress"}}"#,
            r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"function_call","id":"fc_1","call_id":"call_1","name":"get_weather","arguments":"{\"city\":\"Beijing\"}","status":"completed"}}"#,
            r#"{"type":"response.completed","response":{"id":"resp_tool","object":"response","created_at":1,"model":"gpt","status":"completed","output":[{"type":"function_call","id":"fc_1","call_id":"call_1","name":"get_weather","arguments":"{\"city\":\"Beijing\"}","status":"completed"}]}}"#,
        ];
        let mut output = Vec::new();
        for input in inputs {
            for frame in stream
                .push(Bytes::from(format!("data: {input}\n\n")))
                .unwrap()
            {
                output.extend_from_slice(&frame);
            }
        }
        for frame in stream.finish().unwrap() {
            output.extend_from_slice(&frame);
        }
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains(marker), "missing {marker}: {output}");
        assert!(
            output.contains("Beijing"),
            "missing tool arguments: {output}"
        );
    }
}

#[test]
fn chat_to_responses_emits_content_lifecycle_before_terminal() {
    let mut stream = ResponseStream::new(
        content(Operation::StreamGenerateContent, Kind::OpenAiResponses),
        content(Operation::StreamGenerateContent, Kind::OpenAiChat),
    )
    .unwrap();
    let mut actual = push(
        &mut stream,
        None,
        r#"{"id":"chat_1","object":"chat.completion.chunk","created":1,"model":"gpt","choices":[{"index":0,"delta":{"role":"assistant","content":"he"},"finish_reason":null}]}"#,
    );
    actual.extend(push(
        &mut stream,
        None,
        r#"{"id":"chat_1","object":"chat.completion.chunk","created":1,"model":"gpt","choices":[{"index":0,"delta":{"content":"llo"},"finish_reason":"stop"}]}"#,
    ));
    actual.extend(push(&mut stream, None, "[DONE]"));
    actual.extend(stream.finish().unwrap().iter().map(frame_kind));
    assert_eq!(
        actual,
        [
            "response.created",
            "response.output_item.added",
            "response.content_part.added",
            "response.output_text.delta",
            "response.output_text.delta",
            "response.output_text.done",
            "response.content_part.done",
            "response.output_item.done",
            "response.completed",
        ]
    );
}

#[test]
fn chat_to_responses_keeps_refusal_and_legacy_function_order() {
    let mut stream = ResponseStream::new(
        content(Operation::StreamGenerateContent, Kind::OpenAiResponses),
        content(Operation::StreamGenerateContent, Kind::OpenAiChat),
    )
    .unwrap();
    let mut actual = push(
        &mut stream,
        None,
        r#"{"id":"chat_2","object":"chat.completion.chunk","created":1,"model":"gpt","choices":[{"index":0,"delta":{"refusal":"no"},"finish_reason":null,"logprobs":{"content":[],"refusal":[{"token":"no","logprob":-0.1,"bytes":[110,111],"top_logprobs":[]}]}}]}"#,
    );
    actual.extend(push(
        &mut stream,
        None,
        r#"{"id":"chat_2","object":"chat.completion.chunk","created":1,"model":"gpt","choices":[{"index":0,"delta":{"function_call":{"name":"lookup","arguments":"{}"}},"finish_reason":"function_call"}]}"#,
    ));
    actual.extend(push(&mut stream, None, "[DONE]"));
    assert!(stream.finish().unwrap().is_empty());
    assert_eq!(
        actual,
        [
            "response.created",
            "response.output_item.added",
            "response.content_part.added",
            "response.refusal.delta",
            "response.refusal.done",
            "response.content_part.done",
            "response.output_item.done",
            "response.output_item.added",
            "response.function_call_arguments.delta",
            "response.function_call_arguments.done",
            "response.output_item.done",
            "response.completed",
        ]
    );
}

#[test]
fn chat_to_gemini_waits_for_complete_tool_arguments() {
    use serde_json::json;

    let mut stream = ResponseStream::new(
        content(
            Operation::StreamGenerateContent,
            Kind::GeminiGenerateContent,
        ),
        content(Operation::StreamGenerateContent, Kind::OpenAiChat),
    )
    .unwrap();
    let chunks = [
        json!({"tool_calls":[
            {"index":0,"id":"shell","type":"function","function":{"name":"run_shell_command","arguments":""}},
            {"index":1,"id":"edit","type":"function","function":{"name":"replace","arguments":"{\"old_string\":"}}
        ]}),
        json!({"tool_calls":[
            {"index":1,"function":{"arguments":"\"bad\",\"new_string\":\"good\"}"}},
            {"index":0,"function":{"arguments":"{\"command\":\"python3 "}}
        ]}),
        json!({"tool_calls":[{"index":0,"function":{"arguments":"test_calc.py\"}"}}]}),
        json!({}),
    ];
    let mut calls = Vec::new();
    for (index, delta) in chunks.into_iter().enumerate() {
        let chunk = json!({"id":"chat_tool","object":"chat.completion.chunk","created":1,
            "model":"gpt","choices":[{"index":0,"delta":delta,
                "finish_reason":(index == 3).then_some("tool_calls")}]});
        let output = stream
            .push(Bytes::from(format!("data: {chunk}\n\n")))
            .unwrap();
        for event in super::super::support::data_frames(&output.concat()) {
            if let Some(parts) = event
                .pointer("/candidates/0/content/parts")
                .and_then(Value::as_array)
            {
                assert_eq!(index, 3, "Gemini calls must contain complete arguments");
                calls.extend(parts.iter().map(|part| part["functionCall"].clone()));
            }
        }
    }
    assert_eq!(
        calls,
        vec![
            json!({"id":"shell","name":"run_shell_command","args":{"command":"python3 test_calc.py"}}),
            json!({"id":"edit","name":"replace","args":{"old_string":"bad","new_string":"good"}}),
        ]
    );
    assert!(
        stream
            .push(Bytes::from_static(b"data: [DONE]\n\n"))
            .unwrap()
            .is_empty()
    );
    assert!(stream.finish().unwrap().is_empty());
}

#[test]
fn gemini_to_responses_closes_candidates_in_source_order_without_eof_delay() {
    let mut stream = ResponseStream::new(
        content(Operation::StreamGenerateContent, Kind::OpenAiResponses),
        content(
            Operation::StreamGenerateContent,
            Kind::GeminiGenerateContent,
        ),
    )
    .unwrap();
    let actual = push(
        &mut stream,
        None,
        r#"{"responseId":"gemini_multi","modelVersion":"gemini","candidates":[{"index":0,"content":{"role":"model","parts":[{"text":"first"}]},"finishReason":"STOP"},{"index":1,"content":{"role":"model","parts":[{"text":"second"}]},"finishReason":"STOP"}]}"#,
    );
    assert_eq!(
        actual,
        [
            "response.created",
            "response.output_item.added",
            "response.content_part.added",
            "response.output_text.delta",
            "response.output_text.done",
            "response.content_part.done",
            "response.output_item.done",
            "response.output_item.added",
            "response.content_part.added",
            "response.output_text.delta",
            "response.output_text.done",
            "response.content_part.done",
            "response.output_item.done",
            "response.completed",
        ]
    );
}

/// A network chunk can end in the middle of a multi-byte character. Decoding each
/// chunk on its own turns that character into U+FFFD, which is what shipped in v2
/// until it buffered the incomplete tail — the symptom was mojibake in streamed
/// CJK output. v3 accumulates bytes and only decodes a delimited frame, so the
/// split is invisible; this pins that, because the shape is easy to lose in a
/// refactor and nothing else would notice.
#[test]
fn a_character_split_across_chunks_survives() {
    let mut stream = ResponseStream::new(
        content(Operation::StreamGenerateContent, Kind::OpenAiChat),
        content(Operation::StreamGenerateContent, Kind::OpenAiResponses),
    )
    .unwrap();
    let wire = concat!(
        "data: {\"type\":\"response.created\",\"sequence_number\":0,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"in_progress\",\"model\":\"gpt\",\"output\":[]}}\n\n",
        "data: {\"type\":\"response.output_text.delta\",\"sequence_number\":1,\"item_id\":\"msg_1\",\"output_index\":0,\"content_index\":0,\"delta\":\"汉字\"}\n\n",
        "data: {\"type\":\"response.completed\",\"sequence_number\":2,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"completed\",\"model\":\"gpt\",\"output\":[{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\",\"status\":\"completed\",\"content\":[{\"type\":\"output_text\",\"text\":\"汉字\",\"annotations\":[],\"logprobs\":[]}]}],\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n",
    );
    // Cut one byte into the three-byte 汉, so the first chunk ends mid-character.
    let split = wire.find('汉').expect("payload carries the character") + 1;
    let mut output = Vec::new();
    for part in [&wire.as_bytes()[..split], &wire.as_bytes()[split..]] {
        for frame in stream.push(Bytes::copy_from_slice(part)).unwrap() {
            output.extend_from_slice(&frame);
        }
    }
    for frame in stream.finish().unwrap() {
        output.extend_from_slice(&frame);
    }
    let text = String::from_utf8(output).expect("output stays valid UTF-8");
    assert!(text.contains("汉字"), "character was mangled: {text}");
    assert!(
        !text.contains('\u{fffd}'),
        "replacement character emitted: {text}"
    );
}
