use gproxy_protocol::openai::count_tokens::*;
use gproxy_protocol::{WireRequest, WireResponse};
use http::{HeaderMap, Method, StatusCode};

#[test]
fn count_tokens_request_round_trips_multimodal_history_and_unknown_fields() {
    let wire = serde_json::json!({
        "model": "gpt-5",
        "input": [
            {"type":"message", "role":"user", "content":[
                {"type":"input_text", "text":"describe this"},
                {"type":"input_image", "detail":"high", "image_url":"data:image/png;base64,AA=="}
            ]},
            {"type":"function_call", "id":"fc_1", "call_id":"call_1", "name":"weather", "arguments":"{}", "future":true}
        ],
        "previous_response_id":"resp_1",
        "future_request_field":{"kept":true}
    });
    let body: CountTokensRequestBody = serde_json::from_value(wire.clone()).unwrap();
    let request = WireRequest {
        method: Method::POST,
        path: "/responses/input_tokens".into(),
        query: Some("v=1".into()),
        headers: HeaderMap::new(),
        body,
    };
    assert_eq!(request.method, Method::POST);
    assert_eq!(request.path, "/responses/input_tokens");
    assert_eq!(request.query.as_deref(), Some("v=1"));
    let output = serde_json::to_value(request.body).unwrap();
    assert_eq!(output, wire);
    assert_eq!(output["model"], "gpt-5");
    assert_eq!(output["future_request_field"]["kept"], true);
    assert_eq!(
        output["input"][0]["content"][1]["image_url"],
        "data:image/png;base64,AA=="
    );
    assert_eq!(output["input"][1]["future"], true);
}

#[test]
fn count_tokens_response_and_optional_fields() {
    let body: CountTokensResponseBody = serde_json::from_value(serde_json::json!({
        "object":"response.input_tokens", "input_tokens":123, "new_field":"future"
    }))
    .unwrap();
    let response = WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        body,
    };
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        serde_json::to_value(response.body).unwrap(),
        serde_json::json!({
            "object":"response.input_tokens", "input_tokens":123, "new_field":"future"
        })
    );
    let request = CountTokensRequestBody::builder().build();
    assert_eq!(
        serde_json::to_value(request).unwrap(),
        serde_json::json!({})
    );
}

