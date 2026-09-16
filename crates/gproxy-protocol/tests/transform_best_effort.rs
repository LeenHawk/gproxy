use gproxy_protocol::{
    Dialect,
    transform::{
        generate::{
            chat_responses, claude_chat, claude_gemini, claude_responses, gemini_chat,
            gemini_responses,
        },
        identity::{IdNamespace, IdentityFlow, TargetIdPolicy},
    },
    wire::{
        claude::generate_content as c,
        gemini as g,
        openai::{chat as h, responses as r},
    },
};
use serde_json::{Value, json};
fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([91; 16]))
}
fn policy(d: Dialect) -> TargetIdPolicy {
    TargetIdPolicy::new(d)
}
fn wire<T: serde::Serialize>(v: T) -> Value {
    serde_json::to_value(v).unwrap()
}
fn has_text(v: &Value) {
    assert!(v.to_string().contains("keep this text"), "{v}");
    assert!(!v.to_string().contains("foreign-extension"));
}

#[test]
fn claude_controls_and_opaque_blocks_do_not_veto_supported_content() {
    let input: c::GenerateContentRequestBody = serde_json::from_value(json!({
        "model":"source", "max_tokens":64, "top_k":17,
        "thinking":{"type":"enabled","budget_tokens":12,"display":"omitted"},
        "messages":[{"role":"user","content":[
            {"type":"text","text":"keep this text"},
            {"type":"compaction","content":"opaque context"}
        ]}], "foreign-extension":true
    }))
    .unwrap();
    let chat = wire(
        claude_chat::claude_to_openai(&input, "target")
            .unwrap()
            .value,
    );
    has_text(&chat);
    assert_eq!(chat["max_completion_tokens"], 64);
    assert!(chat.get("top_k").is_none());
    let responses = wire(
        claude_responses::claude_to_responses_request(
            input.clone(),
            "target",
            &mut flow(),
            &policy(Dialect::OpenAi),
        )
        .unwrap()
        .value,
    );
    has_text(&responses);
    assert_eq!(responses["max_output_tokens"], 64);
    let gemini = wire(
        claude_gemini::claude_to_gemini_request(
            input,
            "target",
            Default::default(),
            &mut flow(),
            &policy(Dialect::Gemini),
        )
        .unwrap()
        .value,
    );
    has_text(&gemini);
    assert_eq!(gemini["generationConfig"]["topK"], 17);
    assert_eq!(
        gemini["generationConfig"]["thinkingConfig"]["thinkingBudget"],
        12
    );
}

#[test]
fn chat_controls_are_omitted_only_where_the_target_has_no_field() {
    let input: h::GenerateContentRequestBody = serde_json::from_value(json!({
        "model":"source", "max_completion_tokens":-3, "frequency_penalty":0.4,"seed":17,
        "messages":[{"role":"user","content":"keep this text"}],"foreign-extension":true
    }))
    .unwrap();
    let claude = wire(
        claude_chat::openai_to_claude(&input, "target")
            .unwrap()
            .value,
    );
    has_text(&claude);
    assert_eq!(claude["max_tokens"], -3);
    assert!(claude.get("frequency_penalty").is_none());
    let gemini = wire(
        gemini_chat::openai_to_gemini_request(&input, "target", &Default::default())
            .unwrap()
            .value,
    );
    has_text(&gemini);
    assert_eq!(gemini["generationConfig"]["frequencyPenalty"], 0.4);
    let responses = wire(
        chat_responses::chat_to_responses_request(
            input,
            "target",
            &mut flow(),
            &policy(Dialect::OpenAi),
        )
        .unwrap()
        .value,
    );
    has_text(&responses);
    assert_eq!(responses["max_output_tokens"], -3);
}

