use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind as Kind, Operation};
use serde_json::json;

use super::support::{data_frames, drive};
use super::{
    BufferedResponse, ResponseCollector, ResponseStream, content, convert_request,
    convert_response, response,
};

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

/// A live Responses stream carries fields Chat Completions has no slot for —
/// `logprobs` and `annotations` ride on every `output_text`, Codex attaches
/// `encrypted_content` to reasoning, and vendors add event types continuously.
/// Refusing any of them used to kill the reply mid-flight, which took the whole
/// Codex channel down.
#[test]
fn responses_stream_survives_fields_and_events_chat_cannot_express() {
    let chat = content(Operation::StreamGenerateContent, Kind::OpenAiChat);
    let responses = content(Operation::StreamGenerateContent, Kind::OpenAiResponses);
    let stream = ResponseStream::new(chat, responses).unwrap();
    let wire = concat!(
        "data: {\"type\":\"response.created\",\"sequence_number\":0,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"in_progress\",\"model\":\"gpt-5.5\",\"output\":[]}}\n\n",
        "data: {\"type\":\"response.queued\",\"sequence_number\":1,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"queued\",\"model\":\"gpt-5.5\",\"output\":[]}}\n\n",
        "data: {\"type\":\"response.something.new\",\"sequence_number\":2}\n\n",
        "data: {\"type\":\"response.output_text.delta\",\"sequence_number\":3,\"item_id\":\"msg_1\",\"output_index\":0,\"content_index\":0,\"delta\":\"Hello\",\"logprobs\":[]}\n\n",
        "data: {\"type\":\"response.output_item.done\",\"sequence_number\":4,\"output_index\":0,\"item\":{\"type\":\"reasoning\",\"id\":\"rs_1\",\"summary\":[],\"encrypted_content\":\"opaque\"}}\n\n",
        "data: {\"type\":\"response.completed\",\"sequence_number\":5,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"completed\",\"model\":\"gpt-5.5\",\"output\":[{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\",\"status\":\"completed\",\"content\":[{\"type\":\"output_text\",\"text\":\"Hello\",\"annotations\":[],\"logprobs\":[]}]}],\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n",
    );
    let text = String::from_utf8(drive(stream, wire, 17)).unwrap();
    assert!(text.contains("Hello"), "text was dropped: {text}");
    assert!(
        text.contains("\"finish_reason\":\"stop\""),
        "no terminal: {text}"
    );
}

#[test]
fn responses_stream_survives_items_and_events_claude_cannot_express() {
    let claude = content(Operation::StreamGenerateContent, Kind::ClaudeMessages);
    let responses = content(Operation::StreamGenerateContent, Kind::OpenAiResponses);
    let stream = ResponseStream::new(claude, responses).unwrap();
    let wire = concat!(
        "data: {\"type\":\"response.created\",\"sequence_number\":0,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"in_progress\",\"model\":\"gpt-5.5\",\"output\":[]}}\n\n",
        "data: {\"type\":\"response.something.new\",\"sequence_number\":1}\n\n",
        "data: {\"type\":\"response.image_generation_call.in_progress\",\"sequence_number\":2,\"item_id\":\"image_1\",\"output_index\":0}\n\n",
        "data: {\"type\":\"response.output_item.added\",\"sequence_number\":3,\"output_index\":0,\"item\":{\"type\":\"image_generation_call\",\"id\":\"image_1\",\"result\":null,\"status\":\"in_progress\"}}\n\n",
        "data: {\"type\":\"response.output_item.done\",\"sequence_number\":4,\"output_index\":0,\"item\":{\"type\":\"image_generation_call\",\"id\":\"image_1\",\"result\":\"aW1hZ2U=\",\"status\":\"completed\"}}\n\n",
        "data: {\"type\":\"response.output_text.delta\",\"sequence_number\":5,\"item_id\":\"msg_1\",\"output_index\":1,\"content_index\":0,\"delta\":\"Done\",\"logprobs\":[]}\n\n",
        "data: {\"type\":\"response.completed\",\"sequence_number\":6,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"completed\",\"model\":\"gpt-5.5\",\"output\":[{\"type\":\"image_generation_call\",\"id\":\"image_1\",\"result\":\"aW1hZ2U=\",\"status\":\"completed\"},{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\",\"status\":\"completed\",\"content\":[{\"type\":\"output_text\",\"text\":\"Done\",\"annotations\":[],\"logprobs\":[]}]}],\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n",
    );
    let text = String::from_utf8(drive(stream, wire, 23)).unwrap();
    assert!(text.contains("Done"), "text was dropped: {text}");
    assert!(text.contains("message_stop"), "no terminal: {text}");
}

