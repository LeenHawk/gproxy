use gproxy_protocol::{
    Dialect,
    transform::{
        TransformErrorKind,
        generate::claude_responses::*,
        identity::{IdNamespace, IdentityFlow, TargetIdPolicy},
    },
    wire::{claude::generate_content as c, openai::responses as r},
};
use serde_json::{Value, json};
fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([13; 16]))
}
fn context() -> ClaudeResponseContext {
    ClaudeResponseContext {request:serde_json::from_value(json!({"model":"original","input":"hi","parallel_tool_calls":false,"tool_choice":"auto","metadata":{"formal":"x"},"unknown":{"bad":1}})).unwrap(),
        effective_parallel_tool_calls:false,effective_tool_choice:r::ToolChoice::Mode(r::ToolChoiceMode::Auto),effective_prompt_cache_options:None,
        usage:ResponsesUsageFacts::default(),created_at:123,
    }
}
fn claude(content: Value, stop: &str) -> c::GenerateContentResponseBody {
    serde_json::from_value(json!({"id":"claude-message","type":"message","role":"assistant","model":"actual-model","content":content,"stop_reason":stop,"usage":{"input_tokens":3,"output_tokens":5,"cache_creation_input_tokens":2,"cache_read_input_tokens":4,"output_tokens_details":{"thinking_tokens":1,"unknown":true},"unknown":true},"unknown":{"bad":1}})).unwrap()
}
fn response(output: Value, status: &str) -> r::GenerateContentResponseBody {
    serde_json::from_value(json!({"id":"response-id","object":"response","created_at":123,"model":"actual-model","output":output,"status":status,"error":null,"incomplete_details":null,"instructions":null,"metadata":null,"temperature":null,"top_p":null,"parallel_tool_calls":false,"tool_choice":"auto","tools":[],"usage":{"input_tokens":9,"input_tokens_details":{"cache_write_tokens":2,"cached_tokens":4},"output_tokens":5,"output_tokens_details":{"reasoning_tokens":1},"total_tokens":14}})).unwrap()
}
#[test]
fn claude_text_thinking_tools_preserve_formal_data_and_identity_roles() {
    let input = claude(
        json!([
            {"type":"thinking","thinking":"reason","signature":"CLAUDE-ONLY","unknown":true},
            {"type":"text","text":"answer","unknown":true},
            {"type":"tool_use","id":"native/a","name":"lookup","input":{"unknown":{"formal":true}},"unknown":true},
            {"type":"tool_use","id":"native-a","name":"lookup","input":{}}
        ]),
        "tool_use",
    );
    let mut ids = flow();
    let converted = claude_to_responses_response(
        input,
        context(),
        &mut ids,
        &TargetIdPolicy::new(Dialect::OpenAi),
    )
    .unwrap();
    let value = converted.value;
    assert_eq!(value.created_at, 123);
    assert_eq!(value.model, "actual-model");
    assert!(!value.parallel_tool_calls);
    assert_eq!(value.metadata.unwrap()["formal"], json!("x"));
    let r::ResponseOutputItem::Reasoning(thinking) = &value.output[0] else {
        panic!()
    };
    assert_eq!(thinking.content.as_ref().unwrap()[0].text, "reason");
    assert!(thinking.encrypted_content.is_none());
    let mut emitted = Vec::new();
    for item in &value.output[2..] {
        let r::ResponseOutputItem::FunctionCall(call) = item else {
            panic!()
        };
        assert_ne!(call.id.as_deref(), Some(call.call_id.as_str()));
        emitted.push(call.call_id.clone());
        assert!(call.rest.is_empty());
    }
    assert_eq!(emitted, vec!["native/a", "native-a"]);
    let r::ResponseOutputItem::FunctionCall(call) = &value.output[2] else {
        panic!()
    };
    assert_eq!(
        serde_json::from_str::<Value>(&call.arguments).unwrap(),
        json!({"unknown":{"formal":true}})
    );
    let usage = value.usage.unwrap().unwrap();
    assert_eq!(
        (usage.input_tokens, usage.output_tokens, usage.total_tokens),
        (9, 5, 14)
    );
    assert!(usage.input_tokens_details.rest.is_empty());
    assert!(!converted.report.diagnostics.is_empty());
}
#[test]
fn max_tokens_and_refusal_do_not_turn_into_normal_end_turn() {
    let mut ids = flow();
    let short = claude_to_responses_response(
        claude(json!([{"type":"text","text":"part"}]), "max_tokens"),
        context(),
        &mut ids,
        &TargetIdPolicy::new(Dialect::OpenAi),
    )
    .unwrap()
    .value;
    assert_eq!(short.status, Some(r::ResponseStatus::Incomplete));
    assert_eq!(
        short.incomplete_details.unwrap().reason,
        Some(r::ResponseIncompleteReason::MaxOutputTokens)
    );
    let mut ids = flow();
    let refusal = claude_to_responses_response(
        claude(json!([{"type":"text","text":"declined"}]), "refusal"),
        context(),
        &mut ids,
        &TargetIdPolicy::new(Dialect::OpenAi),
    )
    .unwrap()
    .value;
    let r::ResponseOutputItem::Message(message) = &refusal.output[0] else {
        panic!()
    };
    assert!(matches!(&message.content[0], r::OutputContent::Refusal(_)));
    for stop in ["pause_turn", "compaction", "model_context_window_exceeded"] {
        assert!(
            claude_to_responses_response(
                claude(json!([]), stop),
                context(),
                &mut flow(),
                &TargetIdPolicy::new(Dialect::OpenAi)
            )
            .is_ok()
        );
    }
}
#[test]
fn actual_usage_supplements_are_required_checked_and_not_zero_filled() {
    let mut source = claude(json!([]), "end_turn");
    source.usage.cache_read_input_tokens = None;
    assert_eq!(
        claude_to_responses_response(
            source.clone(),
            context(),
            &mut flow(),
            &TargetIdPolicy::new(Dialect::OpenAi)
        )
        .unwrap_err()
        .kind(),
        TransformErrorKind::MissingMetadata
    );
    let mut facts = context();
    facts.usage.cached_tokens = Some(4);
    assert_eq!(
        claude_to_responses_response(
            source,
            facts,
            &mut flow(),
            &TargetIdPolicy::new(Dialect::OpenAi)
        )
        .unwrap()
        .value
        .usage
        .unwrap()
        .unwrap()
        .input_tokens,
        9
    );
    let mut facts = context();
    facts.usage.reasoning_tokens = Some(2);
    assert!(
        claude_to_responses_response(
            claude(json!([]), "end_turn"),
            facts,
            &mut flow(),
            &TargetIdPolicy::new(Dialect::OpenAi)
        )
        .is_ok()
    );
    let mut source = claude(json!([]), "end_turn");
    source.usage.input_tokens = i64::MAX;
    assert!(
        claude_to_responses_response(
            source,
            context(),
            &mut flow(),
            &TargetIdPolicy::new(Dialect::OpenAi)
        )
        .is_err()
    );
}
#[test]
fn failed_response_or_late_context_error_never_commits_identity_flow() {
    let original = flow();
    let mut ids = original.clone();
    let mut facts = context();
    facts.effective_parallel_tool_calls = true;
    let source = claude(
        json!([{"type":"tool_use","id":"call","name":"lookup","input":{}}]),
        "tool_use",
    );
    assert!(
        claude_to_responses_response(
            source,
            facts,
            &mut ids,
            &TargetIdPolicy::new(Dialect::OpenAi)
        )
        .is_err()
    );
    assert_eq!(format!("{ids:?}"), format!("{original:?}"));
    for status in ["failed", "queued", "in_progress", "cancelled"] {
        assert!(
            responses_to_claude_response(
                response(json!([]), status),
                &mut ids,
                &TargetIdPolicy::new(Dialect::Claude)
            )
            .is_err()
        );
        assert_eq!(format!("{ids:?}"), format!("{original:?}"));
    }
}
#[test]
fn responses_function_calls_use_call_id_and_preserve_mixed_refusal_text() {
    let source = response(
        json!([
            {"type":"message","id":"item-message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"prefix","annotations":[],"logprobs":[]},{"type":"refusal","refusal":"declined"}]},
            {"type":"function_call","id":"item-distinct","call_id":"actual-call","name":"lookup","arguments":"{\"formal\":true}","status":"completed"}
        ]),
        "completed",
    );
    let result =
        responses_to_claude_response(source, &mut flow(), &TargetIdPolicy::new(Dialect::Claude))
            .unwrap()
            .value;
    assert_eq!(result.stop_reason, c::StopReason::ToolUse);
    assert_eq!(result.content.len(), 3);
    let c::ResponseContentBlock::ToolUse(call) = &result.content[2] else {
        panic!()
    };
    assert_eq!(call.id, "actual-call");
    assert_eq!(call.input["formal"], json!(true));
    assert_eq!(result.usage.input_tokens, 3);
    assert_eq!(result.usage.cache_read_input_tokens, Some(Some(4)));
}
#[test]
fn missing_or_inconsistent_usage_and_nonterminal_items_are_errors() {
    let mut source = response(json!([]), "completed");
    source.usage = None;
    assert_eq!(
        responses_to_claude_response(source, &mut flow(), &TargetIdPolicy::new(Dialect::Claude))
            .unwrap_err()
            .kind(),
        TransformErrorKind::MissingMetadata
    );
    let mut source = response(json!([]), "completed");
    source
        .usage
        .as_mut()
        .unwrap()
        .as_mut()
        .unwrap()
        .total_tokens = 99;
    assert!(
        responses_to_claude_response(source, &mut flow(), &TargetIdPolicy::new(Dialect::Claude))
            .is_ok()
    );
    let source = response(
        json!([{"type":"message","id":"msg","role":"assistant","status":"in_progress","content":[]}]),
        "completed",
    );
    assert!(
        responses_to_claude_response(source, &mut flow(), &TargetIdPolicy::new(Dialect::Claude))
            .is_err()
    );
}
#[test]
fn mcp_native_execution_maps_call_and_success_or_failure_result_both_ways() {
    for is_error in [false, true] {
        let source = claude(
            json!([{"type":"mcp_tool_use","id":"mcp-native","name":"lookup","server_name":"server","input":{"q":1}},
            {"type":"mcp_tool_result","tool_use_id":"mcp-native","is_error":is_error,"content":"result"}]),
            "end_turn",
        );
        let mapped = claude_to_responses_response(
            source,
            context(),
            &mut flow(),
            &TargetIdPolicy::new(Dialect::OpenAi),
        )
        .unwrap()
        .value;
        assert_eq!(mapped.output.len(), 1);
        let r::ResponseOutputItem::McpCall(call) = &mapped.output[0] else {
            panic!()
        };
        assert_eq!(call.id, "mcp-native");
        assert_eq!(
            call.status,
            Some(if is_error {
                r::McpCallStatus::Failed
            } else {
                r::McpCallStatus::Completed
            })
        );
        let back = responses_to_claude_response(
            mapped,
            &mut flow(),
            &TargetIdPolicy::new(Dialect::Claude),
        )
        .unwrap()
        .value;
        assert_eq!(back.stop_reason, c::StopReason::EndTurn);
        assert_eq!(back.content.len(), 2);
        let c::ResponseContentBlock::McpToolResult(result) = &back.content[1] else {
            panic!()
        };
        assert_eq!(result.tool_use_id, "mcp-native");
        assert_eq!(result.is_error, is_error);
    }
}

#[test]
fn native_thinking_restoration_preserves_content_and_clears_extensions() {
    use gproxy_protocol::transform::identity::{
        IdentityRole, IdentityStateRecord, IdentityTarget, OpaqueSignature, OutputItemKind,
    };
    fn native_context() -> ClaudeRequestContext {
        let target = IdentityTarget::new("actual-model", Dialect::Claude)
            .unwrap()
            .with_origin("original-upstream")
            .unwrap();
        let mut state = IdentityStateRecord::new(
            IdentityRole::OutputItem(OutputItemKind::Reasoning),
            target.clone(),
        );
        state.client_item_id = Some("rs-1".into());
        state.opaque_signature = Some(
            OpaqueSignature::new(
                gproxy_protocol::transform::identity::OpaqueField::ClaudeThinkingSignature,
                "native-signature",
                "original-upstream",
                "actual-model",
            )
            .unwrap(),
        );
        ClaudeRequestContext{target:Some(target),restored_thinking:std::collections::BTreeMap::from([("rs-1".into(),RestoredClaudeThinking{state,block:serde_json::from_value(json!({"type":"thinking","thinking":"original thought","signature":"native-signature","foreign":true})).unwrap()})])}
    }
    let output = json!([{"type":"reasoning","id":"rs-1","summary":[],"content":[{"type":"reasoning_text","text":"original thought","foreign":true}],"status":"completed"}]);
    let result = responses_to_claude_response_with_context(
        response(output.clone(), "completed"),
        native_context(),
        &mut flow(),
        &TargetIdPolicy::new(Dialect::Claude),
    )
    .unwrap()
    .value;
    let c::ResponseContentBlock::Thinking(block) = &result.content[0] else {
        panic!()
    };
    assert_eq!(block.signature, "native-signature");
    assert!(block.rest.is_empty());
    let mut modified = output.clone();
    modified[0]["content"][0]["text"] = json!("tampered");
    assert!(
        responses_to_claude_response_with_context(
            response(modified, "completed"),
            native_context(),
            &mut flow(),
            &TargetIdPolicy::new(Dialect::Claude)
        )
        .is_err()
    );
}
