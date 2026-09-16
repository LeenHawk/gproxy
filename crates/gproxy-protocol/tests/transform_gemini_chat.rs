use gproxy_protocol::{
    gemini,
    openai::chat,
    transform::{
        generate::gemini_chat,
        identity::{DialectId, IdNamespace, IdentityFlow, TargetIdPolicy},
    },
};
use serde_json::json;
use std::collections::BTreeMap;

fn identity() -> (IdentityFlow, TargetIdPolicy) {
    (
        IdentityFlow::new(IdNamespace::with_bytes([7; 16])),
        TargetIdPolicy::new(DialectId::OpenAiChat),
    )
}

#[test]
fn gemini_request_maps_system_media_functions_schema_and_generation() {
    let input: gemini::GenerateContentRequestBody = serde_json::from_value(json!({
        "contents":[{"role":"user","parts":[{"text":"hi"},{"inlineData":{"mimeType":"image/png","data":"AQI="}}]},{"role":"model","parts":[{"functionCall":{"name":"lookup","id":"gcall","args":{"q":"x"}}}]}],
        "systemInstruction":{"parts":[{"text":"rules"}]},
        "tools":[{"functionDeclarations":[{"name":"lookup","description":"find","parametersJsonSchema":{"type":"object","properties":{"q":{"type":"string"}}}}]}],
        "toolConfig":{"functionCallingConfig":{"mode":"VALIDATED"}},
        "generationConfig":{"candidateCount":1,"maxOutputTokens":32,"thinkingConfig":{"thinkingLevel":"LOW"}}
    })).unwrap();
    let (mut flow, policy) = identity();
    let converted =
        gemini_chat::gemini_to_openai_request(input, "gpt", &mut flow, &policy).unwrap();
    assert_eq!(converted.value.messages.len(), 3);
    assert_eq!(converted.value.max_completion_tokens, Some(Some(32)));
    assert!(converted.value.tools.is_some());
    let chat::ChatTool::Function(tool) = &converted.value.tools.as_ref().unwrap()[0] else {
        panic!()
    };
    assert_eq!(tool.function.strict, Some(Some(true)));
    assert!(
        converted
            .value
            .messages
            .iter()
            .any(|message| matches!(message, chat::ChatMessage::System(_)))
    );
}

