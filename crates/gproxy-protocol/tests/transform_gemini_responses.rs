use gproxy_protocol::{
    Dialect, gemini as g,
    openai::responses as r,
    transform::{generate::gemini_responses::*, identity::*},
};
use serde_json::json;
fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([57; 16]))
}
fn policy() -> TargetIdPolicy {
    TargetIdPolicy::new(Dialect::OpenAi)
}
fn context() -> GeminiResponseContext {
    GeminiResponseContext {
        request: serde_json::from_value(
            json!({"model":"selected","input":"x","parallel_tool_calls":true,"tool_choice":"auto"}),
        )
        .unwrap(),
        effective_parallel_tool_calls: true,
        effective_tool_choice: r::ToolChoice::Mode(r::ToolChoiceMode::Auto),
        usage: GeminiUsageFacts {
            cache_write_tokens: Some(0),
            cached_tokens: None,
        },
        created_at: 123,
        effective_prompt_cache_options: None,
    }
}
#[test]
fn direct_request_preserves_roles_media_tools_strict_controls_and_call_results() {
    let input:g::GenerateContentRequestBody=serde_json::from_value(json!({"contents":[{"role":"user","parts":[{"text":"hi"},{"inlineData":{"mimeType":"image/png","data":"AQI="}}]},{"role":"model","parts":[{"functionCall":{"name":"f","args":{"x":1}}}]},{"role":"user","parts":[{"functionResponse":{"name":"f","response":{"result":2}}}]}],"systemInstruction":{"parts":[{"text":"rules"}]},"generationConfig":{"maxOutputTokens":32,"temperature":0.2,"thinkingConfig":{"thinkingLevel":"LOW"},"responseJsonSchema":{"type":"object","additionalProperties":false}},"tools":[{"functionDeclarations":[{"name":"f","description":"f","parametersJsonSchema":{"type":"object","additionalProperties":false}}]}],"toolConfig":{"functionCallingConfig":{"mode":"VALIDATED"}}})).unwrap();
    let output = gemini_to_responses_request(input, "target", &mut flow(), &policy())
        .unwrap()
        .value;
    let wire = serde_json::to_value(&output).unwrap();
    assert_eq!(wire["instructions"], "rules");
    assert_eq!(wire["input"][2]["type"], "function_call");
    assert_eq!(wire["input"][2]["call_id"], wire["input"][3]["call_id"]);
    assert_ne!(wire["input"][2]["id"], wire["input"][2]["call_id"]);
    assert_eq!(wire["tools"][0]["strict"], true);
    let back = responses_to_gemini_request(output, "model", GeminiReplayContext::default())
        .unwrap()
        .value;
    assert_eq!(back.generation_config.unwrap().max_output_tokens, Some(32));
    assert_eq!(
        back.tool_config
            .unwrap()
            .function_calling_config
            .unwrap()
            .mode,
        Some(g::FunctionCallingMode::Validated)
    );
    assert!(back.contents.iter().any(|content| {
        content
            .parts
            .as_ref()
            .unwrap()
            .iter()
            .any(|p| p.function_response.is_some())
    }));
}
#[test]
fn response_status_thoughts_functions_and_usage_are_preserved_without_fake_signatures() {
    let input:g::GenerateContentResponseBody=serde_json::from_value(json!({"responseId":"g1","modelVersion":"actual","candidates":[{"index":0,"finishReason":"MAX_TOKENS","content":{"role":"model","parts":[{"text":"think","thought":true,"thoughtSignature":"native"},{"text":"visible"},{"functionCall":{"name":"f","args":{"x":1}}}]}}],"usageMetadata":{"promptTokenCount":3,"cachedContentTokenCount":1,"candidatesTokenCount":2,"thoughtsTokenCount":1,"totalTokenCount":6}})).unwrap();
    let output = gemini_to_responses_response(input, context(), &mut flow(), &policy())
        .unwrap()
        .value;
    assert_eq!(output.model, "actual");
    assert_eq!(output.status, Some(r::ResponseStatus::Incomplete));
    assert_eq!(
        output
            .usage
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .output_tokens,
        3
    );
    let wire = serde_json::to_value(&output).unwrap();
    assert!(wire["output"][0].get("encrypted_content").is_none());
    assert_eq!(wire["output"][2]["type"], "function_call");
    assert_eq!(wire["incomplete_details"]["reason"], "max_output_tokens");
    let back = responses_to_gemini_response(output, GeminiReplayContext::default())
        .unwrap()
        .value;
    let usage = back.usage_metadata.unwrap();
    assert_eq!(usage.candidates_token_count, Some(2));
    assert_eq!(usage.thoughts_token_count, Some(1));
    assert_eq!(
        back.candidates.unwrap()[0].finish_reason,
        Some(g::FinishReason::MaxTokens)
    );
}
#[test]
fn raw_string_input_multisystem_and_needed_facts_are_explicit() {
    let input: r::GenerateContentRequestBody =
        serde_json::from_value(json!({"input":"hello","instructions":"first","temperature":null}))
            .unwrap();
    let output = responses_to_gemini_request(input, "target", GeminiReplayContext::default())
        .unwrap()
        .value;
    assert_eq!(
        output.contents[0].parts.as_ref().unwrap()[0]
            .text
            .as_deref(),
        Some("hello")
    );
    assert!(output.system_instruction.is_some());
    let input:g::GenerateContentResponseBody=serde_json::from_value(json!({"candidates":[{"finishReason":"STOP","content":{"parts":[{"text":"x"}]}}],"usageMetadata":{"promptTokenCount":1,"candidatesTokenCount":1,"totalTokenCount":2}})).unwrap();
    assert!(gemini_to_responses_response(input, context(), &mut flow(), &policy()).is_err());
}
#[test]
fn identity_transaction_rolls_back_and_multicandidate_never_merges() {
    let input:g::GenerateContentRequestBody=serde_json::from_value(json!({"contents":[{"role":"model","parts":[{"functionCall":{"name":"f","id":"id","args":{}}}]},{"role":"user","parts":[{"inlineData":{"mimeType":"audio/ogg","data":"AQI="}}]}]})).unwrap();
    let mut ids = flow();
    assert!(gemini_to_responses_request(input, "target", &mut ids, &policy()).is_err());
    assert!(
        ids.lookup_source(
            &SourceIdentity::new(Dialect::Gemini, Some("id".into()), 0),
            IdentityRole::ToolCall
        )
        .is_none()
    );
    let input: g::GenerateContentResponseBody = serde_json::from_value(
        json!({"candidates":[{"finishReason":"STOP"},{"finishReason":"STOP"}]}),
    )
    .unwrap();
    assert!(gemini_to_responses_response(input, context(), &mut ids, &policy()).is_err());
}
#[test]
fn malformed_arguments_are_rejected_but_scoped_replay_never_accepts_foreign_ciphertext() {
    let input: r::GenerateContentRequestBody = serde_json::from_value(
        json!({"input":[{"type":"function_call","call_id":"id","name":"f","arguments":"broken"}]}),
    )
    .unwrap();
    assert!(responses_to_gemini_request(input, "target", GeminiReplayContext::default()).is_err());
    let input:r::GenerateContentRequestBody=serde_json::from_value(json!({"input":[{"type":"reasoning","id":"rs","summary":[],"encrypted_content":"foreign"}]})).unwrap();
    assert!(responses_to_gemini_request(input, "target", GeminiReplayContext::default()).is_err());
}

