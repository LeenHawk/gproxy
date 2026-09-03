use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind as Kind, Operation, WireFamily};
use serde_json::json;

use super::super::{content, convert_response, family, request};

#[test]
fn responses_to_claude_defaults_and_wraps_lossy_blocks() {
    let converted = convert_response(
        content(Operation::GenerateContent, Kind::ClaudeMessages),
        content(Operation::GenerateContent, Kind::OpenAiResponses),
        json!({
            "id":"resp_defaults","object":"response","status":"completed",
            "output":[
                {"type":"reasoning","id":"rs_1","summary":[],
                 "content":[{"type":"reasoning_text","text":"visible"}],
                 "status":"completed"},
                {"type":"function_call","id":"fc_1","call_id":"call_1",
                 "name":"broken","arguments":"not-json","status":"completed"},
                {"type":"custom_tool_call","id":"ctc_1","call_id":"call_2",
                 "name":"raw","input":"plain text"}
            ]
        }),
    );
    assert_eq!(converted["model"], "unknown");
    assert_eq!(converted["usage"]["input_tokens"], 0);
    assert_eq!(converted["usage"]["output_tokens"], 0);
    assert_eq!(converted["content"][0]["type"], "text");
    assert_eq!(converted["content"][0]["text"], "visible");
    assert_eq!(converted["content"][1]["input"], json!({}));
    assert_eq!(
        converted["content"][2]["input"],
        json!({"input":"plain text"})
    );
}

#[test]
fn claude_to_responses_collects_text_after_non_text_and_maps_refusal_tier() {
    let converted = convert_response(
        content(Operation::GenerateContent, Kind::OpenAiResponses),
        content(Operation::GenerateContent, Kind::ClaudeMessages),
        json!({
            "id":"msg_order","type":"message","role":"assistant","model":"claude",
            "content":[
                {"type":"text","text":"before"},
                {"type":"thinking","thinking":"plan","signature":"opaque"},
                {"type":"text","text":"after"},
                {"type":"tool_use","id":"toolu_1","name":"lookup","input":{}}
            ],
            "stop_reason":"refusal","stop_sequence":null,
            "usage":{"input_tokens":2,"output_tokens":3,"service_tier":"priority"}
        }),
    );
    assert_eq!(converted["output"][0]["type"], "reasoning");
    assert_eq!(converted["output"][1]["type"], "function_call");
    assert_eq!(converted["output"][2]["type"], "message");
    assert_eq!(converted["output"][2]["content"][0]["text"], "before");
    assert_eq!(converted["output"][2]["content"][1]["text"], "after");
    assert_eq!(converted["incomplete_details"]["reason"], "content_filter");
    assert_eq!(converted["service_tier"], "priority");
}

#[test]
fn empty_claude_thinking_does_not_create_forbidden_reasoning_content() {
    let converted = request(
        content(Operation::GenerateContent, Kind::ClaudeMessages),
        content(Operation::GenerateContent, Kind::OpenAiResponses),
        Bytes::from(
            serde_json::to_vec(&json!({
                "model":"route","max_tokens":32,
                "messages":[{"role":"assistant","content":[{
                    "type":"thinking","thinking":"","signature":"opaque"
                }]}]
            }))
            .unwrap(),
        ),
        "upstream-model",
        false,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&converted).unwrap();
    assert!(value["input"][0].get("content").is_none());
    assert_eq!(value["input"][0]["encrypted_content"], "opaque");
}

#[test]
fn gemini_sdk_system_instruction_role_is_accepted_as_system_context() {
    let converted = request(
        content(Operation::GenerateContent, Kind::GeminiGenerateContent),
        content(Operation::GenerateContent, Kind::ClaudeMessages),
        Bytes::from_static(
            br#"{"systemInstruction":{"role":"user","parts":[{"text":"system policy"}]},"contents":[{"role":"user","parts":[{"text":"hello"}]}],"generationConfig":{"topK":40,"thinkingConfig":{"includeThoughts":true}}}"#,
        ),
        "upstream-model",
        false,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&converted).unwrap();
    assert_eq!(value["system"][0]["text"], "system policy");
    assert_eq!(value["messages"][0]["content"][0]["text"], "hello");
    assert!(value.get("top_k").is_none());
    assert_eq!(value["thinking"]["type"], "enabled");
    assert_eq!(value["thinking"]["budget_tokens"], 4096);
}