#[test]
fn responses_stream_survives_items_and_events_gemini_cannot_express() {
    let gemini = content(
        Operation::StreamGenerateContent,
        Kind::GeminiGenerateContent,
    );
    let responses = content(Operation::StreamGenerateContent, Kind::OpenAiResponses);
    let stream = ResponseStream::new(gemini, responses).unwrap();
    let wire = concat!(
        "data: {\"type\":\"response.created\",\"sequence_number\":0,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"in_progress\",\"model\":\"gpt-5.5\",\"output\":[]}}\n\n",
        "data: {\"type\":\"response.something.new\",\"sequence_number\":1}\n\n",
        "data: {\"type\":\"response.image_generation_call.in_progress\",\"sequence_number\":2,\"item_id\":\"image_1\",\"output_index\":0}\n\n",
        "data: {\"type\":\"response.output_item.added\",\"sequence_number\":3,\"output_index\":0,\"item\":{\"type\":\"image_generation_call\",\"id\":\"image_1\",\"result\":null,\"status\":\"in_progress\"}}\n\n",
        "data: {\"type\":\"response.output_item.done\",\"sequence_number\":4,\"output_index\":0,\"item\":{\"type\":\"image_generation_call\",\"id\":\"image_1\",\"result\":\"aW1hZ2U=\",\"status\":\"completed\"}}\n\n",
        "data: {\"type\":\"response.output_text.delta\",\"sequence_number\":5,\"item_id\":\"msg_1\",\"output_index\":1,\"content_index\":0,\"delta\":\"Done\",\"logprobs\":[]}\n\n",
        "data: {\"type\":\"response.completed\",\"sequence_number\":6,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"created_at\":0,\"status\":\"completed\",\"model\":\"gpt-5.5\",\"output\":[{\"type\":\"image_generation_call\",\"id\":\"image_1\",\"result\":\"aW1hZ2U=\",\"status\":\"completed\"},{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\",\"status\":\"completed\",\"content\":[{\"type\":\"output_text\",\"text\":\"Done\",\"annotations\":[],\"logprobs\":[]}]}],\"usage\":{\"input_tokens\":1,\"output_tokens\":1,\"total_tokens\":2}}}\n\n",
    );
    let text = String::from_utf8(drive(stream, wire, 29)).unwrap();
    assert!(text.contains("Done"), "text was dropped: {text}");
    assert!(text.contains("STOP"), "no terminal: {text}");
}

#[test]
fn claude_stream_survives_blocks_and_events_gemini_cannot_express() {
    let gemini = content(
        Operation::StreamGenerateContent,
        Kind::GeminiGenerateContent,
    );
    let claude = content(Operation::StreamGenerateContent, Kind::ClaudeMessages);
    let stream = ResponseStream::new(gemini, claude).unwrap();
    let wire = concat!(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-opus\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"redacted_thinking\",\"data\":\"opaque\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\",\"citations\":[]}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"citations_delta\",\"citation\":{\"type\":\"char_location\",\"cited_text\":\"Hello\",\"document_index\":0,\"start_char_index\":0,\"end_char_index\":5}}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"future_delta\",\"future\":true}}\n\n",
        "event: future_event\ndata: {\"type\":\"future_event\",\"future\":true}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"tool_use\",\"id\":\"call_1\",\"name\":\"lookup\",\"input\":{},\"caller\":{\"type\":\"direct\"}}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":2}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":1}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    );
    let text = String::from_utf8(drive(stream, wire, 31)).unwrap();
    assert!(text.contains("Hello"), "text was dropped: {text}");
    assert!(text.contains("STOP"), "no terminal: {text}");
}

