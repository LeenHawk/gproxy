use gproxy_protocol::{
    Dialect,
    transform::{generate::claude_responses::*, identity::*},
    wire::{
        claude::{content as cc, generate_content as c},
        openai::responses as r,
    },
};
use serde_json::json;
fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([88; 16]))
}
fn policy() -> TargetIdPolicy {
    TargetIdPolicy::new(Dialect::OpenAi)
}
#[test]
fn tools_controls_multiple_system_messages_and_multimodal_results_map_directly() {
    let input:c::GenerateContentRequestBody=serde_json::from_value(json!({"model":"claude","max_tokens":32,"system":"first","messages":[{"role":"system","content":"second"},{"role":"assistant","content":[{"type":"tool_use","id":"call","name":"f","input":{"x":1}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"call","content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"AQI="}},{"type":"text","text":"done"}]}]}],"tools":[{"name":"f","input_schema":{"type":"object","properties":{"x":{"type":"integer"}},"additionalProperties":false}}],"tool_choice":{"type":"tool","name":"f","disable_parallel_tool_use":true},"output_config":{"effort":"high","format":{"type":"json_schema","schema":{"type":"object"}}}})).unwrap();
    let output = claude_to_responses_request(input, "target", &mut flow(), &policy())
        .unwrap()
        .value;
    let wire = serde_json::to_value(&output).unwrap();
    assert_eq!(wire["instructions"], "first");
    assert_eq!(wire["input"][0]["content"], "second");
    assert_eq!(wire["parallel_tool_calls"], false);
    assert_eq!(
        wire["tools"][0]["parameters"]["additionalProperties"],
        false
    );
    assert_eq!(wire["input"][2]["output"][0]["type"], "input_image");
    assert_eq!(wire["reasoning"]["effort"], "high");
    let back = responses_to_claude_request(output, "claude", ClaudeRequestContext::default())
        .unwrap()
        .value;
    let gproxy_protocol::claude::count_tokens::SystemPrompt::Blocks(blocks) = back.system.unwrap()
    else {
        panic!()
    };
    assert_eq!(blocks.len(), 2);
}

#[test]
fn raw_input_and_file_urls_keep_native_fields() {
    let input: r::GenerateContentRequestBody = serde_json::from_value(
        json!({"model":"source","max_output_tokens":8,"input":"hello","instructions":"rules"}),
    )
    .unwrap();
    let output = responses_to_claude_request(input, "target", ClaudeRequestContext::default())
        .unwrap()
        .value;
    assert_eq!(output.messages.len(), 1);
    assert_eq!(output.model, "target");
    let input:c::GenerateContentRequestBody=serde_json::from_value(json!({"model":"claude","max_tokens":8,"messages":[{"role":"user","content":[{"type":"document","source":{"type":"url","url":"https://example/doc.pdf"}}]}]})).unwrap();
    let output = claude_to_responses_request(input, "target", &mut flow(), &policy())
        .unwrap()
        .value;
    assert_eq!(
        serde_json::to_value(output).unwrap()["input"][0]["content"][0]["file_url"],
        "https://example/doc.pdf"
    );
}
#[test]
fn claude_carried_reasoning_restores_thinking_and_anything_else_is_dropped() {
    // The client carries the Claude signature in encrypted_content behind
    // `claude:`; a later request rebuilds the block from the item alone.
    let reasoning = |encrypted: serde_json::Value| {
        let mut item = json!({"type":"reasoning","id":"rs_any","summary":[],"content":[{"type":"reasoning_text","text":"native text"}]});
        if !encrypted.is_null() {
            item["encrypted_content"] = encrypted;
        }
        let request: r::GenerateContentRequestBody = serde_json::from_value(json!({
            "model":"claude","max_output_tokens":8,
            "input":[item,{"type":"message","role":"user","content":"next"}]
        }))
        .unwrap();
        let back = responses_to_claude_request(request, "claude", ClaudeRequestContext::default())
            .unwrap()
            .value;
        serde_json::to_value(back.messages).unwrap()
    };
    assert_eq!(
        reasoning(json!("claude:native-signature"))[0]["content"],
        json!([{"type":"thinking","thinking":"native text","signature":"native-signature"}])
    );
    for foreign in [
        json!(null),
        json!("gAAAAopenai-ciphertext"),
        json!("gemini:gemini-signature"),
        json!("claude:"),
    ] {
        let messages = reasoning(foreign.clone());
        assert_eq!(
            messages.as_array().unwrap().len(),
            1,
            "{foreign}: {messages}"
        );
        assert_eq!(messages[0]["role"], "user", "{foreign}");
    }
}
#[test]
fn identity_transaction_rolls_back_on_late_unrepresentable_block() {
    let input:c::GenerateContentRequestBody=serde_json::from_value(json!({"model":"claude","max_tokens":8,"messages":[{"role":"assistant","content":[{"type":"tool_use","id":"call","name":"f","input":{}},{"type":"redacted_thinking","data":"opaque"}]}]})).unwrap();
    let mut ids = flow();
    assert!(claude_to_responses_request(input, "target", &mut ids, &policy()).is_err());
    assert!(
        ids.lookup_source(
            &SourceIdentity::new(Dialect::Claude, Some("call".into()), 0),
            IdentityRole::ToolCall
        )
        .is_none()
    );
}

#[test]
fn mcp_connector_and_paired_history_preserve_native_no_approval_contract() {
    let input:c::GenerateContentRequestBody=serde_json::from_value(json!({"model":"claude","max_tokens":8,"mcp_servers":[{"type":"url","name":"server","url":"https://example/mcp","authorization_token":"test-token","tool_configuration":{"enabled":true,"allowed_tools":["f"]}}],"messages":[{"role":"assistant","content":[{"type":"mcp_tool_use","id":"mcp-id","input":{"x":1},"name":"f","server_name":"server"},{"type":"mcp_tool_result","tool_use_id":"mcp-id","is_error":false,"content":"result"}]}]})).unwrap();
    let output = claude_to_responses_request(input, "target", &mut flow(), &policy())
        .unwrap()
        .value;
    let wire = serde_json::to_value(&output).unwrap();
    assert_eq!(wire["tools"][0]["require_approval"], "never");
    assert_eq!(wire["tools"][0]["authorization"], "test-token");
    assert_eq!(wire["input"][0]["type"], "mcp_call");
    assert_eq!(wire["input"][0]["output"], "result");
    let back = responses_to_claude_request(output, "claude", ClaudeRequestContext::default())
        .unwrap()
        .value;
    assert_eq!(back.mcp_servers.unwrap()[0].url, "https://example/mcp");
    let cc::MessageContent::Blocks(blocks) = &back.messages[0].content else {
        panic!()
    };
    assert_eq!(blocks.len(), 2);
    let mut requires_approval = wire;
    requires_approval["tools"][0]["require_approval"] = json!("always");
    assert!(
        responses_to_claude_request(
            serde_json::from_value(requires_approval).unwrap(),
            "claude",
            ClaudeRequestContext::default()
        )
        .is_ok()
    );
}
