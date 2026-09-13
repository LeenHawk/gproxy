use gproxy_protocol::transform::generate::chat_responses::*;
use gproxy_protocol::transform::identity::{IdNamespace, IdentityFlow, TargetIdPolicy};
use gproxy_protocol::wire::openai::{chat, responses};
use serde_json::json;
fn chat_to_responses_request(
    input: chat::GenerateContentRequestBody,
    model: &str,
) -> Result<
    gproxy_protocol::transform::Converted<responses::GenerateContentRequestBody>,
    gproxy_protocol::transform::TransformError,
> {
    let mut flow = IdentityFlow::new(IdNamespace::with_bytes([31; 16]));
    gproxy_protocol::transform::generate::chat_responses::chat_to_responses_request(
        input,
        model,
        &mut flow,
        &TargetIdPolicy::new(gproxy_protocol::Dialect::OpenAi),
    )
}

#[test]
fn chat_request_maps_roles_tools_and_keeps_call_id_distinct_from_item_id() {
    let input: chat::GenerateContentRequestBody = serde_json::from_value(json!({
        "model":"gpt-chat",
        "messages":[
            {"role":"system","content":"rules"},
            {"role":"user","content":"hello"},
            {"role":"assistant","content":null,"tool_calls":[{"id":"call-1","type":"function","function":{"name":"lookup","arguments":"{\"q\":1}"}}]},
            {"role":"tool","tool_call_id":"call-1","content":"result"}
        ],
        "future_source":true
    })).unwrap();
    let converted = chat_to_responses_request(input, "gpt-responses").unwrap();
    let wire = serde_json::to_value(converted.value).unwrap();
    let items = wire["input"].as_array().unwrap();
    assert_eq!(items[0]["content"], "rules");
    assert_eq!(items[0]["role"], "system");
    assert_eq!(items[1]["role"], "user");
    assert_eq!(items[2]["type"], "function_call");
    assert_eq!(items[2]["call_id"], "call-1");
    assert!(items[2].get("id").is_none());
    assert_eq!(items[3]["type"], "function_call_output");
    assert!(wire.get("future_source").is_none());
}

#[test]
fn responses_request_maps_text_and_requires_history_state_for_previous_id() {
    let input: responses::GenerateContentRequestBody = serde_json::from_value(json!({
        "model":"gpt-responses",
        "input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}]
    }))
    .unwrap();
    let converted = responses_to_chat_request(input, "target-chat").unwrap();
    let wire = serde_json::to_value(converted.value).unwrap();
    assert_eq!(wire["messages"][0]["role"], "user");
    assert_eq!(wire["model"], "target-chat");

    let input: responses::GenerateContentRequestBody = serde_json::from_value(json!({
        "model":"gpt-responses","previous_response_id":"resp-old","input":"next"
    }))
    .unwrap();
    assert!(responses_to_chat_request(input, "target-chat").is_err());
}

