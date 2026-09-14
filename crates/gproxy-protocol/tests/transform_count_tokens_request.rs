use gproxy_protocol::{
    Dialect,
    transform::{
        count_tokens::request::*,
        generate::{
            claude_gemini::ClaudeGeminiRequestContext, claude_responses::ClaudeRequestContext,
            gemini_responses::GeminiReplayContext,
        },
        identity::*,
    },
    wire::{claude::count_tokens as c, gemini as g, openai::count_tokens as o},
};
use serde_json::json;
fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([91; 16]))
}
fn policy(d: Dialect) -> TargetIdPolicy {
    TargetIdPolicy::new(d)
}
#[test]
fn all_six_count_requests_map_without_generation_token_budget() {
    let claude: c::CountTokensRequestBody = serde_json::from_value(
        json!({"model":"source","messages":[{"role":"user","content":"hello"}]}),
    )
    .unwrap();
    let cg = claude_to_gemini(
        claude.clone(),
        "gemini",
        ClaudeGeminiRequestContext::default(),
        &mut flow(),
        &policy(Dialect::Gemini),
    )
    .unwrap()
    .value;
    let embedded = cg.generate_content_request.unwrap();
    assert_eq!(embedded.model, "models/gemini");
    assert!(
        embedded
            .generation_config
            .as_ref()
            .and_then(|v| v.max_output_tokens)
            .is_none()
    );
    assert!(
        claude_to_openai(claude, "openai", &mut flow(), &policy(Dialect::OpenAi))
            .unwrap()
            .value
            .input
            .is_some()
    );
    let gemini: g::CountTokensRequestBody =
        serde_json::from_value(json!({"contents":[{"role":"model","parts":[{"text":"hello"}]}]}))
            .unwrap();
    let gc = gemini_to_claude(
        gemini.clone(),
        "claude",
        &mut flow(),
        &policy(Dialect::Claude),
    )
    .unwrap()
    .value;
    assert_eq!(
        gc.messages[0].role,
        gproxy_protocol::claude::content::Role::Assistant
    );
    assert!(
        serde_json::to_value(gc)
            .unwrap()
            .get("max_tokens")
            .is_none()
    );
    assert!(gemini_to_openai(gemini, "openai", &mut flow(), &policy(Dialect::OpenAi)).is_ok());
    let openai: o::CountTokensRequestBody =
        serde_json::from_value(json!({"input":"hello","instructions":"rules"})).unwrap();
    let oc = openai_to_claude(openai.clone(), "claude", ClaudeRequestContext::default())
        .unwrap()
        .value;
    assert!(oc.system.is_some());
    assert_eq!(oc.model, "claude");
    let og = openai_to_gemini(openai, "gemini", GeminiReplayContext::default())
        .unwrap()
        .value;
    assert!(
        og.generate_content_request
            .unwrap()
            .system_instruction
            .is_some()
    );
}
#[test]
fn claude_tools_images_schema_thinking_and_results_are_counted_as_native_fields() {
    let input:c::CountTokensRequestBody=serde_json::from_value(json!({"model":"source","system":"rules","thinking":{"type":"disabled"},"output_format":{"type":"json_schema","schema":{"type":"object","additionalProperties":false}},"tools":[{"name":"f","input_schema":{"type":"object","additionalProperties":false}}],"messages":[{"role":"user","content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"AQI="}}]},{"role":"assistant","content":[{"type":"tool_use","id":"call","name":"f","input":{}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"call","content":"done"}]}],"unknown":{"not_counted":true}})).unwrap();
    let output = claude_to_openai(
        input.clone(),
        "openai",
        &mut flow(),
        &policy(Dialect::OpenAi),
    )
    .unwrap()
    .value;
    let wire = serde_json::to_value(&output).unwrap();
    assert_eq!(
        wire["tools"][0]["parameters"]["additionalProperties"],
        false
    );
    assert_eq!(wire["reasoning"]["effort"], "none");
    assert_eq!(wire["input"][0]["content"][0]["type"], "input_image");
    assert!(wire.get("unknown").is_none());
    assert_eq!(wire["input"][1]["call_id"], wire["input"][2]["call_id"]);
    let back = openai_to_claude(output, "claude", ClaudeRequestContext::default())
        .unwrap()
        .value;
    assert!(back.tools.is_some());
    assert!(back.output_format.is_some());
    assert!(back.thinking.is_some());
    let output = claude_to_gemini(
        input,
        "gemini",
        ClaudeGeminiRequestContext::default(),
        &mut flow(),
        &policy(Dialect::Gemini),
    )
    .unwrap()
    .value;
    let embedded = output.generate_content_request.unwrap();
    assert!(embedded.tools.is_some());
    assert_eq!(
        embedded
            .generation_config
            .unwrap()
            .thinking_config
            .unwrap()
            .thinking_budget,
        Some(0)
    );
}
#[test]
fn embedded_gemini_tools_system_schema_and_missing_call_ids_convert() {
    let input:g::CountTokensRequestBody=serde_json::from_value(json!({"generateContentRequest":{"model":"models/source","systemInstruction":{"parts":[{"text":"rules"}]},"contents":[{"role":"model","parts":[{"functionCall":{"name":"f","args":{}}}]},{"role":"user","parts":[{"functionResponse":{"name":"f","response":{"result":1}}}]}],"tools":[{"functionDeclarations":[{"name":"f","description":"function","parametersJsonSchema":{"type":"object","additionalProperties":false}}]}],"generationConfig":{"responseJsonSchema":{"type":"object"},"thinkingConfig":{"thinkingLevel":"LOW"}}}})).unwrap();
    let output = gemini_to_openai(
        input.clone(),
        "openai",
        &mut flow(),
        &policy(Dialect::OpenAi),
    )
    .unwrap()
    .value;
    let wire = serde_json::to_value(&output).unwrap();
    assert_eq!(wire["instructions"], "rules");
    assert_eq!(wire["reasoning"]["effort"], "low");
    assert_eq!(wire["input"][0]["call_id"], wire["input"][1]["call_id"]);
    assert!(wire["tools"].is_array());
    let output = gemini_to_claude(input, "claude", &mut flow(), &policy(Dialect::Claude))
        .unwrap()
        .value;
    assert!(output.system.is_some());
    assert!(output.tools.is_some());
    assert!(output.output_format.is_some());
}
#[test]
fn count_conflicts_context_requirements_and_late_failures_are_explicit() {
    let input: g::CountTokensRequestBody = serde_json::from_value(
        json!({"contents":[],"generateContentRequest":{"model":"models/a","contents":[]}}),
    )
    .unwrap();
    assert!(gemini_to_openai(input, "openai", &mut flow(), &policy(Dialect::OpenAi)).is_err());
    let input: o::CountTokensRequestBody =
        serde_json::from_value(json!({"input":"x","personality":"p"})).unwrap();
    assert!(openai_to_gemini(input, "g", GeminiReplayContext::default()).is_err());
    let input:c::CountTokensRequestBody=serde_json::from_value(json!({"model":"c","messages":[{"role":"assistant","content":[{"type":"tool_use","id":"id","name":"f","input":{}},{"type":"redacted_thinking","data":"native"}]}]})).unwrap();
    let mut ids = flow();
    assert!(claude_to_openai(input, "o", &mut ids, &policy(Dialect::OpenAi)).is_err());
    assert!(
        ids.lookup_source(
            &SourceIdentity::new(Dialect::Claude, Some("id".into()), 0),
            IdentityRole::ToolCall
        )
        .is_none()
    );
}

#[test]
fn image_count_controls_match_generation_without_fake_output_budget() {
    use gproxy_protocol::transform::generate::gemini_responses as pair;
    let config = json!({"responseModalities":["IMAGE"],"responseFormat":{"image":{"mimeType":"IMAGE_JPEG","delivery":"INLINE"},"rest_sentinel":"DROP"}});
    let contents = json!([{"role":"user","parts":[{"text":"draw","rest_sentinel":"DROP"}]}]);
    let count:g::CountTokensRequestBody=serde_json::from_value(json!({"generateContentRequest":{"model":"models/source","contents":contents,"generationConfig":config},"rest_sentinel":"DROP"})).unwrap();
    let generation: g::GenerateContentRequestBody =
        serde_json::from_value(json!({"contents":contents,"generationConfig":config})).unwrap();
    let expected = pair::gemini_to_responses_request(
        generation,
        "selected",
        &mut flow(),
        &policy(Dialect::OpenAi),
    )
    .unwrap()
    .value;
    let actual = gemini_to_openai(count, "selected", &mut flow(), &policy(Dialect::OpenAi))
        .unwrap()
        .value;
    assert_eq!(actual.tools.clone().flatten(), expected.tools);
    assert_eq!(actual.tool_choice.clone().flatten(), expected.tool_choice);
    let wire = serde_json::to_value(&actual).unwrap();
    assert!(!wire.to_string().contains("sentinel"));
    assert!(wire.get("max_tokens").is_none());
    assert!(wire.get("max_output_tokens").is_none());
    let back = openai_to_gemini(actual, "selected", Default::default())
        .unwrap()
        .value
        .generate_content_request
        .unwrap();
    let generation_back =
        pair::responses_to_gemini_request(expected, "selected", Default::default())
            .unwrap()
            .value;
    assert_eq!(back.generation_config, generation_back.generation_config);
    assert_eq!(back.tools, generation_back.tools);
    assert_eq!(back.tool_config, generation_back.tool_config);
    assert!(back.generation_config.unwrap().max_output_tokens.is_none());
}
#[test]
fn count_image_constraints_and_tool_exclusion_use_the_same_native_rules() {
    let gcount = |config| {
        serde_json::from_value::<g::CountTokensRequestBody>(json!({"generateContentRequest":{"model":"models/source","contents":[{"parts":[{"text":"draw"}]}],"generationConfig":config}})).unwrap()
    };
    for config in [
        json!({"responseModalities":["TEXT","IMAGE"],"imageConfig":{"imageSize":"1K"}}),
        json!({"responseModalities":["AUDIO"]}),
    ] {
        assert!(
            gemini_to_openai(
                gcount(config),
                "selected",
                &mut flow(),
                &policy(Dialect::OpenAi)
            )
            .is_err()
        );
    }
    let disabled:o::CountTokensRequestBody=serde_json::from_value(json!({"input":"text","tools":[{"type":"image_generation","size":"999x999"}],"tool_choice":"none"})).unwrap();
    let actual = openai_to_gemini(disabled, "selected", Default::default())
        .unwrap()
        .value
        .generate_content_request
        .unwrap();
    assert_eq!(
        actual.generation_config.unwrap().response_modalities,
        Some(vec![g::Modality::Text])
    );
    let required:o::CountTokensRequestBody=serde_json::from_value(json!({"input":"task","tools":[{"type":"image_generation"},{"type":"function","name":"f","parameters":{},"strict":false}],"tool_choice":"required"})).unwrap();
    assert!(openai_to_gemini(required, "selected", Default::default()).is_err());
}
