use gproxy_protocol::{
    claude::{count_tokens as cc, generate_content as cg},
    openai::chat,
    transform::generate::claude_chat,
};
use serde_json::json;

#[test]
fn tool_references_preserve_names_schema_and_mixed_result_order() {
    for mixed in [false, true] {
        let mut result = vec![
            json!({"type":"tool_reference","tool_name":"lookup","cache_control":{"type":"ephemeral"},"foreign":"DROP"}),
        ];
        if mixed {
            result.insert(0, json!({"type":"text","text":"Found: "}));
            result.push(json!({"type":"text","text":"; use it."}));
        }
        let input: cg::GenerateContentRequestBody = serde_json::from_value(json!({
            "model":"claude", "max_tokens":32,
            "tools":[{"name":"lookup","input_schema":{"type":"object","properties":{"q":{"type":"string"}}}}],
            "messages":[
                {"role":"assistant","content":[{"type":"tool_use","id":"call-search","name":"discover","input":{}}]},
                {"role":"user","content":[{"type":"tool_result","tool_use_id":"call-search","content":result}]}
            ]
        })).unwrap();
        let converted = claude_chat::claude_to_openai(&input, "target").unwrap();
        let wire = serde_json::to_value(converted.value).unwrap();
        let reference = json!({"type":"tool_reference","tool_name":"lookup"}).to_string();
        assert_eq!(
            wire["messages"][1]["content"],
            if mixed {
                format!("Found: {reference}; use it.")
            } else {
                reference
            }
        );
        assert_eq!(wire["messages"][1]["tool_call_id"], "call-search");
        assert_eq!(wire["tools"][0]["function"]["name"], "lookup");
        assert_eq!(
            wire["tools"][0]["function"]["parameters"]["properties"]["q"]["type"],
            "string"
        );
        assert!(!wire.to_string().contains("DROP"));
        assert!(
            converted
                .report
                .diagnostics
                .iter()
                .any(|d| d.field == "tool_result.tool_reference")
        );
        let mut deferred = input.clone();
        let gproxy_protocol::claude::tools::ToolUnion::Custom(tool) =
            &mut deferred.tools.as_mut().unwrap()[0]
        else {
            panic!()
        };
        tool.defer_loading = Some(true);
        assert!(
            claude_chat::claude_to_openai(&deferred, "target").is_err(),
            "reference text alone must not silently enable deferred tools"
        );
    }
}

#[test]
fn claude_request_maps_roles_tools_and_nested_schema_without_rest_leakage() {
    let input: cg::GenerateContentRequestBody = serde_json::from_value(json!({
        "max_tokens": 128,
        "model": "claude-source",
        "system": "ignored top-level system",
        "messages": [
            {"role":"system","content":"system text"},
            {"role":"user","content":[{"type":"text","text":"hello"}]},
            {"role":"assistant","content":[{"type":"tool_use","id":"call-1","name":"lookup","input":{"q":"x"}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"call-1","content":"answer"}]}
        ],
        "tools":[{"name":"lookup","description":"find","input_schema":{"type":"object","properties":{"q":{"type":"string","x-unknown":{"deep":true}}},"required":["q"]}}],
        "unknown":{"deep":{"value":true}}
    })).unwrap();
    let converted = claude_chat::claude_to_openai(&input, "gpt-target").unwrap();
    assert_eq!(converted.value.model, "gpt-target");
    assert!(converted.value.rest.is_empty());
    assert_eq!(converted.value.messages.len(), 5);
    assert!(converted.value.tools.is_some());
    let tool = converted.value.tools.unwrap();
    let chat::ChatTool::Function(tool) = &tool[0] else {
        panic!("expected function tool")
    };
    assert_eq!(
        tool.function.parameters.as_ref().unwrap()["properties"]["q"]["type"],
        "string"
    );
    assert_eq!(
        tool.function.parameters.as_ref().unwrap()["properties"]["q"]["x-unknown"]["deep"],
        true
    );
}

