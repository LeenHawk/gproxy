use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind as Kind, Operation, WireFamily};
use serde_json::{Value, json};

use super::{content, convert_request, family, request};

#[test]
fn claude_assistant_history_uses_responses_output_parts() {
    let image = json!({"type":"image","source":{"type":"url","url":"https://image.invalid/a.png"}});
    for assistant_content in [
        json!("previous answer"),
        json!([
            {"type":"text","text":"previous answer","cache_control":{"type":"ephemeral"}},
            image.clone(),
            {"type":"tool_use","id":"call_1","name":"lookup","input":{}}
        ]),
    ] {
        for stream in [false, true] {
            let converted: Value = serde_json::from_slice(
                &request(
                    content(Operation::GenerateContent, Kind::ClaudeMessages),
                    content(Operation::GenerateContent, Kind::OpenAiResponses),
                    Bytes::from(
                        serde_json::to_vec(&json!({
                            "model":"route","max_tokens":32,"system":"policy",
                            "messages":[
                                {"role":"user","content":[{"type":"text","text":"hello"},image]},
                                {"role":"assistant","content":assistant_content},
                                {"role":"user","content":"continue"}
                            ]
                        }))
                        .unwrap(),
                    ),
                    "upstream-model",
                    stream,
                )
                .unwrap(),
            )
            .unwrap();
            let items = converted["input"].as_array().unwrap();
            assert_eq!(items[0]["content"][0]["type"], "input_text");
            assert_eq!(items[1]["content"][0]["type"], "input_text");
            assert_eq!(items[1]["content"][1]["type"], "input_image");
            assert_eq!(items[2]["role"], "assistant");
            assert_eq!(
                items[2]["content"],
                json!([
                    {"type":"output_text","text":"previous answer","annotations":[]}
                ])
            );
            assert_eq!(items.last().unwrap()["content"][0]["type"], "input_text");
            assert_eq!(converted["stream"], stream);
            if assistant_content.is_array() {
                assert_eq!(items[3]["type"], "function_call");
                assert_eq!(items[3]["call_id"], "call_1");
            }
        }
    }
}

#[test]
fn responses_only_options_do_not_block_pairwise_requests() {
    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    for target in [
        Kind::OpenAiChat,
        Kind::ClaudeMessages,
        Kind::GeminiGenerateContent,
    ] {
        let converted = convert_request(
            responses,
            content(Operation::GenerateContent, target),
            json!({
                "model":"route","input":"hello","background":false,
                "conversation":{"id":"conv_1"},"include":["reasoning.encrypted_content"],
                "prompt":{"id":"pmpt_1"},"truncation":"auto","store":true
            }),
        );
        assert!(!converted.is_null());
    }
}

