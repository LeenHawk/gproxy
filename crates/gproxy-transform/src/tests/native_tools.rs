use gproxy_protocol::{ContentGenerationKind as Kind, Operation};
use serde_json::json;

use super::support::{data_frames, drive};
use super::{ResponseStream, content, convert_request, convert_response};

#[test]
fn buffered_native_tools_preserve_calls_results_and_definition_fallbacks() {
    let claude = content(Operation::GenerateContent, Kind::ClaudeMessages);
    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let outward = convert_request(
        claude,
        responses,
        json!({
            "model":"route","max_tokens":128,
            "tools":[
                {"type":"bash_20250124","name":"bash"},
                {"type":"text_editor_20250728","name":"str_replace_based_edit_tool"},
                {"type":"memory_20250818","name":"memory"}
            ],
            "messages":[
                {"role":"assistant","content":[{"type":"tool_use","id":"call_shell","name":"bash","input":{"command":"pwd"}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"call_shell","content":"/repo"}]},
                {"role":"assistant","content":[{"type":"tool_use","id":"call_patch","name":"str_replace_based_edit_tool","input":{"command":"str_replace","path":"src/lib.rs","old_str":"old","new_str":"new"}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"call_patch","content":"done"}]}
            ]
        }),
    );
    assert_eq!(outward["tools"][0]["type"], "shell");
    assert_eq!(outward["tools"][1]["type"], "apply_patch");
    assert_eq!(outward["tools"][2]["type"], "function");
    assert_eq!(outward["tools"][2]["name"], "memory");
    assert_eq!(outward["input"][0]["type"], "shell_call");
    assert_eq!(outward["input"][1]["type"], "shell_call_output");
    assert_eq!(outward["input"][2]["type"], "apply_patch_call");
    assert_eq!(outward["input"][3]["type"], "apply_patch_call_output");
    assert_eq!(outward["input"][0]["call_id"], "call_shell");
    assert_eq!(outward["input"][1]["call_id"], "call_shell");
    assert_eq!(outward["input"][2]["call_id"], "call_patch");
    assert_eq!(outward["input"][3]["call_id"], "call_patch");
    assert!(outward["input"][0].get("id").is_none());

    let inward = convert_request(
        responses,
        claude,
        json!({
            "model":"route","max_output_tokens":128,
            "tools":[{"type":"shell"},{"type":"apply_patch"}],
            "input":[
                {"type":"shell_call","call_id":"shell_2","action":{"commands":["pwd"]},"status":"completed"},
                {"type":"shell_call_output","call_id":"shell_2","output":[{"outcome":{"type":"exit","exit_code":0},"stdout":"/repo","stderr":""}]},
                {"type":"apply_patch_call","call_id":"patch_2","operation":{"type":"update_file","path":"src/lib.rs","diff":"@@\n-old\n+new\n"},"status":"completed"},
                {"type":"apply_patch_call_output","call_id":"patch_2","status":"failed","output":"conflict"}
            ]
        }),
    );
    assert_eq!(inward["tools"][0]["type"], "bash_20250124");
    assert_eq!(inward["tools"][1]["type"], "text_editor_20250728");
    assert_eq!(inward["messages"][0]["content"][0]["id"], "shell_2");
    assert_eq!(
        inward["messages"][1]["content"][0]["tool_use_id"],
        "shell_2"
    );
    assert_eq!(inward["messages"][2]["content"][0]["id"], "patch_2");
    assert_eq!(
        inward["messages"][3]["content"][0]["tool_use_id"],
        "patch_2"
    );
    assert_eq!(inward["messages"][3]["content"][0]["is_error"], true);

    let missing_file_text = convert_request(
        claude,
        responses,
        json!({
            "model":"route","max_tokens":32,
            "messages":[{"role":"assistant","content":[{
                "type":"tool_use","id":"editor_raw","name":"str_replace_based_edit_tool",
                "input":{"command":"create","path":"new.txt"}
            }]}]
        }),
    );
    assert_eq!(missing_file_text["input"][0]["type"], "function_call");
    assert_eq!(missing_file_text["input"][0]["call_id"], "editor_raw");
}

