use gproxy_protocol::{
    Dialect,
    transform::{
        generate::{claude_chat, claude_gemini, claude_responses, stream::claude::*},
        identity::*,
    },
    wire::{
        DeclaredFields,
        claude::{generate_content as c, stream::StreamEvent},
    },
};
use serde_json::json;

fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([91; 16]))
}
fn request() -> c::GenerateContentRequestBody {
    serde_json::from_value(json!({
        "model":"claude-opus-5-5", "max_tokens":128,
        "thinking":{"type":"adaptive","display":"updates","block_binding":{"prefix_mismatch_behavior":"drop_block"}},
        "output_config":{"effort":"high"},
        "tools":[{"name":"old","input_schema":{"type":"object"}}],
        "messages":[
            {"role":"user","content":"first"},
            {"role":"system","clear_at":"next_user_message","content":"expired reminder"},
            {"role":"assistant","content":"done"},
            {"role":"user","content":"next"},
            {"role":"system","content":[
                {"type":"tool_removal","tool":{"type":"tool_reference","name":"old"}},
                {"type":"tool_addition","tool":{"type":"tool_definition","definition":{"name":"new","input_schema":{"type":"object","properties":{"x":{"type":"integer"}}}}}}
            ],"output_config":{"effort":"low"}},
            {"role":"system","clear_at":"next_user_message","content":"current reminder"}
        ]
    })).unwrap()
}

#[test]
fn conversation_controls_project_to_all_generation_pairs() {
    let input = request();
    let chat = claude_chat::claude_to_openai(&input, "target")
        .unwrap()
        .value;
    let responses = claude_responses::claude_to_responses_request(
        input.clone(),
        "target",
        &mut flow(),
        &TargetIdPolicy::new(Dialect::OpenAi),
    )
    .unwrap()
    .value;
    let gemini = claude_gemini::claude_to_gemini_request(
        input.clone(),
        "target",
        Default::default(),
        &mut flow(),
        &TargetIdPolicy::new(Dialect::Gemini),
    )
    .unwrap()
    .value;
    let values = [
        serde_json::to_value(chat).unwrap(),
        serde_json::to_value(responses).unwrap(),
        serde_json::to_value(gemini).unwrap(),
    ];
    for value in &values {
        let text = value.to_string();
        assert!(!text.contains("expired reminder"));
        assert!(text.contains("current reminder"));
        assert!(!text.contains("block_binding"));
        assert!(!text.contains("tool_definition"));
        assert!(text.contains("\"name\":\"new\""));
        assert!(!text.contains("\"name\":\"old\""));
    }
    assert_eq!(values[0]["reasoning_effort"], "low");
    assert_eq!(values[1]["reasoning"]["effort"], "low");
    assert_eq!(values[1]["reasoning"]["summary"], "auto");
    assert_eq!(
        values[2]["generationConfig"]["thinkingConfig"]["includeThoughts"],
        true
    );
    assert_eq!(input.messages.len(), 6); // Projection does not mutate caller history.
}

#[test]
fn count_tokens_uses_the_same_current_tools_and_system_text() {
    let mut value = serde_json::to_value(request()).unwrap();
    value.as_object_mut().unwrap().remove("max_tokens");
    let input = serde_json::from_value(value).unwrap();
    let output = gproxy_protocol::transform::count_tokens::request::claude_to_openai(
        input,
        "target",
        &mut flow(),
        &TargetIdPolicy::new(Dialect::OpenAi),
    )
    .unwrap()
    .value;
    let value = serde_json::to_value(output).unwrap();
    assert_eq!(value["tools"][0]["name"], "new");
    assert!(!value.to_string().contains("expired reminder"));
}

