use gproxy_protocol::{ContentGenerationKind as Kind, Operation};
use serde_json::json;

use super::{content, convert_request, convert_response};

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

#[test]
fn responses_text_annotations_and_fallback_reach_chat() {
    let chat = content(Operation::GenerateContent, Kind::OpenAiChat);
    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let rich = convert_response(
        chat,
        responses,
        json!({
            "id":"resp_rich","object":"response","model":"gpt","status":"completed",
            "output":[
                {"type":"message","id":"msg_1","role":"assistant","status":"completed",
                 "content":[{"type":"output_text","text":"one","annotations":[{
                    "type":"url_citation","start_index":0,"end_index":3,
                    "title":"source","url":"https://example.invalid"
                 }]}]},
                {"type":"message","id":"msg_2","role":"assistant","status":"completed",
                 "content":[{"type":"output_text","text":"two","annotations":[]}]}
            ]
        }),
    );
    let message = &rich["choices"][0]["message"];
    assert_eq!(message["content"], "one\ntwo");
    assert_eq!(message["annotations"][0]["type"], "url_citation");
    assert_eq!(
        message["annotations"][0]["url_citation"]["url"],
        "https://example.invalid"
    );

    let fallback = convert_response(
        chat,
        responses,
        json!({
            "id":"resp_fallback","object":"response","model":"gpt",
            "status":"completed","output":[],"output_text":"fallback"
        }),
    );
    assert_eq!(fallback["choices"][0]["message"]["content"], "fallback");
}

#[test]
fn nullable_chat_logprobs_reach_responses() {
    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let chat = content(Operation::GenerateContent, Kind::OpenAiChat);
    let converted = convert_response(
        responses,
        chat,
        json!({
            "id":"chat_nullable_logprobs","object":"chat.completion","created":42,
            "model":"gpt","choices":[{
                "index":0,"finish_reason":"stop",
                "logprobs":{"content":null,"refusal":null},
                "message":{"role":"assistant","content":"ok"}
            }]
        }),
    );
    assert_eq!(converted["output_text"], "ok");
}

#[test]
fn responses_history_keeps_media_reasoning_and_patch_calls_in_chat() {
    let chat = content(Operation::GenerateContent, Kind::OpenAiChat);
    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let request = convert_request(
        responses,
        chat,
        json!({
            "model":"route",
            "input":[
                {"type":"message","role":"user","content":[
                    {"type":"input_image","image_url":"https://example.invalid/image.png","detail":"high"},
                    {"type":"input_file","file_url":"https://example.invalid/report.pdf"}
                ]},
                {"type":"reasoning","id":"rs_1",
                 "summary":[{"type":"summary_text","text":"plan"}],
                 "content":[{"type":"reasoning_text","text":"details"}]},
                {"type":"message","id":"msg_1","role":"assistant","status":"completed",
                 "content":[{"type":"output_text","text":"answer","annotations":[]}]},
                {"type":"apply_patch_call","id":"item_patch","call_id":"call_patch",
                 "operation":{"type":"create_file","path":"new.txt","diff":"hello"},
                 "status":"completed"}
            ]
        }),
    );
    assert_eq!(
        request["messages"][0]["content"][0]["image_url"]["detail"],
        "high"
    );
    assert_eq!(request["messages"][0]["content"][1]["type"], "text");
    assert_eq!(
        request["messages"][0]["content"][1]["text"],
        "Attachment URL: https://example.invalid/report.pdf"
    );
    assert_eq!(request["messages"][1]["content"][0]["text"], "answer");
    assert_eq!(request["messages"][1]["reasoning_content"], "plan\ndetails");
    assert_eq!(request["messages"][2]["tool_calls"][0]["id"], "call_patch");
    assert_eq!(
        request["messages"][2]["tool_calls"][0]["function"]["name"],
        "apply_patch"
    );
}