#[test]
fn openai_request_maps_tool_results_images_and_malformed_arguments() {
    let input: chat::GenerateContentRequestBody = serde_json::from_value(json!({
        "model":"gpt-source","max_tokens":64,
        "messages":[
            {"role":"developer","content":"rules"},
            {"role":"assistant","content":null,"tool_calls":[{"id":"call-x","type":"function","function":{"name":"lookup","arguments":"{\"q\":\"x\"}"}}]},
            {"role":"tool","tool_call_id":"call-x","content":"done"},
            {"role":"user","content":[{"type":"text","text":"look"},{"type":"image_url","image_url":{"url":"https://example/image.png"}}]}
        ],
        "tools":[{"type":"function","function":{"name":"lookup","parameters":{"type":"object","properties":{"q":{"type":"string"}}}}}],
        "unknown":{"nested":{"must_not_copy":true}}
    })).unwrap();
    let converted = claude_chat::openai_to_claude(&input, "claude-target").unwrap();
    assert_eq!(converted.value.model, "claude-target");
    assert!(converted.value.rest.is_empty());
    assert_eq!(converted.value.messages.len(), 3);
    assert!(converted.value.system.is_some());
    let encoded = serde_json::to_value(&converted.value).unwrap();
    assert!(encoded.get("unknown").is_none());
    let text = encoded.to_string();
    assert!(text.contains("tool_use"));
    assert!(text.contains("tool_result"));
    assert!(text.contains("https://example/image.png"));
}

#[test]
fn malformed_request_arguments_are_invalid_input() {
    let input: chat::GenerateContentRequestBody = serde_json::from_value(json!({
        "model":"gpt","max_tokens":8,
        "messages":[{"role":"assistant","content":null,"tool_calls":[{"id":"call-x","type":"function","function":{"name":"lookup","arguments":"[]"}}]}]
    })).unwrap();
    let error = claude_chat::openai_to_claude(&input, "claude").unwrap_err();
    assert_eq!(
        error.kind(),
        gproxy_protocol::transform::TransformErrorKind::InvalidInput
    );
}

#[test]
fn document_resource_is_explicitly_rejected() {
    let input: cg::GenerateContentRequestBody = serde_json::from_value(json!({
        "max_tokens": 8,"model":"claude","messages":[{"role":"user","content":[{"type":"document","source":{"type":"url","url":"https://example/doc.pdf"}}]}]
    })).unwrap();
    let error = claude_chat::claude_to_openai(&input, "gpt").unwrap_err();
    assert_eq!(
        error.kind(),
        gproxy_protocol::transform::TransformErrorKind::MissingMetadata
    );
    assert_eq!(error.context(), "document.source");
}

#[test]
fn same_name_parallel_calls_keep_distinct_ids() {
    let input: cg::GenerateContentRequestBody = serde_json::from_value(json!({
        "max_tokens":16,"model":"claude","messages":[{"role":"assistant","content":[
            {"type":"tool_use","id":"call-a","name":"lookup","input":{"q":"a"}},
            {"type":"tool_use","id":"call-b","name":"lookup","input":{"q":"b"}}
        ]}]
    }))
    .unwrap();
    let converted = claude_chat::claude_to_openai(&input, "gpt").unwrap();
    let chat::ChatMessage::Assistant(message) = &converted.value.messages[0] else {
        panic!("expected assistant")
    };
    let calls = message.tool_calls.as_ref().unwrap();
    assert_eq!(calls.len(), 2);
    let ids: Vec<_> = calls
        .iter()
        .map(|call| match call {
            chat::MessageToolCall::Function(call) => call.id.as_str(),
            _ => "",
        })
        .collect();
    assert_eq!(ids, ["call-a", "call-b"]);
}

#[test]
fn local_document_data_is_converted_without_resource_capability() {
    let input: cg::GenerateContentRequestBody = serde_json::from_value(json!({
        "max_tokens":16,"model":"claude","messages":[{"role":"user","content":[
            {"type":"document","title":"x.pdf","source":{"type":"base64","media_type":"application/pdf","data":"AQI="}}
        ]}]
    })).unwrap();
    let converted = claude_chat::claude_to_openai(&input, "gpt").unwrap();
    let value = serde_json::to_value(&converted.value).unwrap();
    assert!(
        value
            .to_string()
            .contains("data:application/pdf;base64,AQI=")
    );

    let chat_input: chat::GenerateContentRequestBody = serde_json::from_value(json!({
        "model":"gpt","max_tokens":16,"messages":[{"role":"user","content":[{"type":"file","file":{"file_data":"data:text/plain;base64,aGVsbG8=","filename":"x.txt"}}]}]
    })).unwrap();
    let back = claude_chat::openai_to_claude(&chat_input, "claude").unwrap();
    assert!(
        serde_json::to_value(&back.value)
            .unwrap()
            .to_string()
            .contains("text/plain")
    );
}

