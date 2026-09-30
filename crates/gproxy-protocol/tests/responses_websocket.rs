use gproxy_protocol::openai::responses::{generate::GenerateContentRequestBody, websocket::*};
use serde_json::json;

#[test]
fn response_create_uses_flat_native_fields_and_preserves_extensions() {
    let wire = json!({
        "type": "response.create", "model": "gpt-5", "input": "hello",
        "previous_response_id": null, "stream": true, "future": {"preserved": true}
    });
    let event: ClientEvent = serde_json::from_value(wire.clone()).unwrap();
    let ClientEvent::ResponseCreate(body) = &event else {
        panic!("expected response.create");
    };
    assert_eq!(body.model.as_deref(), Some("gpt-5"));
    assert!(!body.rest.contains_key("type"));
    assert_eq!(body.rest.len(), 1);
    assert_eq!(serde_json::to_value(event).unwrap(), wire);
    assert_eq!(
        serde_json::to_value(ClientEvent::ResponseCreate(
            GenerateContentRequestBody::builder().build()
        ))
        .unwrap(),
        json!({"type": "response.create"})
    );
}

#[test]
fn server_events_keep_payload_schema_and_handshake_stays_http() {
    let wire = json!({"type": "response.audio.delta", "sequence_number": 3, "delta": "AA=="});
    let event: ServerEvent = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(event).unwrap(), wire);
    let request = HandshakeRequest {
        method: http::Method::GET,
        path: "/v1/responses".into(),
        query: None,
        headers: http::HeaderMap::new(),
        body: (),
    };
    let response = HandshakeResponse {
        status: http::StatusCode::SWITCHING_PROTOCOLS,
        headers: http::HeaderMap::new(),
        body: (),
    };
    assert_eq!(request.path, "/v1/responses");
    assert_eq!(response.status.as_u16(), 101);
}