#[test]
fn gemini_response_uses_first_candidate_and_ignores_claude_unmapped_fields() {
    let body = response(
        content(Operation::GenerateContent, Kind::ClaudeMessages),
        content(Operation::GenerateContent, Kind::GeminiGenerateContent),
        Bytes::from_static(
            br#"{"responseId":"gemini_1","modelVersion":"gemini-3-flash","candidates":[{"index":0,"finishReason":"STOP","content":{"role":"model","future_content":true,"parts":[{"text":"first","thought":false,"thoughtSignature":"opaque","partMetadata":{"state":"live"}}]}},{"index":1,"finishReason":"STOP","content":{"role":"model","parts":[{"text":"second"}]}}],"usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1,"totalTokenCount":2}}"#,
        ),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["content"][0]["text"], "first");
    assert!(
        !body
            .windows("second".len())
            .any(|window| window == b"second")
    );

    let stream = ResponseStream::new(
        content(Operation::StreamGenerateContent, Kind::ClaudeMessages),
        content(
            Operation::StreamGenerateContent,
            Kind::GeminiGenerateContent,
        ),
    )
    .unwrap();
    let wire = "data: {\"responseId\":\"gemini_1\",\"modelVersion\":\"gemini-3-flash\",\"candidates\":[{\"index\":0,\"finishReason\":\"STOP\",\"content\":{\"role\":\"model\",\"parts\":[{\"text\":\"first\",\"thought\":false,\"thoughtSignature\":\"opaque\",\"partMetadata\":{\"state\":\"live\"}}]}}],\"usageMetadata\":{\"promptTokenCount\":1,\"candidatesTokenCount\":1,\"totalTokenCount\":2}}\n\n";
    let text = String::from_utf8(drive(stream, wire, 17)).unwrap();
    assert!(text.contains("first"), "stream text was dropped: {text}");
    assert!(text.contains("message_stop"), "no stream terminal: {text}");
}

#[test]
fn gemini_response_skips_parts_chat_cannot_render() {
    let body = Bytes::from_static(
        br#"{"responseId":"gemini_1","modelVersion":"gemini-3-flash","candidates":[{"index":0,"finishReason":"STOP","content":{"role":"user","parts":[{"text":"visible"},{"inlineData":{"mimeType":"audio/wav","data":"UklGRg=="}},{"functionResponse":{"id":"call_1","name":"lookup","response":{"output":"done"}}}]}}],"usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1,"totalTokenCount":2}}"#,
    );
    let output = response(
        content(Operation::GenerateContent, Kind::OpenAiChat),
        content(Operation::GenerateContent, Kind::GeminiGenerateContent),
        body.clone(),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["choices"][0]["message"]["content"], "visible");

    let stream = ResponseStream::new(
        content(Operation::StreamGenerateContent, Kind::OpenAiChat),
        content(
            Operation::StreamGenerateContent,
            Kind::GeminiGenerateContent,
        ),
    )
    .unwrap();
    let wire = format!("data: {}\n\n", String::from_utf8(body.to_vec()).unwrap());
    let text = String::from_utf8(drive(stream, &wire, 19)).unwrap();
    assert!(text.contains("visible"), "stream text was dropped: {text}");
    assert!(text.contains("[DONE]"), "no stream terminal: {text}");
}