#[test]
fn forced_tool_parallel_schema_and_reasoning_fields_map_explicitly() {
    let claude_input: cg::GenerateContentRequestBody = serde_json::from_value(json!({
        "max_tokens":32,"model":"claude","tool_choice":{"type":"tool","name":"lookup","disable_parallel_tool_use":true},
        "output_config":{"effort":"high","format":{"type":"json_schema","schema":{"type":"object","properties":{"x":{"type":"string"}}}}},
        "messages":[{"role":"user","content":"x"}],"tools":[{"name":"lookup","input_schema":{"type":"object"}}]
    })).unwrap();
    let chat = claude_chat::claude_to_openai(&claude_input, "gpt")
        .unwrap()
        .value;
    assert!(matches!(
        chat.tool_choice,
        Some(chat::ToolChoice::Function(_))
    ));
    assert_eq!(chat.parallel_tool_calls, Some(false));
    assert!(matches!(
        chat.response_format,
        Some(chat::ResponseFormat::JsonSchema(_))
    ));
    assert_eq!(
        chat.reasoning_effort,
        Some(Some(chat::ReasoningEffort::High))
    );

    let openai: chat::GenerateContentRequestBody = serde_json::from_value(json!({
        "model":"gpt","max_tokens":32,"parallel_tool_calls":false,"reasoning_effort":"medium",
        "tool_choice":{"type":"function","function":{"name":"lookup"}},
        "response_format":{"type":"json_schema","json_schema":{"name":"result","schema":{"type":"object","properties":{"x":{"type":"string"}}},"strict":true}},
        "messages":[{"role":"user","content":"x"}],"tools":[{"type":"function","function":{"name":"lookup","parameters":{"type":"object"}}}]
    })).unwrap();
    let claude = claude_chat::openai_to_claude(&openai, "claude")
        .unwrap()
        .value;
    assert!(matches!(claude.tool_choice, Some(cc::ToolChoice::Tool(_))));
    assert!(claude.output_format.is_some());
    assert_eq!(
        claude.output_config.as_ref().unwrap().effort,
        Some(cc::Effort::Medium)
    );
}

#[test]
fn n_greater_than_one_requires_adaptation_diagnostic() {
    let input: chat::GenerateContentRequestBody = serde_json::from_value(json!({
        "model":"gpt","max_tokens":8,"n":2,"messages":[{"role":"user","content":"x"}]
    }))
    .unwrap();
    let error = claude_chat::openai_to_claude(&input, "claude").unwrap_err();
    assert_eq!(
        error.kind(),
        gproxy_protocol::transform::TransformErrorKind::Unsupported
    );
}

#[test]
fn text_modalities_none_reasoning_and_formal_schema_keywords_survive() {
    let input:chat::GenerateContentRequestBody=serde_json::from_value(json!({
        "model":"source","max_completion_tokens":20,"modalities":["text"],"reasoning_effort":"none",
        "messages":[{"role":"developer","content":"rules"},{"role":"user","content":"hello"}],
        "tools":[{"type":"function","function":{"name":"f","parameters":{"type":"object","properties":{"x":{"$ref":"#/$defs/X"}},"$defs":{"X":{"type":"string","x-vendor-formal":true}},"additionalProperties":false,"required":["x"],"minProperties":1}}}]
    })).unwrap();
    let converted = claude_chat::openai_to_claude(&input, "target")
        .unwrap()
        .value;
    assert!(matches!(
        converted.thinking,
        Some(cc::ThinkingConfig::Disabled(_))
    ));
    assert_eq!(converted.messages.len(), 1);
    let wire = serde_json::to_value(&converted).unwrap();
    assert_eq!(wire["system"], "rules");
    assert_eq!(
        wire["tools"][0]["input_schema"]["additionalProperties"],
        false
    );
    assert_eq!(
        wire["tools"][0]["input_schema"]["$defs"]["X"]["x-vendor-formal"],
        true
    );
    let back = claude_chat::claude_to_openai(&converted, "source")
        .unwrap()
        .value;
    assert_eq!(
        back.reasoning_effort,
        Some(Some(chat::ReasoningEffort::None))
    );
    let back = serde_json::to_value(back).unwrap();
    assert_eq!(
        back["tools"][0]["function"]["parameters"]["minProperties"],
        1
    );
}