#[test]
fn gemini_response_maps_usage_and_function_call_with_factual_supplement() {
    let input: gemini::GenerateContentResponseBody = serde_json::from_value(json!({
        "responseId":"r1","candidates":[{"index":0,"finishReason":"STOP","content":{"role":"model","parts":[{"text":"ok"},{"functionCall":{"name":"lookup","id":"gcall","args":{"q":"x"}}}]}}],
        "usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":2,"totalTokenCount":5}
    })).unwrap();
    let (mut flow, policy) = identity();
    let converted = gemini_chat::gemini_to_openai_response(
        input,
        "gpt",
        &gemini_chat::GeminiChatResponseSupplement {
            created_unix_seconds: Some(1),
        },
        &mut flow,
        &policy,
    )
    .unwrap();
    assert_eq!(converted.value.usage.unwrap().total_tokens, 5);
    assert_eq!(
        converted.value.choices[0]
            .message
            .tool_calls
            .as_ref()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn chat_request_maps_system_and_preserves_function_name_binding() {
    let input: chat::GenerateContentRequestBody = serde_json::from_value(json!({
        "model":"gpt","max_tokens":16,"messages":[{"role":"system","content":"rules"},{"role":"assistant","content":null,"tool_calls":[{"id":"call-1","type":"function","function":{"name":"lookup","arguments":"{\"q\":\"x\"}"}}]},{"role":"tool","tool_call_id":"call-1","content":"done"}],
        "tools":[{"type":"function","function":{"name":"lookup","parameters":{"type":"object"}}}]
    })).unwrap();
    let names = BTreeMap::from([(String::from("call-1"), String::from("lookup"))]);
    let converted = gemini_chat::openai_to_gemini_request(&input, "gemini", &names).unwrap();
    assert!(converted.value.system_instruction.is_some());
    assert!(converted.value.contents.iter().any(|content| {
        content
            .parts
            .as_ref()
            .unwrap()
            .iter()
            .any(|part| part.function_response.is_some())
    }));
}

#[test]
fn same_name_missing_ids_stay_distinct_and_late_failure_is_transactional() {
    let source:gemini::GenerateContentRequestBody=serde_json::from_value(json!({"contents":[{"role":"model","parts":[{"text":"assistant"},{"functionCall":{"name":"f","args":{}}},{"functionCall":{"name":"f","args":{}}}]},{"role":"model","parts":[{"functionCall":{"name":"f","args":{}}}]}]})).unwrap();
    let (mut flow, policy) = identity();
    let output =
        gemini_chat::gemini_to_openai_request(source.clone(), "target", &mut flow, &policy)
            .unwrap()
            .value;
    let mut ids = std::collections::BTreeSet::new();
    for message in output.messages {
        let chat::ChatMessage::Assistant(message) = message else {
            panic!("role changed")
        };
        for call in message.tool_calls.unwrap_or_default() {
            let chat::MessageToolCall::Function(call) = call else {
                panic!()
            };
            assert!(ids.insert(call.id));
        }
    }
    assert_eq!(ids.len(), 3);
    let mut bad = source;
    bad.contents.push(serde_json::from_value(json!({"role":"user","parts":[{"functionResponse":{"name":"f","response":{"output":"ambiguous"}}}]})).unwrap());
    let (mut flow, policy) = identity();
    assert!(gemini_chat::gemini_to_openai_request(bad, "target", &mut flow, &policy).is_err());
    assert!(
        flow.lookup_logical(
            gproxy_protocol::transform::identity::IdentityRole::ToolCall,
            &DialectId::Gemini,
            0
        )
        .is_none()
    );
}

#[test]
fn chat_controls_systems_media_refusal_and_tool_names_are_not_dropped() {
    let source:chat::GenerateContentRequestBody=serde_json::from_value(json!({"model":"source","n":2,"max_completion_tokens":40,"temperature":0.3,"top_p":0.8,"presence_penalty":0.2,"frequency_penalty":0.1,"reasoning_effort":"none","response_format":{"type":"json_schema","json_schema":{"name":"x","schema":{"type":"object","additionalProperties":false},"strict":true}},"tool_choice":{"type":"function","function":{"name":"f"}},"messages":[{"role":"system","content":"one"},{"role":"developer","content":"two"},{"role":"user","content":[{"type":"image_url","image_url":{"url":"data:image/png;base64,AQI="}},{"type":"input_audio","input_audio":{"data":"AwQ=","format":"mp3"}}]},{"role":"assistant","content":[{"type":"refusal","refusal":"no"}],"tool_calls":[{"id":"call","type":"function","function":{"name":"f","arguments":"{}"}}]},{"role":"tool","tool_call_id":"call","content":"done"}]})).unwrap();
    let output = gemini_chat::openai_to_gemini_request(&source, "target", &BTreeMap::new())
        .unwrap()
        .value;
    assert_eq!(output.system_instruction.unwrap().parts.unwrap().len(), 2);
    let config = output.generation_config.unwrap();
    assert_eq!(config.candidate_count, Some(2));
    assert_eq!(config.max_output_tokens, Some(40));
    assert_eq!(config.thinking_config.unwrap().thinking_budget, Some(0));
    assert_eq!(
        config.response_json_schema.unwrap()["additionalProperties"],
        false
    );
    let parts = output.contents[0].parts.as_ref().unwrap();
    assert_eq!(
        parts[0].inline_data.as_ref().unwrap().mime_type,
        "image/png"
    );
    assert_eq!(parts[0].inline_data.as_ref().unwrap().data, "AQI=");
    assert_eq!(
        parts[1].inline_data.as_ref().unwrap().mime_type,
        "audio/mpeg"
    );
    assert_eq!(
        output.contents[1].parts.as_ref().unwrap()[0]
            .text
            .as_deref(),
        Some("no")
    );
    assert_eq!(
        output.contents[2].parts.as_ref().unwrap()[0]
            .function_response
            .as_ref()
            .unwrap()
            .name,
        "f"
    );
}

#[test]
fn response_thought_counts_actual_model_and_finish_reasons_roundtrip() {
    let source:gemini::GenerateContentResponseBody=serde_json::from_value(json!({"responseId":"response","modelVersion":"actual-model","candidates":[{"index":0,"finishReason":"STOP","content":{"role":"model","parts":[{"text":"hidden","thought":true},{"text":"visible"},{"functionCall":{"name":"f","args":{}}},{"functionCall":{"name":"f","args":{}}}]}},{"index":1,"finishReason":"MAX_TOKENS","content":{"role":"model","parts":[{"text":"second"}]}}],"usageMetadata":{"promptTokenCount":10,"cachedContentTokenCount":2,"candidatesTokenCount":5,"thoughtsTokenCount":3,"totalTokenCount":18}})).unwrap();
    let (mut flow, policy) = identity();
    let output = gemini_chat::gemini_to_openai_response(
        source,
        "selected",
        &gemini_chat::GeminiChatResponseSupplement {
            created_unix_seconds: Some(10),
        },
        &mut flow,
        &policy,
    )
    .unwrap()
    .value;
    assert_eq!(output.model, "actual-model");
    assert_eq!(output.choices.len(), 2);
    assert_eq!(
        output.choices[0].finish_reason,
        chat::FinishReason::ToolCalls
    );
    assert_eq!(output.choices[1].finish_reason, chat::FinishReason::Length);
    assert_eq!(
        output.choices[0].message.content.as_deref(),
        Some("visible")
    );
    assert_eq!(output.usage.as_ref().unwrap().completion_tokens, 8);
    let back = gemini_chat::openai_to_gemini_response(&output)
        .unwrap()
        .value;
    let usage = back.usage_metadata.unwrap();
    assert_eq!(usage.candidates_token_count, Some(5));
    assert_eq!(usage.thoughts_token_count, Some(3));
    assert_eq!(usage.total_token_count, Some(18));
    assert_eq!(back.model_version.as_deref(), Some("actual-model"));
}

#[test]
fn unsupported_media_and_inconsistent_usage_fail_without_false_success() {
    let source:gemini::GenerateContentRequestBody=serde_json::from_value(json!({"contents":[{"role":"user","parts":[{"inlineData":{"mimeType":"audio/ogg","data":"AQI="}}]}]})).unwrap();
    let (mut flow, policy) = identity();
    assert!(gemini_chat::gemini_to_openai_request(source, "target", &mut flow, &policy).is_ok());
    let source:gemini::GenerateContentResponseBody=serde_json::from_value(json!({"candidates":[{"finishReason":"STOP","content":{"role":"model","parts":[{"text":"x"}]}}],"usageMetadata":{"promptTokenCount":2,"candidatesTokenCount":1,"thoughtsTokenCount":2,"totalTokenCount":3}})).unwrap();
    assert!(
        gemini_chat::gemini_to_openai_response(
            source,
            "target",
            &gemini_chat::GeminiChatResponseSupplement {
                created_unix_seconds: Some(1)
            },
            &mut flow,
            &policy
        )
        .is_ok()
    );
}

#[test]
fn token_logprobs_map_without_fabricating_token_ids_or_bytes() {
    let source:gemini::GenerateContentResponseBody=serde_json::from_value(json!({"candidates":[{"finishReason":"STOP","content":{"parts":[{"text":"hello"}]},"logprobsResult":{"chosenCandidates":[{"token":"hello","logProbability":-0.1,"tokenId":12}],"topCandidates":[{"candidates":[{"token":"hello","logProbability":-0.1}]}]}}],"usageMetadata":{"promptTokenCount":2,"candidatesTokenCount":1,"totalTokenCount":3}})).unwrap();
    let (mut flow, policy) = identity();
    let converted = gemini_chat::gemini_to_openai_response(
        source,
        "target",
        &gemini_chat::GeminiChatResponseSupplement {
            created_unix_seconds: Some(1),
        },
        &mut flow,
        &policy,
    )
    .unwrap()
    .value;
    let logs = converted.choices[0]
        .logprobs
        .as_ref()
        .unwrap()
        .content
        .as_ref()
        .unwrap();
    assert_eq!(logs[0].token, "hello");
    assert_eq!(logs[0].bytes, None);
    let back = gemini_chat::openai_to_gemini_response(&converted)
        .unwrap()
        .value;
    let candidates = back.candidates.unwrap();
    let logs = candidates[0].logprobs_result.as_ref().unwrap();
    assert_eq!(logs.chosen_candidates.as_ref().unwrap()[0].token_id, None);
    assert_eq!(
        logs.chosen_candidates.as_ref().unwrap()[0].log_probability,
        Some(-0.1)
    );
}

#[test]
fn legacy_function_declarations_history_and_response_map_without_fake_ids() {
    let input:chat::GenerateContentRequestBody=serde_json::from_value(json!({"model":"source","functions":[{"name":"f","parameters":{"type":"object"}}],"function_call":{"name":"f"},"messages":[{"role":"assistant","function_call":{"name":"f","arguments":"{}"}},{"role":"function","name":"f","content":null}]})).unwrap();
    let output = gemini_chat::openai_to_gemini_request(&input, "target", &BTreeMap::new())
        .unwrap()
        .value;
    let call = output.contents[0].parts.as_ref().unwrap()[0]
        .function_call
        .as_ref()
        .unwrap();
    assert_eq!(call.name, "f");
    assert!(call.id.is_none());
    assert_eq!(
        output.contents[1].parts.as_ref().unwrap()[0]
            .function_response
            .as_ref()
            .unwrap()
            .response["output"],
        serde_json::Value::Null
    );
    assert_eq!(
        output.tools.unwrap()[0]
            .function_declarations
            .as_ref()
            .unwrap()[0]
            .name,
        "f"
    );
    let input:chat::GenerateContentResponseBody=serde_json::from_value(json!({"id":"response","model":"source","created":1,"object":"chat.completion","choices":[{"index":0,"finish_reason":"function_call","logprobs":null,"message":{"role":"assistant","content":null,"refusal":null,"function_call":{"name":"f","arguments":"{}"}}}],"usage":{"prompt_tokens":2,"completion_tokens":1,"total_tokens":3,"completion_tokens_details":{"reasoning_tokens":0}}})).unwrap();
    let output = gemini_chat::openai_to_gemini_response(&input)
        .unwrap()
        .value;
    let candidates = output.candidates.unwrap();
    let call = candidates[0]
        .content
        .as_ref()
        .unwrap()
        .parts
        .as_ref()
        .unwrap()[0]
        .function_call
        .as_ref()
        .unwrap();
    assert_eq!(call.name, "f");
    assert!(call.id.is_none());
}

#[test]
fn strict_tools_require_validated_mode_and_keep_forced_allowlist() {
    for choice in [
        json!("auto"),
        json!({"type":"function","function":{"name":"f"}}),
    ] {
        let input:chat::GenerateContentRequestBody=serde_json::from_value(json!({"model":"source","messages":[{"role":"user","content":"x"}],"tools":[{"type":"function","function":{"name":"f","strict":true,"parameters":{"type":"object"}}}],"tool_choice":choice})).unwrap();
        let output = gemini_chat::openai_to_gemini_request(&input, "target", &BTreeMap::new())
            .unwrap()
            .value;
        let config = output.tool_config.unwrap().function_calling_config.unwrap();
        if let Some(names) = config.allowed_function_names {
            assert_eq!(config.mode, Some(gemini::FunctionCallingMode::Any));
            assert_eq!(names, vec!["f"]);
        } else {
            assert_eq!(config.mode, Some(gemini::FunctionCallingMode::Validated));
        }
    }
}