#[test]
fn typed_input_and_tool_choice_keep_native_tags_without_round_trip_setup() {
    let text = InputContent::Text(
        ResponseInputText::builder(ResponseInputTextType::ResponseInputText, "hello".into())
            .build(),
    );
    assert_eq!(
        serde_json::to_value(text).unwrap(),
        serde_json::json!({
            "type": "input_text", "text": "hello"
        })
    );
    let function = InputItem::FunctionCall(
        FunctionCall::builder(
            FunctionCallType::FunctionCall,
            "{}".into(),
            "call_1".into(),
            "lookup".into(),
        )
        .build(),
    );
    assert_eq!(
        serde_json::to_value(function).unwrap(),
        serde_json::json!({
            "type": "function_call", "arguments": "{}", "call_id": "call_1", "name": "lookup"
        })
    );
    let choice: ToolChoice = serde_json::from_value(serde_json::json!({
        "type":"function", "name":"lookup"
    }))
    .unwrap();
    assert!(matches!(choice, ToolChoice::Function(_)));
    assert_eq!(
        serde_json::to_value(choice).unwrap(),
        serde_json::json!({
            "type":"function", "name":"lookup"
        })
    );
    let request: CountTokensRequestBody = serde_json::from_value(serde_json::json!({
        "input":[{"role":"user", "content":"hello"}],
        "tools":[{"type":"function", "name":"lookup", "parameters":{}, "strict":null}]
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(request).unwrap(),
        serde_json::json!({
            "input":[{"role":"user", "content":"hello"}],
            "tools":[{"type":"function", "name":"lookup", "parameters":{}, "strict":null}]
        })
    );
}

#[test]
fn implemented_extended_response_input_items_round_trip() {
    let items = serde_json::json!([
        {"type":"tool_search_call","id":"ts1","arguments":"{}"},
        {"type":"tool_search_output","id":"ts1","tools":[]},
        {"type":"local_shell_call","id":"ls1","call_id":"c1","action":{"command":["pwd"],"env":{},"type":"exec","timeout_ms":10},"status":"completed"},
        {"type":"local_shell_call_output","id":"ls1","output":"{}","status":"completed"},
        {"type":"shell_call","call_id":"s1","action":{"commands":["echo hi"]}},
        {"type":"shell_call_output","call_id":"s1","output":[{"stdout":"","stderr":"","outcome":{"type":"exit","exit_code":0}}],"status":"completed"},
        {"type":"apply_patch_call","call_id":"p1","status":"completed","operation":{"type":"create_file","path":"a","diff":"*** Begin Patch"}},
        {"type":"apply_patch_call_output","call_id":"p1","status":"completed","output":"ok"},
        {"type":"mcp_approval_request","id":"a1","arguments":"{}","name":"read","server_label":"docs"},
        {"type":"mcp_approval_response","approval_request_id":"a1","approve":true},
        {"type":"mcp_call","id":"m1","arguments":"{}","name":"read","server_label":"docs"},
        {"type":"compaction_trigger"},
        {"type":"program","id":"pr1","call_id":"c1","code":"1","fingerprint":"fp"},
        {"type":"program_output","id":"pr1","call_id":"c1","result":"1","status":"completed"}
    ]);
    let parsed: Vec<InputItem> = serde_json::from_value(items.clone()).unwrap();
    let encoded = serde_json::to_value(parsed).unwrap();
    assert_eq!(encoded, items);
}

#[test]
fn implemented_tool_envelopes_round_trip() {
    let tools = serde_json::json!([
        {"type":"function","name":"f","parameters":{"type":"object"},"strict":true,"output_schema":{"type":"string"},"defer_loading":true},
        {"type":"file_search","vector_store_ids":["vs"],"ranking_options":{"score_threshold":0.5,"ranker":"auto"}},
        {"type":"web_search_preview","search_context_size":"medium","search_content_types":["text"],"user_location":{"type":"approximate","city":"Shanghai"}},
        {"type":"computer_use_preview","display_width":100,"display_height":80,"environment":"linux"},
        {"type":"code_interpreter","container":"auto"},
        {"type":"custom","name":"paint","format":{"type":"grammar","definition":"[a-z]+","syntax":"regex"}},
        {"type":"namespace","name":"ns","description":"d","tools":[]},
        {"type":"shell","environment":{"type":"local"}},
        {"type":"mcp","server_label":"docs","connector_id":"connector_gmail"},
        {"type":"tool_search","execution":"server","parameters":{"type":"object"}},
        {"type":"image_generation","quality":"high","partial_images":1}
    ]);
    let parsed: Vec<Tool> = serde_json::from_value(tools.clone()).unwrap();
    let encoded = serde_json::to_value(parsed).unwrap();
    for (index, value) in tools.as_array().unwrap().iter().enumerate() {
        assert_eq!(encoded[index], *value);
    }
}

#[test]
fn required_nullable_fields_distinguish_missing_null_and_value() {
    let image = |result: serde_json::Value| serde_json::json!({"type":"image_generation_call","id":"ig","result":result,"status":"completed"});
    for value in [serde_json::Value::Null, serde_json::json!("base64")] {
        let parsed: ImageGenerationCall = serde_json::from_value(image(value.clone())).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap()["result"], value);
    }
    assert!(serde_json::from_value::<ImageGenerationCall>(image(serde_json::json!({}))).is_err());

    let code = |extra: serde_json::Value| serde_json::json!({"type":"code_interpreter_call","id":"ci","container_id":"c","status":"completed","code":null,"outputs":null, "extra":extra});
    let parsed: CodeInterpreterCall =
        serde_json::from_value(code(serde_json::json!(true))).unwrap();
    let encoded = serde_json::to_value(parsed).unwrap();
    assert!(encoded.get("code").is_some());
    assert!(encoded.get("outputs").is_some());
    let missing_code = serde_json::json!({"type":"code_interpreter_call","id":"ci","container_id":"c","status":"completed","outputs":null});
    assert!(serde_json::from_value::<CodeInterpreterCall>(missing_code).is_err());

    for parameters in [
        serde_json::Value::Null,
        serde_json::json!({"type":"object"}),
    ] {
        let tool =
            serde_json::json!({"type":"function","name":"f","parameters":parameters,"strict":null});
        let parsed: Tool = serde_json::from_value(tool.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(parsed).unwrap()["parameters"],
            tool["parameters"]
        );
    }
    assert!(
        serde_json::from_value::<Tool>(
            serde_json::json!({"type":"function","name":"f","parameters":[],"strict":null})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<Tool>(
            serde_json::json!({"type":"function","name":"f","parameters":{},"strict":{}})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<Tool>(
            serde_json::json!({"type":"function","name":"f","strict":null})
        )
        .is_err()
    );
}

#[test]
fn derived_unions_keep_message_and_reference_shapes() {
    let values = serde_json::json!([
        {"role":"user","content":"plain"},
        {"type":"message","role":"user","content":[{"type":"input_text","text":"array"}]},
        {"type":"message","role":"assistant","content":[],"id":"out","status":"completed"},
        {"role":"developer","content":[{"type":"input_text","text":"status"}],"status":"completed"},
        {"id":"ref-only"},
        {"id":"ref-null","type":null}
    ]);
    let parsed: Vec<InputItem> = serde_json::from_value(values.clone()).unwrap();
    assert!(matches!(parsed[0], InputItem::Easy(_)));
    assert!(matches!(parsed[1], InputItem::Message(_)));
    assert!(matches!(parsed[2], InputItem::OutputMessage(_)));
    assert!(matches!(parsed[3], InputItem::Message(_)));
    assert!(matches!(parsed[4], InputItem::ItemReference(_)));
    assert!(matches!(parsed[5], InputItem::ItemReference(_)));
}