#[test]
fn declared_fields_keep_inline_tools_toolsets_and_mcp_listings() {
    let mut value = serde_json::to_value(request()).unwrap();
    value["compaction"] = json!({"type":"summarize","instructions":"Preserve decisions"});
    value["tools"] = json!([
        {"type":"computer_toolset_20260801","configs":{"zoom":{"enabled":false},"type":{"defer_loading":true}}},
        {"type":"browser_toolset_20260801","configs":{"navigate":{"enabled":true},"javascript_exec":{"enabled":false}}},
        {"type":"mcp_toolset","mcp_server_name":"calendar","tools":[{"name":"events","description":"List events","input_schema":{"type":"object"}}]}
    ]);
    value["messages"].as_array_mut().unwrap().push(json!({"role":"assistant","content":[
        {"type":"mcp_tool_listing","mcp_server_name":"calendar","tools":[{"name":"events","input_schema":{"type":"object"}}]},
        {"type":"tool_use","id":"tool1","name":"navigate","toolset_name":"browser","input":{"url":"https://example.com"}}
    ]}));
    value["messages"].as_array_mut().unwrap().push(json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"tool1","toolset_name":"browser","content":[{"type":"browser_state","tabs":[{"tab_id":"1","title":"Example","url":"https://example.com","active":true}],"state_changes":[{"type":"tab_opened","tab_id":"1"}]}]}]}));
    let body: c::GenerateContentRequestBody = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(body.into_declared()).unwrap(), value);
}

#[test]
fn native_stream_retains_signed_compaction_and_binding_reports() {
    let value = json!({"type":"message","id":"msg1","model":"claude-opus-5-5","role":"assistant","stop_reason":"compaction","content":[{"type":"compaction","content":"summary","signature":"signed-summary"},{"type":"mcp_tool_listing","mcp_server_name":"s","tools":[]}],"usage":{"input_tokens":10,"output_tokens":4},"input_transformations":[{"type":"thinking_mismatch_allowed","path":"messages.1.content.0","reason":"prefix_binding_mismatch"}]});
    let body: c::GenerateContentResponseBody = serde_json::from_value(value.clone()).unwrap();
    let events = synthesize_claude_stream(body, ClaudeStreamLimits::default())
        .unwrap()
        .value;
    let mut collector = ClaudeStreamCollector::new(ClaudeStreamLimits::default());
    for event in events {
        collector.push(event).unwrap();
    }
    assert_eq!(
        serde_json::to_value(collector.finish().unwrap().value).unwrap(),
        value
    );
}

#[test]
fn final_message_delta_replaces_input_transformations_including_empty_list() {
    let events = [
        json!({"type":"message_start","message":{"type":"message","id":"msg1","model":"claude-opus-5-5","role":"assistant","content":[],"usage":{"input_tokens":10,"output_tokens":0},"input_transformations":[{"type":"thinking_dropped","path":"messages.1.content.0","reason":"model_binding_mismatch"}]}}),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"input_transformations":[],"usage":{"output_tokens":1}}),
        json!({"type":"message_stop"}),
    ];
    let mut collector = ClaudeStreamCollector::new(ClaudeStreamLimits::default());
    for event in events {
        collector
            .push(serde_json::from_value::<StreamEvent>(event).unwrap())
            .unwrap();
    }
    let value = serde_json::to_value(collector.finish().unwrap().value).unwrap();
    assert_eq!(value["input_transformations"], json!([]));
}

#[test]
fn ga_file_pagination_and_nullable_expiration_are_declared() {
    let value = json!({"data":[{"type":"file","id":"file1","created_at":"2026-09-22T00:00:00Z","filename":"a.txt","mime_type":"text/plain","size_bytes":1,"expires_at":null}],"next_page":"page_next"});
    let body: gproxy_protocol::claude::files::ListFilesResponseBody =
        serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(body.into_declared()).unwrap(), value);
    let query: gproxy_protocol::claude::files::ListFilesQuery =
        serde_json::from_value(json!({"ids":["file1"]})).unwrap();
    assert_eq!(
        serde_json::to_value(query.into_declared()).unwrap()["ids"],
        json!(["file1"])
    );
}

