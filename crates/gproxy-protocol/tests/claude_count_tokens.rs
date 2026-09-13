use gproxy_protocol::claude::content::{BlockType, Message, TextBlock};
use gproxy_protocol::claude::count_tokens::{CountTokensRequestBody, CountTokensResponseBody};
use gproxy_protocol::{WireRequest, WireResponse};
use http::{HeaderMap, HeaderValue, Method, StatusCode};

#[test]
fn count_tokens_response_round_trips_exactly() {
    let value = serde_json::json!({
        "context_management": {"original_input_tokens": 12},
        "input_tokens": 34,
        "future": null
    });
    let parsed: CountTokensResponseBody = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
}

#[test]
fn count_tokens_request_preserves_message_media_and_tool_schema() {
    let value = serde_json::json!({
        "messages": [{"role": "user", "content": [
            {"type": "text", "text": "look", "cache_control": {"type": "ephemeral", "ttl": "1h"}},
            {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AA=="}},
            {"type": "document", "source": {"type": "text", "media_type": "text/plain", "data": "notes"}, "title": "notes"}
        ]}],
        "model": "claude-opus-4-6",
        "system": [{"type": "text", "text": "be concise"}],
        "thinking": {"type": "adaptive", "display": "omitted"},
        "tools": [{"name": "lookup", "description": "find a value", "input_schema": {"type": "object", "properties": {"q": {"type": "string"}}, "required": ["q"]}}],
        "future_request_field": null
    });
    let parsed: CountTokensRequestBody = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
}

#[test]
fn count_tokens_http_envelope_keeps_transport_outside_json_body() {
    let body: CountTokensRequestBody = serde_json::from_value(serde_json::json!({
        "messages": [{"role": "user", "content": "Hello"}],
        "model": "claude-opus-4-6"
    }))
    .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_static("token-counting-2024-11-01"),
    );
    let request = WireRequest {
        method: Method::POST,
        path: "/v1/messages/count_tokens".into(),
        query: None,
        headers,
        body,
    };
    assert_eq!(request.path, "/v1/messages/count_tokens");
    assert_eq!(
        request.headers["anthropic-beta"],
        "token-counting-2024-11-01"
    );
    let body_json = serde_json::to_value(&request.body).unwrap();
    assert!(!body_json.as_object().unwrap().contains_key("path"));
    assert!(
        !body_json
            .as_object()
            .unwrap()
            .contains_key("anthropic-beta")
    );

    let response = WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        body: CountTokensResponseBody::builder(
            gproxy_protocol::claude::count_tokens::CountTokensContextManagementResponse::builder(0)
                .build(),
            12,
        )
        .build(),
    };
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.body.input_tokens, 12);
}

#[test]
fn direct_text_block_builder_emits_its_type_tag() {
    let block = TextBlock::builder(BlockType::Text, "be concise".to_owned()).build();
    assert_eq!(
        serde_json::to_value(block).unwrap(),
        serde_json::json!({
            "type": "text",
            "text": "be concise"
        })
    );
}

#[test]
fn message_role_is_limited_to_documented_turns() {
    let value = serde_json::json!({"role": "assistant", "content": "partial"});
    let parsed: Message = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
}

#[test]
fn service_content_is_dispatched_by_tag_instead_of_first_generic_variant() {
    use gproxy_protocol::claude::content::ContentBlock;
    let json = serde_json::json!({
        "type":"web_fetch_tool_result", "tool_use_id":"fetch_1",
        "content":{"type":"web_fetch_tool_result_error", "error_code":"url_not_allowed"}
    });
    let block: ContentBlock = serde_json::from_value(json.clone()).unwrap();
    assert!(matches!(block, ContentBlock::WebFetchToolResult(_)));
    assert_eq!(serde_json::to_value(block).unwrap(), json);
}