#[test]
fn claude_cache_bearing_system_blocks_remain_distinct_responses_items() {
    let claude = content(Operation::GenerateContent, Kind::ClaudeMessages);
    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let converted = convert_request(
        claude,
        responses,
        json!({
            "model":"route","max_tokens":32,
            "cache_control":{"type":"ephemeral"},
            "stop_sequences":["END"],"top_k":10,
            "mcp_servers":[{"type":"url","name":"remote","url":"https://mcp.invalid"}],
            "system":[
                {"type":"text","text":"stable","cache_control":{"type":"ephemeral"}},
                {"type":"text","text":"policy"}
            ],
            "messages":[{"role":"user","content":"hello"}]
        }),
    );
    assert!(converted.get("instructions").is_none());
    assert_eq!(converted["input"][0]["role"], "system");
    assert_eq!(converted["input"][0]["content"][0]["text"], "stable");
    assert_eq!(
        converted["input"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
        "explicit"
    );
    assert_eq!(converted["input"][0]["content"][1]["text"], "policy");
    assert_eq!(converted["tools"][0]["type"], "mcp");
    assert_eq!(converted["tools"][0]["server_label"], "remote");
}

#[test]
fn claude_only_fields_do_not_block_chat_or_gemini_requests() {
    let claude = content(Operation::GenerateContent, Kind::ClaudeMessages);
    let input = json!({
        "model":"route","max_tokens":32,
        "cache_control":{"type":"ephemeral"},
        "stop_sequences":["END"],"top_k":10,
        "system":[{"type":"text","text":"policy","cache_control":{"type":"ephemeral"}}],
        "messages":[{"role":"user","content":"hello"}]
    });
    for target in [Kind::OpenAiChat, Kind::GeminiGenerateContent] {
        let converted = convert_request(
            claude,
            content(Operation::GenerateContent, target),
            input.clone(),
        );
        assert!(!converted.is_null());
    }
}

#[test]
fn mid_conversation_system_policy_is_model_gated() {
    let chat = content(Operation::GenerateContent, Kind::OpenAiChat);
    let claude = content(Operation::GenerateContent, Kind::ClaudeMessages);
    let body = Bytes::from(
        serde_json::to_vec(&json!({
            "model":"route","messages":[
                {"role":"user","content":"one"},
                {"role":"assistant","content":"reply"},
                {"role":"system","content":"mid"},
                {"role":"assistant","content":"again"}
            ]
        }))
        .unwrap(),
    );
    let convert = |model: &str| -> Value {
        let bytes = request(chat, claude, body.clone(), model, false).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    };
    let old = convert("claude-sonnet-4-5");
    assert_eq!(old["messages"][2]["role"], "assistant");
    let new = convert("claude-opus-4-8");
    assert_eq!(new["messages"][2]["role"], "system");
}

#[test]
fn gemini_safety_and_generation_extras_do_not_block_text_requests() {
    let gemini = content(Operation::GenerateContent, Kind::GeminiGenerateContent);
    let input = json!({
        "contents":[{"role":"user","parts":[{"text":"hello"}]}],
        "safetySettings":[{"category":"HARM_CATEGORY_HATE_SPEECH","threshold":"BLOCK_NONE"}],
        "generationConfig":{
            "stopSequences":["END"],"responseModalities":["TEXT"],
            "imageConfig":{"aspectRatio":"1:1"}
        }
    });
    for target in [
        Kind::OpenAiResponses,
        Kind::ClaudeMessages,
        Kind::OpenAiChat,
    ] {
        let converted = convert_request(
            gemini,
            content(Operation::GenerateContent, target),
            input.clone(),
        );
        assert!(!converted.is_null());
    }
}

#[test]
fn chat_to_gemini_merges_adjacent_roles_and_keeps_lossy_tools() {
    let converted = convert_request(
        content(Operation::GenerateContent, Kind::OpenAiChat),
        content(Operation::GenerateContent, Kind::GeminiGenerateContent),
        json!({
            "model":"route",
            "messages":[
                {"role":"user","content":"one"},
                {"role":"user","content":"two"},
                {"role":"assistant","tool_calls":[{
                    "id":"call_1","type":"function",
                    "function":{"name":"lookup","arguments":"not-json"}
                }]}
            ],
            "tools":[{"type":"function","function":{
                "name":"lookup","parameters":{"type":"object"}
            }}]
        }),
    );
    assert_eq!(converted["contents"].as_array().unwrap().len(), 2);
    assert_eq!(converted["contents"][0]["parts"][0]["text"], "one");
    assert_eq!(converted["contents"][0]["parts"][1]["text"], "two");
    assert!(
        converted["contents"][1]["parts"][0]["functionCall"]
            .get("args")
            .is_none()
    );
    assert_eq!(
        converted["tools"][0]["functionDeclarations"][0]["description"],
        ""
    );
}

#[test]
fn request_tool_definitions_map_or_drop_without_killing_text() {
    let claude = content(Operation::GenerateContent, Kind::ClaudeMessages);
    let gemini = content(Operation::GenerateContent, Kind::GeminiGenerateContent);
    let claude_to_gemini = convert_request(
        claude,
        gemini,
        json!({
            "model":"route","max_tokens":32,
            "messages":[{"role":"assistant","content":[{
                "type":"tool_use","id":"toolu_1","name":"lookup","input":{}
            }]}],
            "tools":[{"name":"lookup","input_schema":{"type":"object"}}]
        }),
    );
    assert_eq!(
        claude_to_gemini["tools"][0]["functionDeclarations"][0]["description"],
        ""
    );

    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let input = json!({
        "model":"route","input":"hello",
        "tools":[
            {"type":"web_fetch"},
            {"type":"memory"},
            {"type":"image_generation"}
        ]
    });
    for target in [Kind::ClaudeMessages, Kind::GeminiGenerateContent] {
        let converted = convert_request(
            responses,
            content(Operation::GenerateContent, target),
            input.clone(),
        );
        assert!(!converted.is_null());
    }
}

#[test]
fn named_responses_choice_and_gemini_schema_survive_claude_and_responses_pairs() {
    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let claude = convert_request(
        responses,
        content(Operation::GenerateContent, Kind::ClaudeMessages),
        json!({
            "model":"route",
            "input":"weather",
            "tools":[{"type":"function","name":"get_weather","parameters":{"type":"object"}}],
            "tool_choice":{"type":"function","name":"get_weather"}
        }),
    );
    assert_eq!(claude["tool_choice"]["type"], "tool");
    assert_eq!(claude["tool_choice"]["name"], "get_weather");

    let claude_source = content(Operation::GenerateContent, Kind::ClaudeMessages);
    let responses_from_claude = convert_request(
        claude_source,
        responses,
        json!({
            "model":"route",
            "max_tokens":64,
            "messages":[{"role":"user","content":"weather"}],
            "tools":[{
                "name":"get_weather",
                "description":"weather",
                "strict":true,
                "input_schema":{
                    "type":"object",
                    "properties":{"city":{"type":"string"}},
                    "required":["city"]
                }
            }],
            "tool_choice":{"type":"tool","name":"get_weather"}
        }),
    );
    assert_eq!(responses_from_claude["tool_choice"]["type"], "function");
    assert_eq!(responses_from_claude["tool_choice"]["name"], "get_weather");
    assert_eq!(responses_from_claude["tools"][0]["strict"], true);
    assert_eq!(
        responses_from_claude["tools"][0]["parameters"]["properties"]["city"]["type"],
        "string"
    );
    let wire = request(
        claude_source,
        responses,
        Bytes::from(
            serde_json::to_vec(&json!({
                "model":"route",
                "max_tokens":64,
                "messages":[{"role":"user","content":"weather"}],
                "tools":[{
                    "name":"get_weather",
                    "strict":true,
                    "input_schema":{
                        "type":"object",
                        "properties":{"city":{"type":"string"}},
                        "required":["city"]
                    }
                }]
            }))
            .unwrap(),
        ),
        "upstream-model",
        false,
    )
    .unwrap();
    serde_json::from_slice::<gproxy_protocol::openai::ResponseCreateRequest>(&wire)
        .expect("converted Claude custom tool has no duplicate fields");

    let chat_source = content(Operation::GenerateContent, Kind::OpenAiChat);
    let responses_from_chat = convert_request(
        chat_source,
        responses,
        json!({
            "model":"route",
            "messages":[{"role":"user","content":"weather"}],
            "tools":[{"type":"function","function":{
                "name":"get_weather",
                "strict":true,
                "parameters":{
                    "type":"object",
                    "properties":{"city":{"type":"string"}},
                    "required":["city"]
                }
            }}],
            "tool_choice":{"type":"function","function":{"name":"get_weather"}}
        }),
    );
    assert_eq!(responses_from_chat["tool_choice"]["type"], "function");
    assert_eq!(responses_from_chat["tool_choice"]["name"], "get_weather");
    assert_eq!(responses_from_chat["tools"][0]["strict"], true);
    assert_eq!(
        responses_from_chat["tools"][0]["parameters"]["properties"]["city"]["type"],
        "string"
    );

    let gemini = content(Operation::GenerateContent, Kind::GeminiGenerateContent);
    let responses = convert_request(
        gemini,
        responses,
        json!({
            "contents":[{"role":"user","parts":[{"text":"weather"}]}],
            "tools":[{"functionDeclarations":[{
                "name":"get_weather",
                "description":"weather",
                "parameters":{
                    "type":"OBJECT",
                    "properties":{"city":{"type":"STRING"}},
                    "required":["city"]
                }
            }]}]
        }),
    );
    assert_eq!(responses["tools"][0]["parameters"]["type"], "object");
    assert_eq!(
        responses["tools"][0]["parameters"]["properties"]["city"]["type"],
        "string"
    );
}

#[test]
fn lossy_request_items_are_filtered_without_dropping_text() {
    let responses = content(Operation::GenerateContent, Kind::OpenAiResponses);
    let response_input = json!({
        "model":"route",
        "input":[
            {"type":"future_item","payload":1},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"kept"}]}
        ]
    });
    for target in [
        Kind::OpenAiChat,
        Kind::ClaudeMessages,
        Kind::GeminiGenerateContent,
    ] {
        let converted = convert_request(
            responses,
            content(Operation::GenerateContent, target),
            response_input.clone(),
        );
        assert!(converted.to_string().contains("kept"));
    }

    let claude_to_gemini = convert_request(
        content(Operation::GenerateContent, Kind::ClaudeMessages),
        content(Operation::GenerateContent, Kind::GeminiGenerateContent),
        json!({
            "model":"route","max_tokens":32,
            "messages":[{"role":"assistant","content":[
                {"type":"text","text":"kept"},
                {"type":"mcp_tool_use","id":"mcp_1","server_name":"remote","name":"lookup","input":{}}
            ]}]
        }),
    );
    assert!(claude_to_gemini.to_string().contains("kept"));

    let gemini_to_claude = convert_request(
        content(Operation::GenerateContent, Kind::GeminiGenerateContent),
        content(Operation::GenerateContent, Kind::ClaudeMessages),
        json!({
            "contents":[{"role":"user","parts":[
                {"text":"kept"},
                {"inlineData":{"mimeType":"application/octet-stream","data":"eA=="}}
            ]}]
        }),
    );
    assert!(gemini_to_claude.to_string().contains("kept"));
}