#[test]
fn multimodal_tool_outputs_use_native_parts_and_scoped_reasoning_restores_exact_text() {
    let input:g::GenerateContentRequestBody=serde_json::from_value(json!({"contents":[{"role":"model","parts":[{"functionCall":{"name":"f","id":"call","args":{}}}]},{"role":"user","parts":[{"functionResponse":{"name":"f","id":"call","response":{"result":1},"parts":[{"inlineData":{"mimeType":"image/png","data":"AQI="}}]}}]}]})).unwrap();
    let output = gemini_to_responses_request(input, "target", &mut flow(), &policy())
        .unwrap()
        .value;
    let wire = serde_json::to_value(&output).unwrap();
    assert_eq!(wire["input"][1]["output"][1]["type"], "input_image");
    let back = responses_to_gemini_request(output, "gemini", GeminiReplayContext::default())
        .unwrap()
        .value;
    assert_eq!(
        back.contents[1].parts.as_ref().unwrap()[0]
            .function_response
            .as_ref()
            .unwrap()
            .parts
            .as_ref()
            .unwrap()[0]
            .inline_data
            .as_ref()
            .unwrap()
            .data,
        "AQI="
    );
    let target = IdentityTarget::new("gemini", Dialect::Gemini)
        .unwrap()
        .with_origin("origin")
        .unwrap();
    let mut state = IdentityStateRecord::new(
        IdentityRole::OutputItem(OutputItemKind::Reasoning),
        target.clone(),
    );
    state.client_item_id = Some("rs".into());
    state.opaque_signature = Some(OpaqueSignature::new("sig", "origin", "gemini").unwrap());
    let part = g::Part::builder()
        .thought(true)
        .text("exact")
        .thought_signature("sig")
        .build();
    let context = GeminiReplayContext {
        target: Some(target),
        parts: std::collections::BTreeMap::from([(
            "rs".into(),
            RestoredGeminiPart { state, part },
        )]),
    };
    let input:r::GenerateContentRequestBody=serde_json::from_value(json!({"input":[{"type":"reasoning","id":"rs","summary":[],"content":[{"type":"reasoning_text","text":"exact"}]}]})).unwrap();
    let output = responses_to_gemini_request(input, "gemini", context)
        .unwrap()
        .value;
    assert_eq!(
        output.contents[0].parts.as_ref().unwrap()[0]
            .thought_signature
            .as_deref(),
        Some("sig")
    );
}

#[test]
fn response_logprobs_and_url_citations_map_to_native_gemini_metadata() {
    let input:r::GenerateContentResponseBody=serde_json::from_value(json!({"id":"response","created_at":1,"model":"model","object":"response","status":"completed","output":[{"type":"message","id":"msg","role":"assistant","status":"completed","content":[{"type":"output_text","text":"x","annotations":[{"type":"url_citation","start_index":0,"end_index":1,"title":"title","url":"https://example"}],"logprobs":[{"token":"x","logprob":-0.1,"bytes":[120],"top_logprobs":[]}]}]}],"parallel_tool_calls":true,"temperature":null,"tool_choice":"auto","tools":[],"top_p":null,"error":null,"incomplete_details":null,"instructions":null,"metadata":null,"usage":{"input_tokens":1,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"output_tokens":1,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":2}})).unwrap();
    let output = responses_to_gemini_response(input, GeminiReplayContext::default())
        .unwrap()
        .value;
    let candidates = output.candidates.unwrap();
    assert_eq!(
        candidates[0]
            .citation_metadata
            .as_ref()
            .unwrap()
            .citation_sources
            .as_ref()
            .unwrap()[0]
            .uri
            .as_deref(),
        Some("https://example")
    );
    assert_eq!(
        candidates[0]
            .logprobs_result
            .as_ref()
            .unwrap()
            .chosen_candidates
            .as_ref()
            .unwrap()[0]
            .token
            .as_deref(),
        Some("x")
    );
}
