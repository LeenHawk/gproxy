use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind as Kind, Operation, OperationKey, WireFamily};
use serde_json::{Value, json};

use crate::{BufferedResponse, ResponseCollector};
use crate::{ResponseStream, can_transform, request, response};

mod buffered_responses;
mod native_tools;
mod ported_surface;
mod request_parity;
mod stream_lifecycle;
mod strict_extensions;
mod support;
mod thought_signature;
mod typed;
use support::*;

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
            content(Operation::GenerateContent, Kind::ClaudeMessages),
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
    assert!(can_transform(
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
    assert!(models["data"][0].get("created").is_none());
    assert_eq!(models["data"][0]["owned_by"], "unknown");

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
                {"role":"user","content":[{"type":"text","text":"question","part_future":7}],"message_future":8},
                {"role":"assistant","content":"checking","tool_calls":[{
                    "id":"call_1","type":"function",
                    "function":{"name":"lookup","arguments":"{\"q\":\"x\"}"}
                }]},
                {"role":"tool","tool_call_id":"call_1","content":"answer"}
            ],
            "tools":[{"type":"function","function":{
                "name":"lookup","description":"find","parameters":{"type":"object"},"tool_future":9
            }}],
            "tool_choice":"required","parallel_tool_calls":false,
            "root_future":10
        }),
    );
    assert_eq!(request["system"][0]["text"], "policy");
    assert_eq!(request["messages"][1]["content"][1]["type"], "tool_use");
    assert_eq!(request["messages"][2]["content"][0]["type"], "tool_result");
    assert_eq!(request["tools"][0]["input_schema"]["type"], "object");
    assert_eq!(request["tool_choice"]["type"], "any");
    assert!(request.get("root_future").is_none());
    assert!(request["messages"][0].get("message_future").is_none());
    assert!(
        request["messages"][0]["content"][0]
            .get("part_future")
            .is_none()
    );
    assert!(request["tools"][0].get("tool_future").is_none());

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
            "usage":{"input_tokens":10,"cache_read_input_tokens":5,"output_tokens":3},
            "response_future":11
        }),
    );
    assert_eq!(chat_response["choices"][0]["message"]["content"], "done");
    assert_eq!(
        chat_response["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
        "{\"x\":1}"
    );
    assert_eq!(chat_response["choices"][0]["finish_reason"], "tool_calls");
    assert_eq!(chat_response["usage"]["prompt_tokens"], 15);
    assert!(chat_response.get("response_future").is_none());

    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let response_request = convert_request(
        responses,
        claude,
        json!({
            "model":"route","instructions":"policy","max_output_tokens":128,
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
    assert!(responses_response.get("created_at").is_none());
    assert!(responses_response.get("completed_at").is_none());

    let ordered = convert_response(
        responses,
        claude,
        json!({
            "id":"msg_order","type":"message","role":"assistant","model":"claude-opus",
            "content":[
                {"type":"text","text":"before"},
                {"type":"tool_use","id":"call_order","name":"lookup","input":{}},
                {"type":"text","text":"after"}
            ],
            "stop_reason":"tool_use","usage":{"input_tokens":4,"output_tokens":2}
        }),
    );
    assert_eq!(ordered["output"][0]["type"], "function_call");
    assert_eq!(ordered["output"][1]["type"], "message");
    assert!(ordered["output"][0].get("id").is_none());
    assert_eq!(ordered["output"][1]["id"], "msg_msg_order_0");
    assert_eq!(ordered["output"][1]["content"][0]["text"], "before");
    assert_eq!(ordered["output"][1]["content"][1]["text"], "after");

    // Compact carries no caller budget and Claude demands one, so every compact
    // request failed until the default came back.
    let compact_request = convert_request(
        family(Operation::CompactContent, WireFamily::OpenAi),
        claude,
        json!({"input": [{"role": "user", "content": "summarise"}]}),
    );
    assert_eq!(compact_request["max_tokens"], 32_768);

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

    for (thinking, expected) in [
        (json!({"type":"disabled"}), Some("none")),
        (
            json!({"type":"enabled","budget_tokens":1024}),
            Some("medium"),
        ),
        (json!({"type":"adaptive"}), Some("medium")),
        (json!({"type":"future_thinking"}), None),
    ] {
        let converted = convert_request(
            claude,
            responses,
            json!({
                "model":"claude-opus","max_tokens":32,
                "messages":[{"role":"user","content":"hello"}],
                "thinking":thinking
            }),
        );
        assert_eq!(
            converted
                .pointer("/reasoning/effort")
                .and_then(Value::as_str),
            expected
        );
    }
}

#[test]
fn split_sse_frames_preserve_lifecycle_text_tools_and_usage() {
    let chat = content(Operation::StreamGenerateContent, Kind::OpenAiChat);
    let claude = content(Operation::StreamGenerateContent, Kind::ClaudeMessages);
    let claude_wire = concat!(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-opus\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":5,\"output_tokens\":0}}}\n\n",
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
        "data: {\"id\":\"chat_1\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"gpt\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"hel\"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"chat_1\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"gpt\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":2,\"total_tokens\":6}}\n\n",
        "data: [DONE]\n\n"
    );
    let claude_out = drive(ResponseStream::new(claude, chat).unwrap(), chat_wire, 13);
    let text = String::from_utf8_lossy(&claude_out);
    assert!(text.contains("message_start"));
    assert!(text.contains("content_block_start"));
    assert!(text.contains("hel"));
    assert!(text.contains("lo"));
    assert!(text.contains("message_stop"));

    let responses = content(Operation::StreamGenerateContent, Kind::OpenAiResponses);
    let responses_wire = concat!(
        "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"model\":\"gpt\",\"status\":\"in_progress\",\"output\":[]}}\n\n",
        "event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"item_1\",\"role\":\"assistant\",\"content\":[],\"status\":\"in_progress\"}}\n\n",
        "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"item_id\":\"item_1\",\"output_index\":0,\"content_index\":0,\"delta\":\"answer\"}\n\n",
        "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"model\":\"gpt\",\"status\":\"completed\",\"output\":[{\"type\":\"message\",\"id\":\"item_1\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"answer\",\"annotations\":[]}],\"status\":\"completed\"}],\"usage\":{\"input_tokens\":3,\"output_tokens\":1,\"total_tokens\":4,\"output_tokens_details\":{\"reasoning_tokens\":0}}}}\n\n"
    );
    let claude_out = drive(
        ResponseStream::new(claude, responses).unwrap(),
        responses_wire,
        19,
    );
    let text = String::from_utf8_lossy(&claude_out);
    assert!(text.contains("answer"));
    assert!(text.contains("message_stop"));

    let chat_from_responses = drive(
        ResponseStream::new(chat, responses).unwrap(),
        responses_wire,
        23,
    );
    let text = String::from_utf8_lossy(&chat_from_responses);
    assert!(text.contains("answer"));
    assert!(text.contains("[DONE]"));

    let responses_from_chat = drive(ResponseStream::new(responses, chat).unwrap(), chat_wire, 29);
    let text = String::from_utf8_lossy(&responses_from_chat);
    assert!(text.contains("response.completed"));
    assert!(text.contains("he"));
    assert!(text.contains("lo"));

    let custom_chat = concat!(
        "data: {\"id\":\"chat_custom\",\"object\":\"chat.completion.chunk\",\"created\":123,\"model\":\"gpt\",\"root_future\":9,\"choices\":[{\"index\":0,\"delta\":{\"content\":\"x\",\"tool_calls\":[{\"index\":0,\"id\":\"ct_1\",\"type\":\"custom\",\"custom\":{\"name\":\"exec\",\"input\":\"a\",\"custom_future\":7},\"call_future\":8}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: [DONE]\n\n"
    );
    let custom_out = drive(
        ResponseStream::new(responses, chat).unwrap(),
        custom_chat,
        31,
    );
    let custom_frames = data_frames(&custom_out);
    let delta = custom_frames
        .iter()
        .find(|v| v["type"] == "response.custom_tool_call_input.delta")
        .unwrap();
    assert_eq!(
        (delta["item_id"].as_str(), delta["output_index"].as_u64()),
        (Some("ct_1"), Some(1))
    );
    let item = custom_frames
        .iter()
        .find(|v| v["item"]["type"] == "custom_tool_call")
        .unwrap();
    assert!(item["item"].get("custom_future").is_none());
    assert!(item["item"].get("call_future").is_none());
    let done = custom_frames
        .iter()
        .filter(|v| v["type"] == "response.output_item.done")
        .map(|v| v["output_index"].as_u64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(done, [0, 1]);
    let terminal = custom_frames
        .iter()
        .find(|v| v["type"] == "response.completed")
        .unwrap();
    assert_eq!(
        (
            terminal["response"]["status"].as_str(),
            terminal["response"]["created_at"].as_u64(),
            terminal["response"].get("root_future")
        ),
        (Some("completed"), Some(123), None)
    );
    assert_eq!(terminal["response"]["completed_at"], 123);

    let custom_responses = "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_custom\",\"object\":\"response\",\"created_at\":4,\"model\":\"gpt\",\"status\":\"in_progress\",\"output\":[]}}\n\nevent: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"custom_tool_call\",\"id\":\"ct_2\",\"call_id\":\"call_2\",\"name\":\"exec\",\"input\":\"\"}}\n\nevent: response.custom_tool_call_input.delta\ndata: {\"type\":\"response.custom_tool_call_input.delta\",\"item_id\":\"ct_2\",\"output_index\":0,\"delta\":\"a\"}\n\nevent: response.custom_tool_call_input.done\ndata: {\"type\":\"response.custom_tool_call_input.done\",\"item_id\":\"ct_2\",\"output_index\":0,\"input\":\"ab\"}\n\nevent: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_custom\",\"object\":\"response\",\"created_at\":4,\"model\":\"gpt\",\"status\":\"completed\",\"output\":[{\"type\":\"custom_tool_call\",\"id\":\"ct_2\",\"call_id\":\"call_2\",\"name\":\"exec\",\"input\":\"ab\"}]}}\n\n";
    let custom_back = ResponseStream::new(chat, responses).unwrap();
    let custom_back = String::from_utf8(drive(custom_back, custom_responses, 41)).unwrap();
    assert!(custom_back.contains("\"type\":\"custom\""));
    assert!(custom_back.contains("\"custom\":{\"input\":\"b\"}"));

    let final_only = concat!(
        "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_late\",\"object\":\"response\",\"created_at\":5,\"model\":\"gpt\",\"status\":\"in_progress\",\"output\":[]}}\n\n",
        "event: response.incomplete\ndata: {\"type\":\"response.incomplete\",\"response\":{\"id\":\"resp_late\",\"object\":\"response\",\"created_at\":5,\"model\":\"gpt\",\"status\":\"incomplete\",\"incomplete_details\":{\"reason\":\"max_output_tokens\"},\"output\":[{\"type\":\"message\",\"id\":\"msg_late\",\"role\":\"assistant\",\"status\":\"incomplete\",\"content\":[{\"type\":\"output_text\",\"text\":\"late\",\"annotations\":[]}]}]}}\n\n"
    );
    let late = data_frames(&drive(
        ResponseStream::new(chat, responses).unwrap(),
        final_only,
        37,
    ));
    assert!(
        !late.iter().any(|v| {
            v.pointer("/choices/0/delta/content") == Some(&Value::String("late".into()))
        })
    );
    assert!(late.iter().any(|v| v.pointer("/choices/0/finish_reason") == Some(&Value::String("length".into()))));
}

#[test]
fn typed_chat_responses_pairs_drop_unknown_fields_and_promotion_stays_identity() {
    let chat = content(Operation::GenerateContent, Kind::OpenAiChat);
    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let chat_request = json!({
        "model":"route",
        "messages":[{
            "role":"user",
            "content":[{"type":"text","text":"hello","part_future":1}],
            "message_future":2
        }],
        "root_future":{"x":3}
    });
    let converted = convert_request(chat, responses, chat_request.clone());
    assert!(converted.get("root_future").is_none());
    assert!(converted["input"][0].get("message_future").is_none());
    assert!(
        converted["input"][0]["content"][0]
            .get("part_future")
            .is_none()
    );
    let roundtrip = convert_request(responses, chat, converted);
    assert!(roundtrip.get("root_future").is_none());
    assert!(roundtrip["messages"][0].get("message_future").is_none());
    assert!(
        roundtrip["messages"][0]["content"][0]
            .get("part_future")
            .is_none()
    );

    let response_wire = json!({
        "id":"resp_1","object":"response","created_at":0,"model":"gpt",
        "status":"completed","response_future":4,
        "output":[{
            "type":"message","id":"item_1","role":"assistant","status":"completed",
            "content":[{"type":"output_text","text":"answer","annotations":[],"text_future":5}]
        }],
        "usage":{"input_tokens":2,"output_tokens":1,"total_tokens":3,
            "output_tokens_details":{"reasoning_tokens":0}}
    });
    let outward = convert_response(chat, responses, response_wire);
    assert!(outward.get("response_future").is_none());
    assert_eq!(outward["choices"][0]["message"]["content"], "answer");
    assert!(
        outward["choices"][0]["message"]
            .get("text_future")
            .is_none()
    );

    let promoted = content(Operation::StreamGenerateContent, Kind::OpenAiResponses);
    assert!(can_transform(responses, promoted));
    let bytes = Bytes::from_static(br#"{"model":"gpt","input":"hello","future":1}"#);
    assert_eq!(
        request(responses, promoted, bytes.clone(), "gpt", true).unwrap(),
        bytes
    );
    let response_bytes = Bytes::from_static(
        br#"{"id":"resp_promote","object":"response","created_at":0,"status":"completed","output":[],"usage":{"input_tokens":0,"output_tokens":0,"total_tokens":0,"output_tokens_details":{"reasoning_tokens":0}},"future":2}"#,
    );
    assert_eq!(
        response(responses, promoted, response_bytes.clone()).unwrap(),
        response_bytes
    );
}
