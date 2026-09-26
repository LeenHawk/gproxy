use gproxy_protocol::{
    Dialect,
    transform::{
        generate::claude_gemini::*,
        identity::{IdNamespace, IdSyntax, IdentityFlow, TargetIdPolicy},
    },
    wire::{claude::generate_content as c, gemini as g},
};
use serde_json::json;

#[test]
fn claude_web_search_uses_google_grounding_without_empty_function_declarations() {
    for version in [
        "web_search_20250305",
        "web_search_20260209",
        "web_search_20260318",
    ] {
        let request = json!({
            "model":"claude", "max_tokens":128,
            "messages":[{"role":"user","content":"Search for Python pathlib documentation"}],
            "tools":[{"type":version,"name":"web_search"}]
        });
        let converted = claude_to_gemini_request(
            serde_json::from_value(request.clone()).unwrap(),
            "gemini",
            Default::default(),
            &mut flow(),
            &policy(Dialect::Gemini),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(converted.value).unwrap()["tools"],
            json!([{"googleSearch":{}}])
        );

        let mut mixed = request;
        mixed["tools"].as_array_mut().unwrap().push(json!({
            "name":"Edit", "input_schema":{"type":"object","properties":{}}
        }));
        let converted = claude_to_gemini_request(
            serde_json::from_value(mixed).unwrap(),
            "gemini",
            Default::default(),
            &mut flow(),
            &policy(Dialect::Gemini),
        )
        .unwrap();
        let tools = serde_json::to_value(converted.value).unwrap()["tools"].clone();
        assert_eq!(tools[0]["functionDeclarations"][0]["name"], "Edit");
        assert_eq!(tools[1], json!({"googleSearch":{}}));
    }
}

fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([42; 16]))
}
fn policy(d: Dialect) -> TargetIdPolicy {
    TargetIdPolicy::new(d)
}
fn facts() -> ClaudeGeminiUsageFacts {
    ClaudeGeminiUsageFacts {
        cache_creation_input_tokens: Some(0),
        cache_read_input_tokens: Some(0),
        thinking_tokens: Some(0),
    }
}
fn c_request() -> c::GenerateContentRequestBody {
    serde_json::from_value(
        json!({"model":"claude","max_tokens":128,"messages":[{"role":"user","content":"hi"}]}),
    )
    .unwrap()
}
fn g_response() -> g::GenerateContentResponseBody {
    serde_json::from_value(json!({"responseId":"native-id","modelVersion":"actual-gemini","candidates":[{"index":0,"content":{"role":"model","parts":[{"text":"answer"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":3,"totalTokenCount":15,"thoughtsTokenCount":2,"cachedContentTokenCount":4}})).unwrap()
}
fn c_response() -> c::GenerateContentResponseBody {
    serde_json::from_value(json!({"type":"message","id":"native-id","model":"actual-claude","role":"assistant","content":[{"type":"text","text":"answer"}],"stop_reason":"end_turn","usage":{"input_tokens":7,"output_tokens":5,"cache_creation_input_tokens":3,"cache_read_input_tokens":4,"output_tokens_details":{"thinking_tokens":2}}})).unwrap()
}
#[test]
fn request_controls_tools_schema_system_and_rest_are_direct() {
    let input=serde_json::from_value(json!({"model":"claude","max_tokens":128,"temperature":0.4,"top_k":20,"top_p":0.8,"stop_sequences":["STOP"],"thinking":{"type":"enabled","budget_tokens":32,"x-extension":"drop"},"system":[{"type":"text","text":"first","x-extension":"drop"}],"messages":[{"role":"system","content":"second"},{"role":"assistant","content":[{"type":"tool_use","id":"native-call","name":"lookup","input":{"q":"x","x-formal":true},"x-extension":"drop"}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"native-call","is_error":true,"content":"bad","x-extension":"drop"}]}],"tools":[{"name":"lookup","input_schema":{"type":"object","properties":{"q":{"type":"string","x-formal":true}},"required":["q"],"x-extension":"drop"},"strict":true,"x-extension":"drop"}],"x-extension":"drop"})).unwrap();
    let out = claude_to_gemini_request(
        input,
        "gemini",
        Default::default(),
        &mut flow(),
        &policy(Dialect::Gemini),
    )
    .unwrap()
    .value;
    let encoded = serde_json::to_value(&out).unwrap();
    assert!(!encoded.to_string().contains("x-extension"));
    assert_eq!(encoded["systemInstruction"]["parts"][0]["text"], "first");
    assert_eq!(encoded["systemInstruction"]["parts"][1]["text"], "second");
    assert_eq!(
        encoded["contents"][1]["parts"][0]["functionResponse"]["name"],
        "lookup"
    );
    assert_eq!(
        encoded["contents"][1]["parts"][0]["functionResponse"]["response"]["error"],
        "bad"
    );
    assert_eq!(
        encoded["toolConfig"]["functionCallingConfig"]["mode"],
        "VALIDATED"
    );
    assert_eq!(
        encoded["tools"][0]["functionDeclarations"][0]["parametersJsonSchema"]["properties"]["q"]["x-formal"],
        true
    );
    assert_eq!(
        encoded["generationConfig"]["thinkingConfig"]["thinkingBudget"],
        32
    );
    let back = gemini_to_claude_request(
        out,
        "target-claude",
        None,
        &mut flow(),
        &policy(Dialect::Claude),
    )
    .unwrap()
    .value;
    assert_eq!(back.max_tokens, 128);
    assert_eq!(back.top_k, Some(20));
    assert_eq!(back.temperature, Some(0.4));
    assert_eq!(
        serde_json::to_value(back).unwrap()["messages"][1]["content"][0]["is_error"],
        true
    );
}
#[test]
fn duplicate_names_have_unique_ids_and_ambiguous_results_fail() {
    let input:g::GenerateContentRequestBody=serde_json::from_value(json!({"contents":[{"role":"model","parts":[{"functionCall":{"name":"f","args":{}}},{"functionCall":{"name":"f","args":{}}}]}]})).unwrap();
    let out = gemini_to_claude_request(
        input.clone(),
        "c",
        Some(100),
        &mut flow(),
        &policy(Dialect::Claude),
    )
    .unwrap()
    .value;
    let value = serde_json::to_value(out).unwrap();
    assert_ne!(
        value["messages"][0]["content"][0]["id"],
        value["messages"][0]["content"][1]["id"]
    );
    let mut input = input;
    input.contents.push(serde_json::from_value(json!({"role":"user","parts":[{"functionResponse":{"name":"f","response":{"output":"x"}}}]})).unwrap());
    assert!(
        gemini_to_claude_request(input, "c", Some(100), &mut flow(), &policy(Dialect::Claude))
            .is_err()
    );
}
#[test]
fn multimedia_history_and_multimodal_tool_result_map() {
    let input=serde_json::from_value(json!({"model":"c","max_tokens":128,"messages":[{"role":"user","content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"YWJj"}},{"type":"document","source":{"type":"text","media_type":"text/plain","data":"doc"}}]},{"role":"assistant","content":[{"type":"tool_use","id":"t","name":"f","input":{}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"t","content":[{"type":"text","text":"result"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"YWJj"}}]}]}]})).unwrap();
    let g = claude_to_gemini_request(
        input,
        "g",
        Default::default(),
        &mut flow(),
        &policy(Dialect::Gemini),
    )
    .unwrap()
    .value;
    assert_eq!(g.contents[0].parts.as_ref().unwrap().len(), 2);
    let back = gemini_to_claude_request(g, "c", None, &mut flow(), &policy(Dialect::Claude))
        .unwrap()
        .value;
    let val = serde_json::to_value(back).unwrap();
    assert_eq!(
        val["messages"][0]["content"][0]["source"]["media_type"],
        "image/png"
    );
    assert_eq!(
        val["messages"][2]["content"][0]["content"][1]["type"],
        "image"
    );
}
#[test]
fn response_usage_cache_thoughts_models_and_signature_isolation() {
    let mut input = c_response();
    input.content.push(serde_json::from_value(json!({"type":"thinking","thinking":"why","signature":"claude-secret","x-extension":"drop"})).unwrap());
    let out = claude_to_gemini_response(
        input,
        Default::default(),
        &mut flow(),
        &policy(Dialect::Gemini),
    )
    .unwrap();
    let val = serde_json::to_value(out.value).unwrap();
    assert_eq!(val["usageMetadata"]["promptTokenCount"], 14);
    assert_eq!(val["usageMetadata"]["candidatesTokenCount"], 3);
    assert_eq!(val["usageMetadata"]["totalTokenCount"], 19);
    assert!(!val.to_string().contains("claude-secret"));
    assert_eq!(val["candidates"][0]["content"]["parts"][1]["thought"], true);
    let out = gemini_to_claude_response(
        g_response(),
        Some("fallback".into()),
        ClaudeGeminiUsageFacts {
            cache_creation_input_tokens: Some(0),
            ..Default::default()
        },
        &mut flow(),
        &policy(Dialect::Claude),
    )
    .unwrap()
    .value;
    assert_eq!(out.model, "actual-gemini");
    assert_eq!(out.usage.input_tokens, 6);
    assert_eq!(out.usage.output_tokens, 5);
    assert_eq!(
        out.usage
            .output_tokens_details
            .flatten()
            .unwrap()
            .thinking_tokens,
        2
    );
}
#[test]
fn incomplete_failure_and_tool_terminal_are_not_default_success() {
    let mut source = g_response();
    source.candidates.as_mut().unwrap()[0].finish_reason = Some(g::FinishReason::MaxTokens);
    let actual = ClaudeGeminiUsageFacts {
        cache_creation_input_tokens: Some(0),
        ..Default::default()
    };
    let out = gemini_to_claude_response(
        source.clone(),
        None,
        actual,
        &mut flow(),
        &policy(Dialect::Claude),
    )
    .unwrap()
    .value;
    assert_eq!(out.stop_reason, c::StopReason::MaxTokens);
    source.candidates.as_mut().unwrap()[0].finish_reason =
        Some(g::FinishReason::MalformedFunctionCall);
    assert!(
        gemini_to_claude_response(
            source.clone(),
            None,
            actual,
            &mut flow(),
            &policy(Dialect::Claude)
        )
        .is_err()
    );
    source.candidates.as_mut().unwrap()[0]=serde_json::from_value(json!({"finishReason":"STOP","content":{"role":"model","parts":[{"functionCall":{"name":"f","args":{}}}]}})).unwrap();
    let out =
        gemini_to_claude_response(source, None, actual, &mut flow(), &policy(Dialect::Claude))
            .unwrap()
            .value;
    assert_eq!(out.stop_reason, c::StopReason::ToolUse);
}
#[test]
fn missing_facts_invalid_counts_and_multi_candidates_reject() {
    assert!(
        gemini_to_claude_response(
            g_response(),
            None,
            Default::default(),
            &mut flow(),
            &policy(Dialect::Claude)
        )
        .is_err()
    );
    let mut input = c_response();
    input.usage.output_tokens = i64::MAX;
    assert!(
        claude_to_gemini_response(
            input,
            Default::default(),
            &mut flow(),
            &policy(Dialect::Gemini)
        )
        .is_err()
    );
    let mut input = g_response();
    input.candidates = Some(Vec::new());
    assert!(
        gemini_to_claude_response(input, None, facts(), &mut flow(), &policy(Dialect::Claude))
            .is_err()
    );
    let mut input = g_response();
    let candidate = input.candidates.as_ref().unwrap()[0].clone();
    input.candidates.as_mut().unwrap().push(candidate);
    assert!(
        gemini_to_claude_response(input, None, facts(), &mut flow(), &policy(Dialect::Claude))
            .is_ok()
    );
    let mut input = c_request();
    input.messages=vec![serde_json::from_value(json!({"role":"assistant","content":[{"type":"tool_use","id":"t","name":"f","input":"invalid"}]})).unwrap()];
    assert!(
        claude_to_gemini_request(
            input,
            "g",
            Default::default(),
            &mut flow(),
            &policy(Dialect::Gemini)
        )
        .is_err()
    );
}
#[test]
fn failed_response_identity_transaction_does_not_poison_retry() {
    let mut input = g_response();
    input.candidates.as_mut().unwrap()[0].content = Some(
        serde_json::from_value(
            json!({"role":"model","parts":[{"functionCall":{"name":"f","id":"a/b","args":{}}}]}),
        )
        .unwrap(),
    );
    let mut ids = flow();
    assert!(
        gemini_to_claude_response(
            input.clone(),
            None,
            Default::default(),
            &mut ids,
            &policy(Dialect::Claude)
        )
        .is_err()
    );
    let actual = ClaudeGeminiUsageFacts {
        cache_creation_input_tokens: Some(0),
        ..Default::default()
    };
    let retried = gemini_to_claude_response(
        input.clone(),
        None,
        actual,
        &mut ids,
        &policy(Dialect::Claude),
    )
    .unwrap()
    .value;
    let fresh =
        gemini_to_claude_response(input, None, actual, &mut flow(), &policy(Dialect::Claude))
            .unwrap()
            .value;
    assert_eq!(retried, fresh);
}
#[test]
fn request_budget_unknown_roles_foreign_thinking_and_media_facts() {
    let input:g::GenerateContentRequestBody=serde_json::from_value(json!({"contents":[{"role":"model","parts":[{"thought":true,"thoughtSignature":"foreign","text":"why"},{"text":"answer"}]}]})).unwrap();
    assert!(
        gemini_to_claude_request(
            input.clone(),
            "c",
            None,
            &mut flow(),
            &policy(Dialect::Claude)
        )
        .is_err()
    );
    let out =
        gemini_to_claude_request(input, "c", Some(128), &mut flow(), &policy(Dialect::Claude))
            .unwrap();
    assert!(!out.report.diagnostics.is_empty());
    assert!(
        !serde_json::to_value(out.value)
            .unwrap()
            .to_string()
            .contains("foreign")
    );
    let mut input = c_request();
    input.messages=vec![serde_json::from_value(json!({"role":"user","content":[{"type":"image","source":{"type":"url","url":"https://example.com/x"}}]})).unwrap()];
    assert!(
        claude_to_gemini_request(
            input.clone(),
            "g",
            Default::default(),
            &mut flow(),
            &policy(Dialect::Gemini)
        )
        .is_ok()
    );
    let mut ctx = ClaudeGeminiRequestContext::default();
    ctx.media.resources.insert(
        "https://example.com/x".into(),
        g::FileData::builder("gs://bucket/x".into())
            .mime_type("image/png")
            .build(),
    );
    let out = claude_to_gemini_request(input, "g", ctx, &mut flow(), &policy(Dialect::Gemini))
        .unwrap()
        .value;
    assert_eq!(
        out.contents[0].parts.as_ref().unwrap()[0]
            .file_data
            .as_ref()
            .unwrap()
            .file_uri,
        "gs://bucket/x"
    );
}

#[test]
fn thought_history_keeps_text_and_seeded_results_obey_target_policy() {
    let input:c::GenerateContentRequestBody=serde_json::from_value(json!({"model":"c","max_tokens":8,"messages":[{"role":"assistant","content":[{"type":"thinking","thinking":"keep text","signature":"native"}]}]})).unwrap();
    let output = claude_to_gemini_request(
        input,
        "g",
        Default::default(),
        &mut flow(),
        &policy(Dialect::Gemini),
    )
    .unwrap()
    .value;
    let part = &output.contents[0].parts.as_ref().unwrap()[0];
    assert_eq!(part.thought, Some(true));
    assert_eq!(part.text.as_deref(), Some("keep text"));
    assert!(part.thought_signature.is_none());
    let input:c::GenerateContentRequestBody=serde_json::from_value(json!({"model":"c","max_tokens":8,"messages":[{"role":"user","content":[{"type":"tool_result","tool_use_id":"bad/id","content":"result"}]}]})).unwrap();
    let mut context = ClaudeGeminiRequestContext::default();
    context.tool_names.insert("bad/id".into(), "f".into());
    let strict = policy(Dialect::Gemini).with_syntax(IdSyntax::AsciiIdentifier);
    assert!(claude_to_gemini_request(input, "g", context, &mut flow(), &strict).is_err());
}

#[test]
fn output_schema_mime_conflict_rejects_and_native_web_citation_keeps_source() {
    let input:g::GenerateContentRequestBody=serde_json::from_value(json!({"contents":[{"parts":[{"text":"x"}]}],"generationConfig":{"maxOutputTokens":8,"responseMimeType":"text/plain","responseJsonSchema":{"type":"object"}}})).unwrap();
    assert!(
        gemini_to_claude_request(input, "c", None, &mut flow(), &policy(Dialect::Claude)).is_ok()
    );
    let mut input = c_response();
    input.content=vec![serde_json::from_value(json!({"type":"text","text":"answer","citations":[{"type":"web_search_result_location","cited_text":"source","encrypted_index":"opaque","url":"https://example/source","title":"title"}]})).unwrap()];
    input.stop_reason = c::StopReason::EndTurn;
    let output = claude_to_gemini_response(
        input,
        Default::default(),
        &mut flow(),
        &policy(Dialect::Gemini),
    )
    .unwrap()
    .value;
    assert_eq!(
        output.candidates.unwrap()[0]
            .citation_metadata
            .as_ref()
            .unwrap()
            .citation_sources
            .as_ref()
            .unwrap()[0]
            .uri
            .as_deref(),
        Some("https://example/source")
    );
}

#[test]
fn thought_flag_on_a_function_preserves_the_payload_without_a_foreign_signature() {
    let mut input = g_response();
    input.candidates.as_mut().unwrap()[0].content.as_mut().unwrap().parts = Some(vec![serde_json::from_value(json!({"thought":true,"thoughtSignature":"opaque","functionCall":{"id":"actual-call","name":"run","args":{"x":1}}})).unwrap()]);
    let converted = gemini_to_claude_response(
        input,
        None,
        ClaudeGeminiUsageFacts {
            cache_creation_input_tokens: Some(0),
            cache_read_input_tokens: Some(4),
            thinking_tokens: Some(2),
        },
        &mut flow(),
        &policy(Dialect::Claude),
    )
    .unwrap();
    assert_eq!(converted.value.stop_reason, c::StopReason::ToolUse);
    let c::ResponseContentBlock::ToolUse(tool) = &converted.value.content[0] else {
        panic!("thought metadata cannot erase a function payload")
    };
    assert_eq!(tool.name, "run");
    assert_eq!(tool.input["x"], 1);
    assert!(
        !serde_json::to_string(&converted.value)
            .unwrap()
            .contains("opaque")
    );
}