#[test]
fn gemini_parts_and_hosted_tools_preserve_independent_supported_fields() {
    let input:g::GenerateContentRequestBody=serde_json::from_value(json!({
        "contents":[{"role":"user","parts":[{"text":"keep this text","mediaResolution":{"level":"MEDIA_RESOLUTION_HIGH"}}]}],
        "generationConfig":{"maxOutputTokens":64,"thinkingConfig":{"thinkingBudget":12,"includeThoughts":true}},
        "tools":[{"googleSearch":{},"functionDeclarations":[{"name":"lookup","description":"lookup","parameters":{"type":"OBJECT"}}]}],
        "foreign-extension":true
    })).unwrap();
    let chat = wire(
        gemini_chat::gemini_to_openai_request(
            input.clone(),
            "target",
            &mut flow(),
            &policy(Dialect::OpenAiChat),
        )
        .unwrap()
        .value,
    );
    has_text(&chat);
    assert_eq!(chat["tools"][0]["function"]["name"], "lookup");
    let claude = wire(
        claude_gemini::gemini_to_claude_request(
            input.clone(),
            "target",
            None,
            &mut flow(),
            &policy(Dialect::Claude),
        )
        .unwrap()
        .value,
    );
    has_text(&claude);
    assert_eq!(claude["tools"][0]["name"], "lookup");
    let responses = wire(
        gemini_responses::gemini_to_responses_request(
            input,
            "target",
            &mut flow(),
            &policy(Dialect::OpenAi),
        )
        .unwrap()
        .value,
    );
    has_text(&responses);
    assert_eq!(responses["tools"][0]["name"], "lookup");
}

#[test]
fn responses_hosted_tools_do_not_discard_the_function_catalog() {
    let input: r::GenerateContentRequestBody = serde_json::from_value(json!({
        "model":"source","max_output_tokens":64,"background":true,
        "input":"keep this text",
        "tools":[{"type":"code_interpreter","container":{"type":"auto"}},
                 {"type":"function","name":"lookup","parameters":{"type":"object"},"strict":false}],
        "foreign-extension":true
    }))
    .unwrap();
    let chat = wire(
        chat_responses::responses_to_chat_request(input.clone(), "target")
            .unwrap()
            .value,
    );
    has_text(&chat);
    assert_eq!(chat["tools"].as_array().unwrap().len(), 1);
    assert_eq!(chat["tools"][0]["function"]["name"], "lookup");
    let claude = wire(
        claude_responses::responses_to_claude_request(input.clone(), "target", Default::default())
            .unwrap()
            .value,
    );
    has_text(&claude);
    assert_eq!(claude["tools"].as_array().unwrap().len(), 1);
    let gemini = wire(
        gemini_responses::responses_to_gemini_request(input, "target", Default::default())
            .unwrap()
            .value,
    );
    has_text(&gemini);
    assert_eq!(
        gemini["tools"][0]["functionDeclarations"][0]["name"],
        "lookup"
    );
}

#[test]
fn malformed_json_arguments_still_fail_when_decoding_is_needed() {
    let input:h::GenerateContentRequestBody=serde_json::from_value(json!({"model":"s","max_tokens":16,"messages":[{"role":"assistant","tool_calls":[{"type":"function","id":"call","function":{"name":"lookup","arguments":"{broken"}}]}]})).unwrap();
    assert!(claude_chat::openai_to_claude(&input, "target").is_err());
}

#[test]
fn omitted_stream_block_does_not_poison_later_text_or_completion() {
    use gproxy_protocol::transform::generate::{
        claude_chat::stream::{ClaudeToChatContext, ClaudeToChatStream},
        stream::{chat::ChatStreamCollector, claude::synthesize_claude_stream},
    };
    let source:c::GenerateContentResponseBody=serde_json::from_value(json!({
        "id":"msg","model":"source","type":"message","role":"assistant","stop_reason":"end_turn","stop_sequence":null,
        "content":[{"type":"server_tool_use","id":"server","name":"web_search","input":{"query":"q"}}, {"type":"text","text":"keep this text"}],
        "usage":{"input_tokens":3,"output_tokens":2,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}
    })).unwrap();
    let events = synthesize_claude_stream(source, Default::default()).unwrap();
    let mut converter = ClaudeToChatStream::new(
        ClaudeToChatContext { created: 1 },
        flow(),
        Default::default(),
    )
    .unwrap();
    let mut collector = ChatStreamCollector::new(flow(), policy(Dialect::OpenAiChat));
    for event in events.value {
        for chunk in converter.push(event).unwrap().value {
            collector.push(chunk).unwrap();
        }
    }
    for chunk in converter.finish().unwrap().chunks {
        collector.push(chunk).unwrap();
    }
    collector.push_done().unwrap();
    let output = collector.finish().unwrap().value;
    assert_eq!(
        output.choices[0].message.content.as_deref(),
        Some("keep this text")
    );
    assert!(output.choices[0].message.tool_calls.is_none());
}