#[test]
fn responses_custom_tool_does_not_leak_native_metadata_to_claude() {
    let converted = request(
        content(Operation::GenerateContent, Kind::OpenAiResponses),
        content(Operation::GenerateContent, Kind::ClaudeMessages),
        Bytes::from_static(
            br#"{"model":"route","input":[{"type":"message","role":"system","content":[{"type":"input_text","text":"system policy"}]},{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}],"tools":[{"type":"custom","name":"raw","description":"raw input"},{"type":"web_search_preview","external_web_access":true,"allowed_callers":["direct","programmatic"]}]}"#,
        ),
        "upstream-model",
        false,
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&converted).unwrap();
    assert_eq!(value["tools"][0]["name"], "raw");
    assert_eq!(value["system"], "system policy");
    assert_eq!(value["messages"][0]["role"], "user");
    assert!(value["tools"][1].get("allowed_callers").is_none());
    assert!(!value.to_string().contains("openai_native_tool"));
    assert!(!value.to_string().contains("external_web_access"));
}

#[test]
fn gemini_candidates_become_ordered_responses_messages() {
    let converted = convert_response(
        content(Operation::GenerateContent, Kind::OpenAiResponses),
        content(Operation::GenerateContent, Kind::GeminiGenerateContent),
        json!({
            "modelVersion":"gemini",
            "candidates":[
                {"index":0,"content":{"role":"model","parts":[{"text":"first"}]},"finishReason":"STOP"},
                {"index":1,"content":{"role":"model","parts":[{"text":"second"}]},"finishReason":"STOP"}
            ]
        }),
    );
    assert_eq!(converted["id"], "");
    assert_eq!(converted["created_at"], 0);
    assert_eq!(converted["completed_at"], 0);
    assert_eq!(converted["output"][0]["content"][0]["text"], "first");
    assert_eq!(converted["output"][1]["content"][0]["text"], "second");
    assert_eq!(converted["output_text"], "first");
}

#[test]
fn responses_terminal_defaults_do_not_reject_convertible_gemini_replies() {
    let gemini = content(Operation::GenerateContent, Kind::GeminiGenerateContent);
    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let missing = convert_response(
        gemini,
        responses,
        json!({"id":"missing","object":"response","status":"incomplete","output":[]}),
    );
    assert_eq!(missing["candidates"][0]["finishReason"], "MAX_TOKENS");

    let extra = convert_response(
        gemini,
        responses,
        json!({
            "id":"extra","object":"response","status":"completed","output":[],
            "incomplete_details":{"reason":"max_output_tokens"}
        }),
    );
    assert_eq!(extra["candidates"][0]["finishReason"], "STOP");
}

#[test]
fn claude_blocks_join_and_native_calls_reach_buffered_chat() {
    let converted = convert_response(
        content(Operation::GenerateContent, Kind::OpenAiChat),
        content(Operation::GenerateContent, Kind::ClaudeMessages),
        json!({
            "id":"msg_chat","type":"message","role":"assistant","model":"claude",
            "content":[
                {"type":"text","text":"before"},
                {"type":"thinking","thinking":"plan","signature":"opaque"},
                {"type":"text","text":"after"},
                {"type":"server_tool_use","id":"srv_1","name":"web_search","input":{"query":"x"}},
                {"type":"mcp_tool_use","id":"mcp_1","server_name":"remote","name":"lookup","input":{}}
            ],
            "stop_reason":"tool_use","stop_sequence":null,
            "usage":{"input_tokens":1,"output_tokens":2,"service_tier":"priority"}
        }),
    );
    let message = &converted["choices"][0]["message"];
    assert_eq!(message["content"], "before\nplan\nafter");
    assert!(message.get("reasoning_content").is_none());
    assert_eq!(message["tool_calls"][0]["type"], "custom");
    assert_eq!(message["tool_calls"][0]["custom"]["name"], "web_search");
    assert_eq!(
        message["tool_calls"][1]["custom"]["name"],
        "mcp:remote:lookup"
    );
    assert_eq!(converted["service_tier"], "priority");
    assert_eq!(converted["created"], 0);
}

#[test]
fn chat_refusal_becomes_claude_text_and_refusal_status() {
    let converted = convert_response(
        content(Operation::GenerateContent, Kind::ClaudeMessages),
        content(Operation::GenerateContent, Kind::OpenAiChat),
        json!({
            "id":"chat_refusal","object":"chat.completion","model":"gpt",
            "choices":[{
                "index":0,"finish_reason":"stop",
                "message":{"role":"assistant","refusal":"blocked"}
            }]
        }),
    );
    assert_eq!(converted["content"][0]["type"], "text");
    assert_eq!(converted["content"][0]["text"], "blocked");
    assert_eq!(converted["stop_reason"], "refusal");
}

