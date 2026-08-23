use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind as Kind, Operation, OperationKey, WireFamily};
use serde_json::{Value, json};

use crate::{ResponseStream, can_transform, request, response};

fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

fn content(operation: Operation, kind: Kind) -> OperationKey {
    OperationKey::content(operation, kind)
}

fn convert_request(source: OperationKey, target: OperationKey, value: Value) -> Value {
    let bytes = request(
        source,
        target,
        Bytes::from(serde_json::to_vec(&value).unwrap()),
        "upstream-model",
        false,
    )
    .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn convert_response(source: OperationKey, target: OperationKey, value: Value) -> Value {
    let bytes = response(
        source,
        target,
        Bytes::from(serde_json::to_vec(&value).unwrap()),
    )
    .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn pair_matrix_models_and_count_tokens_are_bidirectional() {
    let pairs = [
        (
            family(Operation::ListModels, WireFamily::OpenAi),
            family(Operation::ListModels, WireFamily::Claude),
        ),
        (
            family(Operation::GetModel, WireFamily::Claude),
            family(Operation::GetModel, WireFamily::OpenAi),
        ),
        (
            family(Operation::CountTokens, WireFamily::OpenAi),
            family(Operation::CountTokens, WireFamily::Claude),
        ),
        (
            content(Operation::GenerateContent, Kind::OpenAiChat),
            content(Operation::GenerateContent, Kind::ClaudeMessages),
        ),
        (
            content(Operation::StreamGenerateContent, Kind::ClaudeMessages),
            content(Operation::StreamGenerateContent, Kind::OpenAiResponses),
        ),
        (
            family(Operation::CompactContent, WireFamily::OpenAi),
            content(Operation::GenerateContent, Kind::ClaudeMessages),
        ),
    ];
    for (source, target) in pairs {
        assert!(can_transform(source, target), "{source:?} -> {target:?}");
    }
    assert!(!can_transform(
        content(Operation::GenerateContent, Kind::OpenAiChat),
        content(Operation::GenerateContent, Kind::OpenAiResponses),
    ));

    let openai_source = family(Operation::ListModels, WireFamily::OpenAi);
    let claude_target = family(Operation::ListModels, WireFamily::Claude);
    let models = convert_response(
        openai_source,
        claude_target,
        json!({"data":[{
            "id":"claude-opus","type":"model","display_name":"Claude Opus",
            "created_at":"2026-01-01T00:00:00Z","max_input_tokens":200000,"max_tokens":32000
        }],"first_id":"claude-opus","last_id":"claude-opus","has_more":false}),
    );
    assert_eq!(models["object"], "list");
    assert_eq!(models["data"][0]["id"], "claude-opus");
    assert_eq!(models["data"][0]["context_window"], 200000);

    let count = convert_request(
        family(Operation::CountTokens, WireFamily::OpenAi),
        family(Operation::CountTokens, WireFamily::Claude),
        json!({
            "model":"route","instructions":"be exact","input":"hello",
            "tools":[{"type":"function","name":"lookup","parameters":{"type":"object"}}]
        }),
    );
    assert_eq!(count["model"], "upstream-model");
    assert_eq!(count["system"], "be exact");
    assert_eq!(count["messages"][0]["role"], "user");
    assert_eq!(count["tools"][0]["name"], "lookup");
    assert!(count.get("max_tokens").is_none());
    let counted = convert_response(
        family(Operation::CountTokens, WireFamily::OpenAi),
        family(Operation::CountTokens, WireFamily::Claude),
        json!({"input_tokens":42}),
    );
    assert_eq!(
        counted,
        json!({"object":"response.input_tokens","input_tokens":42})
    );
}

#[test]
fn buffered_content_and_compact_preserve_turns_tools_stops_and_usage() {
    let chat = content(Operation::GenerateContent, Kind::OpenAiChat);
    let claude = content(Operation::GenerateContent, Kind::ClaudeMessages);
    let request = convert_request(
        chat,
        claude,
        json!({
            "model":"route","max_completion_tokens":128,
            "messages":[
                {"role":"system","content":"policy"},
                {"role":"user","content":[{"type":"text","text":"question"}]},
                {"role":"assistant","content":"checking","tool_calls":[{
                    "id":"call_1","type":"function",
                    "function":{"name":"lookup","arguments":"{\"q\":\"x\"}"}
                }]},
                {"role":"tool","tool_call_id":"call_1","content":"answer"}
            ],
            "tools":[{"type":"function","function":{
                "name":"lookup","description":"find","parameters":{"type":"object"}
            }}],
            "tool_choice":"required","parallel_tool_calls":false
        }),
    );
    assert_eq!(request["system"][0]["text"], "policy");
    assert_eq!(request["messages"][1]["content"][1]["type"], "tool_use");
    assert_eq!(request["messages"][2]["content"][0]["type"], "tool_result");
    assert_eq!(request["tools"][0]["input_schema"]["type"], "object");
    assert_eq!(request["tool_choice"]["type"], "any");

    let chat_response = convert_response(
        chat,
        claude,
        json!({
            "id":"msg_1","type":"message","role":"assistant","model":"claude-opus",
            "content":[
                {"type":"text","text":"done"},
                {"type":"tool_use","id":"call_2","name":"save","input":{"x":1}}
            ],
            "stop_reason":"tool_use","stop_sequence":null,
            "usage":{"input_tokens":10,"cache_read_input_tokens":5,"output_tokens":3}
        }),
    );
    assert_eq!(chat_response["choices"][0]["message"]["content"], "done");
    assert_eq!(
        chat_response["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
        "{\"x\":1}"
    );
    assert_eq!(chat_response["choices"][0]["finish_reason"], "tool_calls");
    assert_eq!(chat_response["usage"]["prompt_tokens"], 15);

    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let response_request = convert_request(
        responses,
        claude,
        json!({
            "model":"route","instructions":"policy",
            "input":[
                {"type":"message","role":"user","content":[{"type":"input_text","text":"go"}]},
                {"type":"function_call","id":"fc_1","call_id":"c1","name":"lookup","arguments":"{\"q\":1}"},
                {"type":"function_call_output","call_id":"c1","output":"ok"}
            ],
            "tools":[{"type":"function","name":"lookup","parameters":{"type":"object"}}]
        }),
    );
    assert_eq!(
        response_request["messages"][1]["content"][0]["type"],
        "tool_use"
    );
    assert_eq!(
        response_request["messages"][2]["content"][0]["type"],
        "tool_result"
    );

    let responses_response = convert_response(
        responses,
        claude,
        json!({
            "id":"msg_2","type":"message","role":"assistant","model":"claude-opus",
            "content":[{"type":"thinking","thinking":"work","signature":"sig"},{"type":"text","text":"answer"}],
            "stop_reason":"end_turn","usage":{"input_tokens":7,"output_tokens":2}
        }),
    );
    assert_eq!(responses_response["status"], "completed");
    assert_eq!(responses_response["output_text"], "answer");
    assert_eq!(responses_response["output"][0]["type"], "reasoning");

    let compact = convert_response(
        family(Operation::CompactContent, WireFamily::OpenAi),
        claude,
        json!({
            "id":"msg_compact","type":"message","role":"assistant","model":"claude-opus",
            "content":[{"type":"text","text":"summary"}],
            "stop_reason":"compaction","usage":{"input_tokens":9,"output_tokens":1}
        }),
    );
    assert_eq!(compact["object"], "response.compaction");
    assert_eq!(compact["output"][0]["content"][0]["type"], "text");
}

#[test]
fn split_sse_frames_preserve_lifecycle_text_tools_and_usage() {
    let chat = content(Operation::StreamGenerateContent, Kind::OpenAiChat);
    let claude = content(Operation::StreamGenerateContent, Kind::ClaudeMessages);
    let claude_wire = concat!(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"model\":\"claude-opus\",\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
    );
    let chat_out = drive(ResponseStream::new(chat, claude).unwrap(), claude_wire, 17);
    let chat_frames = data_frames(&chat_out);
    assert!(chat_frames.iter().any(|value| {
        value.pointer("/choices/0/delta/content") == Some(&Value::String("hi".into()))
    }));
    assert!(
        chat_frames
            .iter()
            .any(|value| value["usage"]["completion_tokens"] == 2)
    );
    assert!(String::from_utf8_lossy(&chat_out).contains("data: [DONE]"));

    let chat_wire = concat!(
        "data: {\"id\":\"chat_1\",\"model\":\"gpt\",\"choices\":[{\"delta\":{\"role\":\"assistant\",\"content\":\"hel\"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"chat_1\",\"model\":\"gpt\",\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":2}}\n\n",
        "data: [DONE]\n\n"
    );
    let claude_out = drive(ResponseStream::new(claude, chat).unwrap(), chat_wire, 13);
    let text = String::from_utf8_lossy(&claude_out);
    assert!(text.contains("message_start"));
    assert!(text.contains("text_delta"));
    assert!(text.contains("hel"));
    assert!(text.contains("lo"));
    assert!(text.contains("message_stop"));

    let responses = content(Operation::StreamGenerateContent, Kind::OpenAiResponses);
    let responses_wire = concat!(
        "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt\"}}\n\n",
        "event: response.content_part.added\ndata: {\"type\":\"response.content_part.added\",\"item_id\":\"item_1\",\"part\":{\"type\":\"output_text\"}}\n\n",
        "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"item_id\":\"item_1\",\"delta\":\"answer\"}\n\n",
        "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt\",\"usage\":{\"input_tokens\":3,\"output_tokens\":1}}}\n\n"
    );
    let claude_out = drive(
        ResponseStream::new(claude, responses).unwrap(),
        responses_wire,
        19,
    );
    let text = String::from_utf8_lossy(&claude_out);
    assert!(text.contains("answer"));
    assert!(text.contains("message_stop"));
}

fn drive(mut stream: ResponseStream, wire: &str, chunk: usize) -> Vec<u8> {
    let mut output = Vec::new();
    for part in wire.as_bytes().chunks(chunk) {
        for frame in stream.push(Bytes::copy_from_slice(part)).unwrap() {
            output.extend_from_slice(&frame);
        }
    }
    for frame in stream.finish().unwrap() {
        output.extend_from_slice(&frame);
    }
    output
}

fn data_frames(wire: &[u8]) -> Vec<Value> {
    String::from_utf8_lossy(wire)
        .split("\n\n")
        .filter_map(|frame| {
            let data = frame
                .lines()
                .filter_map(|line| line.strip_prefix("data: "))
                .collect::<Vec<_>>()
                .join("\n");
            serde_json::from_str(&data).ok()
        })
        .collect()
}