#[test]
fn refusal_parts_and_tool_result_order_survive_without_extension_content() {
    let input:chat::GenerateContentRequestBody=serde_json::from_value(json!({"model":"source","max_tokens":10,"messages":[{"role":"assistant","content":[{"type":"text","text":"a","foreign":"secret"},{"type":"refusal","refusal":"b"}],"refusal":"c"}]})).unwrap();
    let output = claude_chat::openai_to_claude(&input, "target")
        .unwrap()
        .value;
    let encoded = serde_json::to_value(output).unwrap();
    assert_eq!(encoded["messages"][0]["content"][1]["text"], "b");
    assert_eq!(encoded["messages"][0]["content"][2]["text"], "c");
    assert!(!encoded.to_string().contains("secret"));
    let input:cg::GenerateContentRequestBody=serde_json::from_value(json!({"model":"source","max_tokens":10,"messages":[{"role":"user","content":[{"type":"tool_result","tool_use_id":"id","content":"answer"},{"type":"text","text":"follow up"}]}]})).unwrap();
    let output = claude_chat::claude_to_openai(&input, "target")
        .unwrap()
        .value;
    assert!(matches!(output.messages[0], chat::ChatMessage::Tool(_)));
    assert!(matches!(output.messages[1], chat::ChatMessage::User(_)));
}

#[test]
fn malformed_schema_and_nontext_tool_results_cannot_disappear() {
    for parameters in [
        json!({"type":"array"}),
        json!({"type":"object","required":"bad"}),
        json!({"type":"object","unknown_root_keyword":true}),
    ] {
        let input:chat::GenerateContentRequestBody=serde_json::from_value(json!({"model":"source","max_tokens":10,"messages":[{"role":"user","content":"x"}],"tools":[{"type":"function","function":{"name":"f","parameters":parameters}}]})).unwrap();
        assert!(claude_chat::openai_to_claude(&input, "target").is_err());
    }
    let input:cg::GenerateContentRequestBody=serde_json::from_value(json!({"model":"source","max_tokens":10,"messages":[{"role":"user","content":[{"type":"tool_result","tool_use_id":"id","content":[{"type":"image","source":{"type":"url","url":"https://example/image.png"}}]}]}]})).unwrap();
    assert!(claude_chat::claude_to_openai(&input, "target").is_err());
}

#[test]
fn legacy_functions_and_service_tier_convert_but_free_json_object_needs_adapter() {
    let input:chat::GenerateContentRequestBody=serde_json::from_value(json!({"model":"source","max_tokens":20,"service_tier":"default","functions":[{"name":"f","parameters":{"type":"object","additionalProperties":false}}],"function_call":{"name":"f"},"messages":[{"role":"user","content":"x"}]})).unwrap();
    let output = claude_chat::openai_to_claude(&input, "target")
        .unwrap()
        .value;
    assert!(matches!(output.tool_choice, Some(cc::ToolChoice::Tool(_))));
    assert!(matches!(
        output.service_tier,
        Some(cg::ServiceTier::StandardOnly)
    ));
    assert!(output.output_format.is_none());
    assert_eq!(output.tools.unwrap().len(), 1);
    let mut input = input;
    input.response_format = Some(chat::ResponseFormat::JsonObject(
        chat::JsonObjectResponseFormat::builder(chat::JsonObjectResponseType::JsonObject).build(),
    ));
    assert!(claude_chat::openai_to_claude(&input, "target").is_err());
}

#[test]
fn native_call_identity_is_preserved_and_empty_identity_is_rejected() {
    let mut input:chat::GenerateContentRequestBody=serde_json::from_value(json!({"model":"source","max_tokens":10,"messages":[{"role":"assistant","tool_calls":[{"id":"native-id","type":"function","function":{"name":"f","arguments":"{}"}}]},{"role":"tool","tool_call_id":"native-id","content":"result"}]})).unwrap();
    let output = claude_chat::openai_to_claude(&input, "target")
        .unwrap()
        .value;
    let wire = serde_json::to_value(output).unwrap();
    assert_eq!(wire["messages"][0]["content"][0]["id"], "native-id");
    assert_eq!(
        wire["messages"][1]["content"][0]["tool_use_id"],
        "native-id"
    );
    let chat::ChatMessage::Tool(result) = &mut input.messages[1] else {
        panic!()
    };
    result.tool_call_id.clear();
    assert!(claude_chat::openai_to_claude(&input, "target").is_err());
}