#[test]
fn multiple_system_messages_controls_schema_custom_tools_and_raw_calls_roundtrip() {
    let input:chat::GenerateContentRequestBody=serde_json::from_value(json!({
        "model":"source","messages":[{"role":"system","content":"one"},{"role":"system","content":"two"},{"role":"developer","content":"three"},{"role":"assistant","tool_calls":[{"type":"custom","id":"custom-id","custom":{"name":"grammar","input":"not-json"}},{"type":"function","id":"function-id","function":{"name":"f","arguments":"{broken"}}]},{"role":"tool","tool_call_id":"custom-id","content":"done"}],
        "temperature":0.25,"top_p":0.75,"max_completion_tokens":20,"reasoning_effort":"none","verbosity":"low","service_tier":"priority","prompt_cache_options":{"mode":"explicit","ttl":"30m","foreign":1},"prompt_cache_retention":"24h",
        "response_format":{"type":"json_schema","json_schema":{"name":"answer","schema":{"type":"object","foreign_schema_data":1},"strict":null}},
        "tools":[{"type":"custom","custom":{"name":"grammar","format":{"type":"grammar","grammar":{"syntax":"regex","definition":"[a-z]+"}}}},{"type":"function","function":{"name":"f","parameters":{"type":"object","foreign":true}}}],"tool_choice":{"type":"custom","custom":{"name":"grammar"}},
        "moderation":{"model":"moderation","policy":{"input":{"mode":"block"}}},"foreign":99
    })).unwrap();
    let converted = chat_to_responses_request(input, "target").unwrap().value;
    let wire = serde_json::to_value(&converted).unwrap();
    assert_eq!(wire["input"][0]["content"], "one");
    assert_eq!(wire["input"][1]["content"], "two");
    assert_eq!(wire["input"][3]["type"], "custom_tool_call");
    assert_eq!(wire["input"][4]["arguments"], "{broken");
    assert_eq!(wire["input"][5]["type"], "custom_tool_call_output");
    assert!(wire.get("foreign").is_none());
    assert!(wire["prompt_cache_options"].get("foreign").is_none());
    let back = responses_to_chat_request(converted, "target-chat")
        .unwrap()
        .value;
    let wire = serde_json::to_value(back).unwrap();
    assert_eq!(wire["temperature"], 0.25);
    assert_eq!(wire["top_p"], 0.75);
    assert_eq!(wire["reasoning_effort"], "none");
    assert_eq!(
        wire["response_format"]["json_schema"]["strict"],
        serde_json::Value::Null
    );
    assert_eq!(
        wire["tools"][0]["custom"]["format"]["grammar"]["definition"],
        "[a-z]+"
    );
    assert_eq!(wire["moderation"]["policy"]["input"]["mode"], "block");
    assert_eq!(wire["prompt_cache_retention"], "24h");
}

#[test]
fn multimodal_both_directions_and_output_refusal_history_are_typed() {
    let input:responses::GenerateContentRequestBody=serde_json::from_value(json!({"model":"target","input":[{"role":"user","content":[{"type":"input_text","text":"look"},{"type":"input_image","detail":"high","image_url":"https://example/image"},{"type":"input_file","file_id":"file-1","filename":"f.pdf"}]},{"type":"message","id":"msg-1","role":"assistant","status":"completed","content":[{"type":"output_text","text":"text","annotations":[],"logprobs":[]},{"type":"refusal","refusal":"no"}]}]})).unwrap();
    let converted = responses_to_chat_request(input, "target-chat")
        .unwrap()
        .value;
    let wire = serde_json::to_value(&converted).unwrap();
    assert_eq!(
        wire["messages"][0]["content"][1]["image_url"]["detail"],
        "high"
    );
    assert_eq!(
        wire["messages"][0]["content"][2]["file"]["file_id"],
        "file-1"
    );
    assert_eq!(wire["messages"][1]["content"][1]["type"], "refusal");
    let back = chat_to_responses_request(converted, "target")
        .unwrap()
        .value;
    let wire = serde_json::to_value(back).unwrap();
    assert_eq!(
        wire["input"][0]["content"][1]["image_url"],
        "https://example/image"
    );
    assert_eq!(wire["input"][0]["content"][2]["file_id"], "file-1");
}

#[test]
fn null_sampling_and_controls_do_not_become_defaults() {
    let input:chat::GenerateContentRequestBody=serde_json::from_value(json!({"model":"source","messages":[{"role":"user","content":"x"}],"temperature":null,"top_p":null,"max_completion_tokens":null,"reasoning_effort":null,"stop":null,"stream_options":null})).unwrap();
    let converted = chat_to_responses_request(input, "target").unwrap().value;
    assert_eq!(converted.temperature, Some(None));
    assert_eq!(converted.max_output_tokens, Some(None));
    let back = responses_to_chat_request(converted, "target-chat")
        .unwrap()
        .value;
    assert_eq!(back.temperature, Some(None));
    assert_eq!(back.top_p, Some(None));
    assert_eq!(back.reasoning_effort, Some(None));
    assert_eq!(back.stream_options, Some(None));
}

