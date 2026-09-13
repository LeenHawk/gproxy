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
        "tools":[{"type":"function", "name":"lookup", "parameters":{}}]
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(request).unwrap(),
        serde_json::json!({
            "input":[{"role":"user", "content":"hello"}],
            "tools":[{"type":"function", "name":"lookup", "parameters":{}}]
        })
    );
}