#[test]
fn buffered_native_response_calls_keep_wire_shape() {
    let chat = content(Operation::GenerateContent, Kind::OpenAiChat);
    let claude = content(Operation::GenerateContent, Kind::ClaudeMessages);
    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let outward = convert_response(
        responses,
        claude,
        json!({
            "id":"msg_native","type":"message","role":"assistant","model":"claude-opus",
            "content":[
                {"type":"tool_use","id":"shell_response","name":"bash","input":{"command":"pwd"}},
                {"type":"tool_use","id":"patch_response","name":"str_replace_based_edit_tool","input":{"command":"create","path":"new.txt","file_text":"hello"}}
            ],
            "stop_reason":"tool_use","usage":{"input_tokens":2,"output_tokens":1}
        }),
    );
    assert_eq!(outward["output"][0]["type"], "shell_call");
    assert_eq!(outward["output"][1]["type"], "apply_patch_call");
    assert_eq!(outward["output"][0]["call_id"], "shell_response");
    assert_eq!(outward["output"][1]["call_id"], "patch_response");

    let inward = convert_response(
        claude,
        responses,
        json!({
            "id":"resp_native","object":"response","model":"gpt","status":"completed",
            "output":[
                {"type":"shell_call","call_id":"shell_response","action":{"commands":["pwd"]},"status":"completed"},
                {"type":"apply_patch_call","call_id":"patch_response","operation":{"type":"create_file","path":"new.txt","diff":"hello"},"status":"completed"}
            ],
            "usage":{"input_tokens":2,"output_tokens":1,"total_tokens":3,"output_tokens_details":{"reasoning_tokens":0}}
        }),
    );
    assert_eq!(inward["content"][0]["name"], "bash");
    assert_eq!(inward["content"][0]["id"], "shell_response");
    assert_eq!(inward["content"][1]["name"], "str_replace_based_edit_tool");
    assert_eq!(inward["content"][1]["id"], "patch_response");

    let chat_outward = convert_response(
        chat,
        responses,
        json!({
            "id":"resp_chat_native","object":"response","model":"gpt","status":"completed",
            "output":[
                {"type":"shell_call","id":"item_shell","call_id":"call_shell","action":{"commands":["pwd"]},"status":"completed"},
                {"type":"apply_patch_call","id":"item_patch","call_id":"call_patch","operation":{"type":"create_file","path":"new.txt","diff":"hello"},"status":"completed"}
            ]
        }),
    );
    let calls = chat_outward["choices"][0]["message"]["tool_calls"]
        .as_array()
        .unwrap();
    assert_eq!(calls[0]["id"], "call_shell");
    assert_eq!(calls[0]["function"]["name"], "shell");
    assert_eq!(calls[1]["id"], "call_patch");
    assert_eq!(calls[1]["function"]["name"], "apply_patch");
}

#[test]
fn incremental_native_streams_emit_correlated_typed_calls() {
    let chat = content(Operation::StreamGenerateContent, Kind::OpenAiChat);
    let claude = content(Operation::StreamGenerateContent, Kind::ClaudeMessages);
    let responses = content(Operation::StreamGenerateContent, Kind::OpenAiResponses);
    let claude_wire = concat!(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_stream\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-opus\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"stream_shell\",\"name\":\"bash\",\"input\":{}}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"command\\\":\\\"pwd\\\"}\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":1}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
    );
    let frames = data_frames(&drive(
        ResponseStream::new(responses, claude).unwrap(),
        claude_wire,
        23,
    ));
    let added = frames
        .iter()
        .find(|event| event["type"] == "response.output_item.added")
        .unwrap();
    assert_eq!(added["item"]["type"], "shell_call");
    assert_eq!(added["item"]["call_id"], "stream_shell");
    assert!(added["item"].get("id").is_none());
    assert_eq!(added["item"]["action"]["commands"][0], "pwd");

    let responses_wire = concat!(
        "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_stream\",\"object\":\"response\",\"model\":\"gpt\",\"status\":\"in_progress\",\"output\":[]}}\n\n",
        "event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"apply_patch_call\",\"id\":\"item_patch\",\"call_id\":\"stream_patch\",\"operation\":{\"type\":\"create_file\",\"path\":\"new.txt\",\"diff\":\"hello\"},\"status\":\"in_progress\"}}\n\n",
        "event: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"apply_patch_call\",\"id\":\"item_patch\",\"call_id\":\"stream_patch\",\"operation\":{\"type\":\"create_file\",\"path\":\"new.txt\",\"diff\":\"hello\"},\"status\":\"completed\"}}\n\n",
        "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_stream\",\"object\":\"response\",\"model\":\"gpt\",\"status\":\"completed\",\"output\":[{\"type\":\"apply_patch_call\",\"id\":\"item_patch\",\"call_id\":\"stream_patch\",\"operation\":{\"type\":\"create_file\",\"path\":\"new.txt\",\"diff\":\"hello\"},\"status\":\"completed\"}]}}\n\n"
    );
    let chat_frames = data_frames(&drive(
        ResponseStream::new(chat, responses).unwrap(),
        responses_wire,
        31,
    ));
    let chat_start = chat_frames
        .iter()
        .find(|event| event.pointer("/choices/0/delta/tool_calls/0/id").is_some())
        .unwrap();
    assert_eq!(
        chat_start.pointer("/choices/0/delta/tool_calls/0/id"),
        Some(&json!("stream_patch"))
    );
    assert_eq!(
        chat_start.pointer("/choices/0/delta/tool_calls/0/function/name"),
        Some(&json!("apply_patch"))
    );
    let frames = data_frames(&drive(
        ResponseStream::new(claude, responses).unwrap(),
        responses_wire,
        29,
    ));
    let start = frames
        .iter()
        .find(|event| event["type"] == "content_block_start")
        .unwrap();
    assert_eq!(start["content_block"]["id"], "stream_patch");
    assert_eq!(
        start["content_block"]["name"],
        "str_replace_based_edit_tool"
    );
    assert_eq!(start["content_block"]["input"]["command"], "create");
    assert!(frames.iter().any(|event| {
        event["type"] == "message_delta" && event["delta"]["stop_reason"] == "tool_use"
    }));
}