#[test]
fn newer_claude_targets_omit_disabled_thinking_and_relax_forced_tools() {
    let input = serde_json::from_value(json!({"model":"source","messages":[{"role":"user","content":"go"}],"max_completion_tokens":64,"reasoning_effort":"none","tool_choice":"required","parallel_tool_calls":false})).unwrap();
    for model in [
        "claude-opus-5-5",
        "anthropic/claude-fable-5-1",
        "claude-mythos-5-1",
    ] {
        let converted = claude_chat::openai_to_claude(&input, model).unwrap();
        let value = serde_json::to_value(converted.value).unwrap();
        assert!(value.get("thinking").is_none());
        assert_eq!(value["tool_choice"]["type"], "auto");
        assert_eq!(value["tool_choice"]["disable_parallel_tool_use"], true);
    }
    for model in ["claude-opus-4-6", "claude-opus-5-50"] {
        let value =
            serde_json::to_value(claude_chat::openai_to_claude(&input, model).unwrap().value)
                .unwrap();
        assert_eq!(value["thinking"]["type"], "disabled");
        assert_eq!(value["tool_choice"]["type"], "any");
    }
    let responses = serde_json::from_value(json!({"model":"source","input":"go","max_output_tokens":64,"reasoning":{"effort":"none"},"tool_choice":"required"})).unwrap();
    let value = serde_json::to_value(
        claude_responses::responses_to_claude_request(
            responses,
            "claude-opus-5-5",
            Default::default(),
        )
        .unwrap()
        .value,
    )
    .unwrap();
    assert!(value.get("thinking").is_none());
    assert_eq!(value["tool_choice"]["type"], "auto");
    let gemini = serde_json::from_value(json!({"contents":[{"role":"user","parts":[{"text":"go"}]}],"generationConfig":{"maxOutputTokens":2048,"thinkingConfig":{"thinkingBudget":1024}}})).unwrap();
    let value = serde_json::to_value(
        claude_gemini::gemini_to_claude_request(
            gemini,
            "claude-opus-5-5",
            None,
            &mut flow(),
            &TargetIdPolicy::new(Dialect::Claude),
        )
        .unwrap()
        .value,
    )
    .unwrap();
    assert_eq!(value["thinking"]["type"], "adaptive");
    assert!(value["thinking"].get("budget_tokens").is_none());
}

#[test]
fn removed_custom_tool_can_be_added_back_by_reference() {
    let mut input = request();
    let addition = serde_json::from_value(json!({"role":"system","content":[{"type":"tool_addition","tool":{"type":"tool_reference","name":"old"}}]})).unwrap();
    input.messages.push(addition);
    let value = serde_json::to_value(
        claude_chat::claude_to_openai(&input, "target")
            .unwrap()
            .value,
    )
    .unwrap();
    assert_eq!(value["tools"].as_array().unwrap().len(), 2);
    assert_eq!(value["tools"][1]["function"]["name"], "old");
}

#[test]
fn native_toolset_calls_do_not_escape_as_custom_chat_tools() {
    use claude_chat::stream::{ClaudeToChatContext, ClaudeToChatStream};
    use gproxy_protocol::transform::generate::stream::chat::ChatStreamCollector;
    let body: c::GenerateContentResponseBody = serde_json::from_value(json!({
        "type":"message","id":"msg1","model":"claude-opus-5-5","role":"assistant","stop_reason":"tool_use",
        "content":[{"type":"tool_use","id":"tool1","name":"navigate","toolset_name":"browser","input":{"url":"https://example.com"}}],
        "usage":{"input_tokens":10,"output_tokens":4,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens_details":{"thinking_tokens":0}}
    })).unwrap();
    let events = synthesize_claude_stream(body, ClaudeStreamLimits::default())
        .unwrap()
        .value;
    let mut stream = ClaudeToChatStream::new(
        ClaudeToChatContext { created: 7 },
        flow(),
        Default::default(),
    )
    .unwrap();
    let mut target = ChatStreamCollector::new(flow(), TargetIdPolicy::new(Dialect::OpenAiChat));
    for event in events {
        for chunk in stream.push(event).unwrap().value {
            target.push(chunk).unwrap();
        }
    }
    for chunk in stream.finish().unwrap().chunks {
        target.push(chunk).unwrap();
    }
    target.push_done().unwrap();
    let value = serde_json::to_value(target.finish().unwrap().value).unwrap();
    assert_eq!(value["choices"][0]["finish_reason"], "stop");
    assert!(value["choices"][0]["message"]["tool_calls"].is_null());
}