#[test]
fn request_fields_map_or_drop_without_blocking_convertible_turns() {
    let chat = content(Operation::GenerateContent, Kind::OpenAiChat);
    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let claude = content(Operation::GenerateContent, Kind::ClaudeMessages);
    let gemini = content(Operation::GenerateContent, Kind::GeminiGenerateContent);

    for (source, value) in [
        (
            chat,
            json!({
                "model":"route","messages":[{"role":"user","content":"hi"}],
                "n":2,"seed":7,"prediction":{"type":"content","content":"expected"},
                "web_search_options":{"search_context_size":"high"}
            }),
        ),
        (responses, json!({"model":"route","input":"hi"})),
        (
            gemini,
            json!({
                "cachedContent":"cachedContents/stable",
                "safetySettings":[{"category":"HARM_CATEGORY_HATE_SPEECH","threshold":"BLOCK_NONE"}],
                "generationConfig":{"candidateCount":2,"seed":7,"responseModalities":["TEXT"]},
                "contents":[{"role":"user","parts":[{"text":"hi"}]}]
            }),
        ),
    ] {
        let request = convert_request(source, claude, value);
        assert_eq!(request["max_tokens"], 16_384);
    }

    let responses_request = convert_request(
        chat,
        responses,
        json!({
            "model":"route","messages":[{"role":"user","content":"hi"}],
            "stop":["END"],"n":2,"seed":7,
            "function_call":"auto",
            "functions":[{"name":"legacy","parameters":{"type":"object"}}],
            "prediction":{"type":"content","content":"expected"},
            "web_search_options":{"search_context_size":"high"}
        }),
    );
    assert_eq!(responses_request["input"][0]["role"], "user");
    for dropped in ["stop", "n", "seed", "functions", "prediction"] {
        assert!(responses_request.get(dropped).is_none(), "kept {dropped}");
    }

    let gemini_request = convert_request(
        chat,
        gemini,
        json!({
            "model":"route","messages":[{"role":"user","content":"hi"}],
            "prompt_cache_key":"cachedContents/stable","metadata":{"trace":"x"},
            "prediction":{"type":"content","content":"expected"},
            "web_search_options":{
                "search_context_size":"high",
                "user_location":{"type":"approximate","approximate":{"country":"US"}}
            }
        }),
    );
    assert_eq!(gemini_request["cachedContent"], "cachedContents/stable");
    assert!(gemini_request["tools"][0].get("googleSearch").is_some());

    let cached_chat = convert_request(
        gemini,
        chat,
        json!({
            "cachedContent":"cachedContents/stable",
            "safetySettings":[{"category":"HARM_CATEGORY_HATE_SPEECH","threshold":"BLOCK_NONE"}],
            "contents":[{"role":"user","parts":[{"text":"hi"}]}]
        }),
    );
    assert_eq!(cached_chat["prompt_cache_key"], "cachedContents/stable");
}

#[test]
fn chat_history_uses_output_messages_and_normalized_tool_correlation() {
    let chat = content(Operation::GenerateContent, Kind::OpenAiChat);
    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let request = convert_request(
        chat,
        responses,
        json!({
            "model":"route",
            "messages":[
                {"role":"assistant","content":"working","tool_calls":[{
                    "id":"vendor-id","type":"function",
                    "function":{"name":"lookup","arguments":"{}"}
                }]},
                {"role":"tool","tool_call_id":"vendor-id","content":"done"},
                {"role":"assistant","content":[{
                    "type":"text","text":"cached",
                    "prompt_cache_breakpoint":{"mode":"explicit"}
                }]}
            ]
        }),
    );
    assert_eq!(request["input"][0]["id"], "msg_0");
    assert_eq!(request["input"][0]["status"], "completed");
    assert_eq!(request["input"][0]["content"][0]["type"], "output_text");
    let call_id = request["input"][1]["call_id"].as_str().unwrap();
    assert!(call_id.starts_with("call_"));
    assert_ne!(call_id, "vendor-id");
    assert!(
        request["input"][1]["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("fc_"))
    );
    assert_eq!(request["input"][2]["call_id"], call_id);
    assert!(request["input"][3].get("id").is_none());
}

#[test]
fn gemini_missing_call_ids_use_v2_name_fallbacks_and_correlate_results() {
    let gemini = content(Operation::GenerateContent, Kind::GeminiGenerateContent);
    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let request = convert_request(
        gemini,
        responses,
        json!({
            "model":"gemini",
            "contents":[
                {"role":"model","parts":[
                    {"functionCall":{"name":"alpha","args":{}}},
                    {"functionCall":{"name":"beta","args":{}}}
                ]},
                {"role":"user","parts":[
                    {"functionResponse":{"name":"alpha","response":{"ok":1}}},
                    {"functionResponse":{"name":"beta","response":{"ok":2}}}
                ]}
            ]
        }),
    );
    assert_eq!(request["input"][0]["call_id"], "call_alpha");
    assert_eq!(request["input"][1]["call_id"], "call_beta");
    assert_eq!(request["input"][2]["call_id"], "call_alpha");
    assert_eq!(request["input"][3]["call_id"], "call_beta");
}