#[test]
fn refusal_identity_is_typed_and_late_errors_do_not_publish_identity() {
    use gproxy_protocol::transform::identity::IdentityRole;
    let source: chat::GenerateContentRequestBody = serde_json::from_value(
        json!({"model":"source","messages":[{"role":"assistant","refusal":"no"}]}),
    )
    .unwrap();
    let mut ids = IdentityFlow::new(IdNamespace::with_bytes([33; 16]));
    let policy = TargetIdPolicy::new(gproxy_protocol::Dialect::OpenAi);
    let converted =
        gproxy_protocol::transform::generate::chat_responses::chat_to_responses_request(
            source.clone(),
            "target",
            &mut ids,
            &policy,
        )
        .unwrap()
        .value;
    let wire = serde_json::to_value(converted).unwrap();
    assert_eq!(wire["input"][0]["content"][0]["type"], "refusal");
    assert!(!wire["input"][0]["id"].as_str().unwrap().is_empty());
    let mut bad = source;
    bad.messages.push(serde_json::from_value(json!({"role":"user","content":[{"type":"input_audio","input_audio":{"data":"AQI=","format":"wav"}}]})).unwrap());
    let mut failed = IdentityFlow::new(IdNamespace::with_bytes([34; 16]));
    assert!(
        gproxy_protocol::transform::generate::chat_responses::chat_to_responses_request(
            bad,
            "target",
            &mut failed,
            &policy
        )
        .is_err()
    );
    assert!(
        failed
            .lookup_logical(
                IdentityRole::Message,
                &gproxy_protocol::Dialect::OpenAiChat,
                0
            )
            .is_none()
    );
}

#[test]
fn allowed_selector_shape_and_rewritten_tool_result_identity_are_preserved() {
    use gproxy_protocol::transform::identity::IdSyntax;
    let input:chat::GenerateContentRequestBody=serde_json::from_value(json!({"model":"source","messages":[{"role":"assistant","tool_calls":[{"type":"function","id":"a/b","function":{"name":"f","arguments":"bad-json"}}]},{"role":"tool","tool_call_id":"a/b","content":"result"}],"tool_choice":{"type":"allowed_tools","allowed_tools":{"mode":"required","tools":[{"type":"function","function":{"name":"f"}}]}}})).unwrap();
    let mut ids = IdentityFlow::new(IdNamespace::with_bytes([37; 16]));
    let policy = TargetIdPolicy::new(gproxy_protocol::Dialect::OpenAi)
        .with_syntax(IdSyntax::AsciiIdentifier);
    let output = gproxy_protocol::transform::generate::chat_responses::chat_to_responses_request(
        input, "target", &mut ids, &policy,
    )
    .unwrap()
    .value;
    let wire = serde_json::to_value(&output).unwrap();
    assert_eq!(wire["tool_choice"]["tools"][0]["name"], "f");
    assert_ne!(wire["input"][0]["call_id"], "a/b");
    assert_eq!(wire["input"][0]["call_id"], wire["input"][1]["call_id"]);
    let back = serde_json::to_value(
        responses_to_chat_request(output, "target-chat")
            .unwrap()
            .value,
    )
    .unwrap();
    assert_eq!(
        back["tool_choice"]["allowed_tools"]["tools"][0]["function"]["name"],
        "f"
    );
}

#[test]
fn legacy_function_history_gets_scoped_call_result_binding() {
    let input:chat::GenerateContentRequestBody=serde_json::from_value(json!({"model":"source","functions":[{"name":"f","parameters":{"type":"object"}}],"function_call":{"name":"f"},"messages":[{"role":"assistant","function_call":{"name":"f","arguments":"raw-invalid"}},{"role":"function","name":"f","content":"result"}]})).unwrap();
    let out =
        serde_json::to_value(chat_to_responses_request(input, "target").unwrap().value).unwrap();
    assert_eq!(out["tools"][0]["name"], "f");
    assert_eq!(out["tool_choice"]["name"], "f");
    assert_eq!(out["input"][0]["arguments"], "raw-invalid");
    assert_eq!(out["input"][0]["call_id"], out["input"][1]["call_id"]);
}

#[test]
fn selected_chat_model_does_not_require_a_source_model() {
    let input: responses::GenerateContentRequestBody =
        serde_json::from_value(json!({"input":"hello"})).unwrap();
    assert!(responses_to_chat_request(input.clone(), "").is_err());
    assert_eq!(
        responses_to_chat_request(input, "selected-chat")
            .unwrap()
            .value
            .model,
        "selected-chat"
    );
}
