use gproxy_protocol::{claude, gemini, openai};

#[test]
fn openai_models_round_trip_unknown_fields_and_snake_case() {
    let value = serde_json::json!({
        "object": "list",
        "data": [{"id": "gpt-5", "created": 1730000000, "object": "model", "owned_by": "openai", "future": {"x": 1}}],
        "future_page": null
    });
    let parsed: openai::models::ListModelsResponseBody =
        serde_json::from_value(value.clone()).unwrap();
    let output = serde_json::to_value(parsed).unwrap();
    assert_eq!(output, value);
    assert_eq!(output["data"][0]["owned_by"], "openai");
    assert_eq!(output["data"][0]["future"]["x"], 1);
    assert!(output["future_page"].is_null());
}

#[test]
fn claude_models_round_trip_pagination_and_capabilities() {
    let value = serde_json::json!({
        "data": [{
            "id": "claude-opus-4-6", "allowed_fallback_models": [],
            "capabilities": {
                "batch": {"supported": true}, "citations": {"supported": false},
                "code_execution": {"supported": true},
                "context_management": {"clear_thinking_20251015": {"supported": true}, "clear_tool_uses_20250919": {"supported": false}, "compact_20260112": {"supported": true}, "supported": true},
                "effort": {"high": {"supported": true}, "low": {"supported": true}, "max": {"supported": false}, "medium": {"supported": true}, "supported": true, "xhigh": {"supported": false}},
                "image_input": {"supported": true}, "pdf_input": {"supported": true}, "structured_outputs": {"supported": true},
                "thinking": {"supported": true, "types": {"adaptive": {"supported": true}, "enabled": {"supported": true}}}
            },
            "created_at": "2026-02-04T00:00:00Z", "display_name": "Claude Opus 4.6",
            "max_input_tokens": 200000, "max_tokens": 32000, "type": "model", "new_capability": "future"
        }],
        "first_id": "first", "has_more": true, "last_id": "last"
    });
    let parsed: claude::models::ListModelsResponseBody =
        serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), value);
    assert!(parsed.has_more);
    let output = serde_json::to_value(parsed).unwrap();
    assert_eq!(output["data"][0]["new_capability"], "future");
    assert_eq!(
        output["data"][0]["capabilities"]["context_management"]["clear_thinking_20251015"]["supported"],
        true
    );
    assert_eq!(output["first_id"], "first");
    assert_eq!(output["last_id"], "last");
}

#[test]
fn gemini_models_round_trip_optional_page_token_and_camel_case() {
    let value = serde_json::json!({
        "models": [{
            "name": "models/gemini-2.0-flash", "baseModelId": "gemini-2.0-flash", "version": "2.0",
            "displayName": "Gemini 2.0 Flash", "description": "fast", "inputTokenLimit": 1048576,
            "outputTokenLimit": 8192, "supportedGenerationMethods": ["generateContent"], "thinking": true,
            "temperature": 1.0, "maxTemperature": 2.0, "topP": 0.95, "topK": 40, "futureField": 7
        }],
        "nextPageToken": null
    });
    let parsed: gemini::models::ListModelsResponseBody =
        serde_json::from_value(value.clone()).unwrap();
    assert!(parsed.next_page_token.is_none());
    let output = serde_json::to_value(parsed).unwrap();
    assert_eq!(output["models"][0]["baseModelId"], "gemini-2.0-flash");
    assert_eq!(output["models"][0]["futureField"], 7);
    let mut expected = value;
    expected.as_object_mut().unwrap().remove("nextPageToken");
    assert_eq!(output, expected);
    assert!(!output.as_object().unwrap().contains_key("nextPageToken"));
}

#[test]
fn model_request_keeps_path_outside_the_body() {
    let request = openai::models::GetModelRequest {
        method: http::Method::GET,
        path: "/v1/models/gpt-5".into(),
        query: None,
        headers: http::HeaderMap::new(),
        body: (),
    };
    assert_eq!(request.path, "/v1/models/gpt-5");
    let model: gemini::models::Model = serde_json::from_value(serde_json::json!({
        "name": "models/x", "baseModelId": "x", "version": "1"
    }))
    .unwrap();
    assert_eq!(model.name, "models/x");
    assert!(
        serde_json::to_value(model)
            .unwrap()
            .as_object()
            .unwrap()
            .get("displayName")
            .is_none()
    );
}

#[test]
fn gemini_empty_page_can_omit_repeated_fields() {
    let page: gemini::models::ListModelsResponseBody = serde_json::from_str("{}").unwrap();
    assert!(page.models.is_none());
    assert_eq!(serde_json::to_value(page).unwrap(), serde_json::json!({}));
}

#[test]
fn model_queries_and_headers_are_separate_from_response_json() {
    let query = claude::models::ListModelsQuery::builder()
        .after_id("claude-opus-4-6")
        .limit(10_i64)
        .build();
    assert_eq!(
        serde_json::to_value(query).unwrap(),
        serde_json::json!({
            "after_id": "claude-opus-4-6", "limit": 10
        })
    );
    let mut headers = http::HeaderMap::new();
    headers.insert(
        "anthropic-beta",
        http::HeaderValue::from_static("test-beta"),
    );
    let request = claude::models::ListModelsRequest {
        method: http::Method::GET,
        path: "/v1/models".into(),
        query: Some("after_id=claude-opus-4-6&limit=10".into()),
        headers,
        body: (),
    };
    assert_eq!(request.headers["anthropic-beta"], "test-beta");
    assert_eq!(
        request.query.as_deref(),
        Some("after_id=claude-opus-4-6&limit=10")
    );

    let response = openai::models::GetModelResponse {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: openai::models::Model::builder(
            "gpt-5".into(),
            0,
            openai::models::ModelObject::Model,
            "openai".into(),
        )
        .build(),
    };
    assert_eq!(
        serde_json::to_value(&response.body).unwrap(),
        serde_json::json!({
            "id": "gpt-5", "created": 0, "object": "model", "owned_by": "openai"
        })
    );
    assert_eq!(response.status, http::StatusCode::OK);
}