#[test]
fn count_tokens_uses_the_v2_text_canonical_form() {
    let openai = family(Operation::CountTokens, WireFamily::OpenAi);
    let claude = family(Operation::CountTokens, WireFamily::Claude);
    let to_claude = convert_request(
        openai,
        claude,
        json!({
            "model":"route",
            "input":[
                {"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]},
                {"type":"function_call","call_id":"call_1","name":"lookup","arguments":"{}"},
                {"type":"message","id":"msg_1","role":"assistant","status":"completed",
                 "content":[{"type":"output_text","text":"answer","annotations":[]}]}
            ]
        }),
    );
    assert_eq!(to_claude["messages"].as_array().unwrap().len(), 1);
    assert_eq!(to_claude["messages"][0]["role"], "user");
    assert_eq!(to_claude["messages"][0]["content"], "hello\n\nanswer");

    let to_openai = convert_request(
        claude,
        openai,
        json!({
            "model":"claude","messages":[
                {"role":"user","content":[
                    {"type":"text","text":"hello"},
                    {"type":"tool_use","id":"toolu_1","name":"lookup","input":{}}
                ]},
                {"role":"assistant","content":"answer"}
            ]
        }),
    );
    assert_eq!(to_openai["input"], "hello\nanswer");
}

#[test]
fn responses_output_parts_flatten_in_chat_text_roles() {
    let converted = convert_request(
        content(Operation::GenerateContent, Kind::OpenAiResponses),
        content(Operation::GenerateContent, Kind::OpenAiChat),
        json!({
            "model":"route","input":[{
                "type":"message","role":"user","content":[
                    {"type":"output_text","text":"answer","annotations":[]},
                    {"type":"refusal","refusal":"blocked"}
                ]
            }]
        }),
    );
    assert_eq!(converted["messages"][0]["content"][0]["text"], "answer");
    assert_eq!(converted["messages"][0]["content"][1]["text"], "blocked");
}
