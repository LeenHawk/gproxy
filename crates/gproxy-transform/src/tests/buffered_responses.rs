use gproxy_protocol::{ContentGenerationKind as Kind, Operation};
use serde_json::json;

use super::{content, convert_response};

#[test]
fn defaults_survive_empty_provider_replies() {
    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let chat = content(Operation::GenerateContent, Kind::OpenAiChat);
    let claude = content(Operation::GenerateContent, Kind::ClaudeMessages);
    let gemini = content(Operation::GenerateContent, Kind::GeminiGenerateContent);

    let empty_responses = convert_response(
        responses,
        chat,
        json!({
            "id":"chat_empty","object":"chat.completion","created":42,
            "model":"gpt","choices":[]
        }),
    );
    assert_eq!(empty_responses["status"], "completed");
    assert_eq!(empty_responses["completed_at"], 42);
    assert_eq!(empty_responses["output"], json!([]));

    let incomplete = convert_response(
        responses,
        chat,
        json!({
            "id":"chat_length","object":"chat.completion","created":43,
            "model":"gpt","choices":[{
                "index":0,"finish_reason":"length",
                "message":{"role":"assistant","content":"partial"}
            }]
        }),
    );
    assert_eq!(incomplete["status"], "incomplete");
    assert_eq!(incomplete["completed_at"], 43);
    assert_eq!(
        incomplete["incomplete_details"]["reason"],
        "max_output_tokens"
    );
    assert_eq!(incomplete["output_text"], "partial");

    let filtered = convert_response(
        responses,
        chat,
        json!({
            "id":"chat_filtered","object":"chat.completion","model":"gpt",
            "choices":[{
                "index":0,"finish_reason":"content_filter",
                "message":{"role":"assistant","tool_calls":[{
                    "id":"vendor id","type":"function",
                    "function":{"name":"lookup","arguments":"{}"}
                }]}
            }]
        }),
    );
    assert_eq!(filtered["status"], "incomplete");
    assert_eq!(filtered["incomplete_details"]["reason"], "content_filter");
    let call = &filtered["output"][0];
    assert!(call["call_id"].as_str().unwrap().starts_with("call_"));
    assert!(call["id"].as_str().unwrap().starts_with("fc_"));
    assert_ne!(call["call_id"], "vendor id");

    let empty_claude = convert_response(
        claude,
        chat,
        json!({
            "id":"chat_claude_empty","object":"chat.completion",
            "model":"gpt","choices":[]
        }),
    );
    assert_eq!(empty_claude["content"][0]["type"], "text");
    assert_eq!(empty_claude["content"][0]["text"], "");
    assert_eq!(empty_claude["stop_reason"], "end_turn");
    assert_eq!(empty_claude["usage"]["input_tokens"], 0);
    assert_eq!(empty_claude["usage"]["output_tokens"], 0);

    let empty_gemini_for_claude = convert_response(claude, gemini, json!({"candidates":[]}));
    assert_eq!(empty_gemini_for_claude["id"], "");
    assert_eq!(empty_gemini_for_claude["model"], "");
    assert_eq!(empty_gemini_for_claude["content"][0]["text"], "");
    assert_eq!(empty_gemini_for_claude["stop_reason"], "end_turn");
    assert_eq!(empty_gemini_for_claude["usage"]["input_tokens"], 0);
    assert_eq!(empty_gemini_for_claude["usage"]["output_tokens"], 0);
}