#[test]
fn chat_response_skips_audio_and_keeps_unparseable_call_for_gemini() {
    let output = response(
        content(Operation::GenerateContent, Kind::GeminiGenerateContent),
        content(Operation::GenerateContent, Kind::OpenAiChat),
        Bytes::from_static(
            br#"{"id":"chat_1","object":"chat.completion","created":0,"model":"gpt-5.5","choices":[{"index":0,"finish_reason":"tool_calls","message":{"role":"assistant","content":"visible","audio":{"id":"audio_1","data":"UklGRg==","expires_at":0,"transcript":"spoken"},"tool_calls":[{"id":"call_1","type":"function","function":{"name":"lookup","arguments":"{\"q\":"}}]}}]}"#,
        ),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        value["candidates"][0]["content"]["parts"][0]["text"],
        "visible"
    );
    assert_eq!(
        value["candidates"][0]["content"]["parts"][1]["functionCall"]["name"],
        "lookup"
    );
    assert!(
        value["candidates"][0]["content"]["parts"][1]["functionCall"]
            .get("args")
            .is_none()
    );

    let stream = ResponseStream::new(
        content(
            Operation::StreamGenerateContent,
            Kind::GeminiGenerateContent,
        ),
        content(Operation::StreamGenerateContent, Kind::OpenAiChat),
    )
    .unwrap();
    let wire = concat!(
        "data: {\"id\":\"chat_1\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"gpt-5.5\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{\\\"q\\\":\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"chat_1\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"gpt-5.5\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: [DONE]\n\n",
    );
    let text = String::from_utf8(drive(stream, wire, 23)).unwrap();
    assert!(text.contains("lookup"), "tool call was dropped: {text}");
}

#[test]
fn claude_stream_survives_unknown_events_for_chat() {
    let stream = ResponseStream::new(
        content(Operation::StreamGenerateContent, Kind::OpenAiChat),
        content(Operation::StreamGenerateContent, Kind::ClaudeMessages),
    )
    .unwrap();
    let wire = concat!(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-opus\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\n",
        "event: future_event\ndata: {\"type\":\"future_event\",\"future\":true}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"redacted_thinking\",\"data\":\"opaque\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"citations_delta\",\"citation\":{\"type\":\"char_location\",\"cited_text\":\"Hello\",\"document_index\":0,\"start_char_index\":0,\"end_char_index\":5}}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"future_delta\",\"future\":true}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":1}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    );
    let text = String::from_utf8(drive(stream, wire, 17)).unwrap();
    assert!(text.contains("Hello"), "text was dropped: {text}");
    assert!(text.contains("[DONE]"), "no terminal: {text}");
}

#[test]
fn chat_response_keeps_unparseable_tool_call_for_claude() {
    let output = response(
        content(Operation::GenerateContent, Kind::ClaudeMessages),
        content(Operation::GenerateContent, Kind::OpenAiChat),
        Bytes::from_static(
            br#"{"id":"chat_1","object":"chat.completion","created":0,"model":"gpt-5.5","choices":[{"index":0,"finish_reason":"tool_calls","message":{"role":"assistant","tool_calls":[{"id":"call_1","type":"function","function":{"name":"lookup","arguments":"{\"q\":"}}]}}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#,
        ),
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(value["content"][0]["type"], "tool_use");
    assert_eq!(value["content"][0]["name"], "lookup");
    assert_eq!(value["content"][0]["input"], json!({}));
}

#[test]
fn collectors_ignore_unknown_events_that_v2_ignored() {
    let mut responses = ResponseCollector::new(Kind::OpenAiResponses).unwrap();
    responses
        .push(Bytes::from_static(
            b"data: {\"type\":\"response.something.new\",\"sequence_number\":0}\n\ndata: {\"type\":\"response.completed\",\"sequence_number\":1,\"response\":{\"id\":\"resp_1\",\"object\":\"response\",\"status\":\"completed\",\"model\":\"gpt-5.5\",\"output\":[]}}\n\n",
        ))
        .unwrap();
    assert!(responses.is_complete());
    assert!(matches!(
        responses.finish().unwrap(),
        BufferedResponse::OpenAiResponses(_)
    ));

    let mut claude = ResponseCollector::new(Kind::ClaudeMessages).unwrap();
    claude
        .push(Bytes::from_static(
            b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-opus\",\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"future_delta\",\"future\":true}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\nevent: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":1}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        ))
        .unwrap();
    assert!(claude.is_complete());
    let BufferedResponse::Claude(message) = claude.finish().unwrap() else {
        panic!("wrong buffered family");
    };
    assert_eq!(
        serde_json::to_value(message).unwrap()["content"][0]["text"],
        "Hello"
    );
}