#[test]
fn claude_and_gemini_buffered_replies_skip_unrenderable_blocks() {
    let claude = content(Operation::GenerateContent, Kind::ClaudeMessages);
    let gemini = content(Operation::GenerateContent, Kind::GeminiGenerateContent);
    let to_gemini = convert_response(
        gemini,
        claude,
        json!({
            "id":"msg_skip","type":"message","role":"assistant","model":"claude",
            "content":[
                {"type":"text","text":"kept"},
                {"type":"mcp_tool_use","id":"mcp_1","server_name":"remote","name":"lookup","input":{}}
            ],
            "stop_reason":"end_turn","stop_sequence":null,
            "usage":{"input_tokens":1,"output_tokens":1}
        }),
    );
    assert_eq!(
        to_gemini["candidates"][0]["content"]["parts"][0]["text"],
        "kept"
    );

    let to_claude = convert_response(
        claude,
        gemini,
        json!({
            "responseId":"gemini_skip","modelVersion":"gemini",
            "candidates":[{
                "content":{"role":"model","parts":[
                    {"text":"kept"},
                    {"functionResponse":{"name":"lookup","response":{"ok":true}}}
                ]},
                "finishReason":"STOP"
            }]
        }),
    );
    assert_eq!(to_claude["content"][0]["text"], "kept");
}

#[test]
fn gemini_to_chat_supplies_v2_defaults_and_usage_counts() {
    let converted = convert_response(
        content(Operation::GenerateContent, Kind::OpenAiChat),
        content(Operation::GenerateContent, Kind::GeminiGenerateContent),
        json!({
            "candidates":[{}],
            "usageMetadata":{
                "promptTokenCount":10,"candidatesTokenCount":5,
                "thoughtsTokenCount":7,"totalTokenCount":22
            }
        }),
    );
    assert_eq!(converted["id"], "");
    assert_eq!(converted["model"], "unknown");
    assert_eq!(converted["created"], 0);
    assert_eq!(converted["choices"][0]["finish_reason"], "stop");
    assert_eq!(converted["choices"][0]["message"]["content"], "");
    assert_eq!(converted["usage"]["completion_tokens"], 5);
    assert_eq!(
        converted["usage"]["completion_tokens_details"]["reasoning_tokens"],
        7
    );
}

#[test]
fn chat_to_gemini_uses_saturating_v2_usage_projection() {
    let converted = convert_response(
        content(Operation::GenerateContent, Kind::GeminiGenerateContent),
        content(Operation::GenerateContent, Kind::OpenAiChat),
        json!({
            "id":"chat_usage","object":"chat.completion","model":"gpt",
            "choices":[{
                "index":0,"finish_reason":"stop",
                "message":{"role":"assistant","content":"ok"}
            }],
            "usage":{
                "prompt_tokens":10,"completion_tokens":7,"total_tokens":99,
                "completion_tokens_details":{"reasoning_tokens":9}
            }
        }),
    );
    assert_eq!(converted["usageMetadata"]["promptTokenCount"], 10);
    assert_eq!(converted["usageMetadata"]["candidatesTokenCount"], 7);
    assert_eq!(converted["usageMetadata"]["thoughtsTokenCount"], 9);
    assert_eq!(converted["usageMetadata"]["totalTokenCount"], 99);
}

#[test]
fn compact_response_puts_message_first_and_marks_incomplete() {
    let converted = convert_response(
        family(Operation::CompactContent, WireFamily::OpenAi),
        content(Operation::GenerateContent, Kind::ClaudeMessages),
        json!({
            "id":"compact_order","type":"message","role":"assistant","model":"claude",
            "content":[
                {"type":"thinking","thinking":"plan","signature":"opaque"},
                {"type":"text","text":"summary"},
                {"type":"tool_use","id":"toolu_1","name":"lookup","input":{}}
            ],
            "stop_reason":"refusal","stop_sequence":null,
            "usage":{}
        }),
    );
    assert_eq!(converted["output"][0]["type"], "message");
    assert_eq!(converted["output"][0]["status"], "incomplete");
    assert_eq!(converted["output"][1]["type"], "reasoning");
    assert_eq!(converted["output"][2]["type"], "function_call");
    assert_eq!(converted["usage"]["input_tokens"], 0);
    assert_eq!(converted["usage"]["output_tokens"], 0);
}

#[test]
fn model_transforms_restore_compatibility_defaults_and_ignore_bodies() {
    let openai = family(Operation::ListModels, WireFamily::OpenAi);
    let claude = family(Operation::ListModels, WireFamily::Claude);
    let to_claude = convert_response(
        claude,
        openai,
        json!({"data":[{"id":"model-a","object":"model"}],"object":"list"}),
    );
    assert_eq!(to_claude["data"][0]["created_at"], "1970-01-01T00:00:00Z");
    assert_eq!(to_claude["data"][0]["display_name"], "model-a");

    let to_openai = convert_response(
        openai,
        claude,
        json!({
            "data":[{"id":"model-b","type":"model","display_name":"Model B"}],
            "has_more":false
        }),
    );
    assert_eq!(to_openai["data"][0]["owned_by"], "unknown");

    assert_eq!(
        request(openai, claude, Bytes::from_static(b"{}"), "unused", false).unwrap(),
        Bytes::new()
    );
}
