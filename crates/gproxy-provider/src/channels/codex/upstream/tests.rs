use serde_json::json;

use super::{
    CodexPreparedRequest, SESSION_ID_HEADER, WreqMethod, apply_codex_priority_tier_override,
    normalize_codex_response_request_body, stable_codex_session_id,
};
use gproxy_middleware::{OperationFamily, ProtocolKind};

#[test]
fn codex_moves_system_and_developer_messages_into_instructions() {
    let mut body = json!({
        "model": "codex/gpt-5.2",
        "input": [
            {
                "type": "message",
                "role": "system",
                "content": "be concise"
            },
            {
                "type": "message",
                "role": "developer",
                "content": [
                    {"type": "input_text", "text": "keep markdown"}
                ]
            },
            {
                "type": "message",
                "role": "user",
                "content": [
                    {"type": "input_text", "text": "hello"}
                ]
            }
        ],
        "temperature": 1
    });

    normalize_codex_response_request_body(&mut body, true);

    assert_eq!(
        body.get("model").and_then(|value| value.as_str()),
        Some("gpt-5.2")
    );
    assert_eq!(
        body.get("stream").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        body.get("instructions").and_then(|value| value.as_str()),
        Some("be concise\n\nkeep markdown")
    );
    assert_eq!(
        body.pointer("/input/0/role")
            .and_then(|value| value.as_str()),
        Some("user")
    );
    assert!(body.pointer("/input/1").is_none());
    assert!(body.get("temperature").is_none());
}

#[test]
fn codex_appends_extracted_system_message_to_existing_instructions() {
    let mut body = json!({
        "model": "gpt-5.2",
        "instructions": "existing",
        "input": [
            {
                "type": "message",
                "role": "system",
                "content": "extra"
            },
            {
                "type": "message",
                "role": "user",
                "content": "hi"
            }
        ]
    });

    normalize_codex_response_request_body(&mut body, false);

    assert_eq!(
        body.get("instructions").and_then(|value| value.as_str()),
        Some("existing\n\nextra")
    );
    assert_eq!(
        body.get("stream").and_then(|value| value.as_bool()),
        Some(false)
    );
    assert_eq!(
        body.pointer("/input/0/role")
            .and_then(|value| value.as_str()),
        Some("user")
    );
    assert!(body.pointer("/input/1").is_none());
}

#[test]
fn codex_supports_websocket_payload_via_stream_fallback() {
    let payload = serde_json::to_vec(&json!({
        "method": "GET",
        "path": { "endpoint": "responses" },
        "query": {},
        "headers": {},
        "body": {
            "type": "response.create",
            "model": "codex/gpt-5",
            "stream": true
        }
    }))
    .expect("serialize websocket payload");

    let prepared = CodexPreparedRequest::from_payload(
        OperationFamily::OpenAiResponseWebSocket,
        ProtocolKind::OpenAi,
        payload.as_slice(),
    )
    .expect("prepare websocket payload");

    assert_eq!(prepared.method, WreqMethod::POST);
    assert_eq!(prepared.path, "/responses");
    assert_eq!(prepared.model.as_deref(), Some("codex/gpt-5"));
    assert!(prepared.body.is_some());
}

#[test]
fn codex_auto_injects_session_id_from_prompt_cache_key() {
    let payload = serde_json::to_vec(&json!({
        "method": "POST",
        "headers": { "extra": {} },
        "body": {
            "model": "gpt-5.3-codex",
            "prompt_cache_key": "thread-123",
            "input": [{"role": "user", "content": "hello"}]
        }
    }))
    .expect("serialize payload");

    let prepared = CodexPreparedRequest::from_payload(
        OperationFamily::GenerateContent,
        ProtocolKind::OpenAi,
        payload.as_slice(),
    )
    .expect("prepare payload");

    assert!(prepared.extra_headers.iter().any(|(name, value)| {
        name == SESSION_ID_HEADER && value == stable_codex_session_id("thread-123").as_str()
    }));
}

#[test]
fn codex_fallback_session_id_uses_instructions_and_first_input_only() {
    let payload_a = serde_json::to_vec(&json!({
        "method": "POST",
        "headers": { "extra": {} },
        "body": {
            "model": "gpt-5.3-codex",
            "input": [
                {"role": "system", "content": "be concise"},
                {"role": "user", "content": "hello"},
                {"role": "assistant", "content": "draft one"},
                {"role": "user", "content": "follow up a"}
            ],
            "tools": [{"type": "function", "name": "ignored_tool"}]
        }
    }))
    .expect("serialize payload a");
    let payload_b = serde_json::to_vec(&json!({
        "method": "POST",
        "headers": { "extra": {} },
        "body": {
            "model": "gpt-5.3-codex",
            "input": [
                {"role": "system", "content": "be concise"},
                {"role": "user", "content": "hello"},
                {"role": "assistant", "content": "draft two"},
                {"role": "user", "content": "follow up b"}
            ],
            "reasoning": {"effort": "high"}
        }
    }))
    .expect("serialize payload b");
    let payload_c = serde_json::to_vec(&json!({
        "method": "POST",
        "headers": { "extra": {} },
        "body": {
            "model": "gpt-5.3-codex",
            "input": [
                {"role": "system", "content": "be concise"},
                {"role": "user", "content": "different opener"}
            ]
        }
    }))
    .expect("serialize payload c");

    let prepared_a = CodexPreparedRequest::from_payload(
        OperationFamily::GenerateContent,
        ProtocolKind::OpenAi,
        payload_a.as_slice(),
    )
    .expect("prepare payload a");
    let prepared_b = CodexPreparedRequest::from_payload(
        OperationFamily::GenerateContent,
        ProtocolKind::OpenAi,
        payload_b.as_slice(),
    )
    .expect("prepare payload b");
    let prepared_c = CodexPreparedRequest::from_payload(
        OperationFamily::GenerateContent,
        ProtocolKind::OpenAi,
        payload_c.as_slice(),
    )
    .expect("prepare payload c");

    let session_id_a = prepared_a
        .extra_headers
        .iter()
        .find(|(name, _)| name == SESSION_ID_HEADER)
        .map(|(_, value)| value.as_str())
        .expect("session id a");
    let session_id_b = prepared_b
        .extra_headers
        .iter()
        .find(|(name, _)| name == SESSION_ID_HEADER)
        .map(|(_, value)| value.as_str())
        .expect("session id b");
    let session_id_c = prepared_c
        .extra_headers
        .iter()
        .find(|(name, _)| name == SESSION_ID_HEADER)
        .map(|(_, value)| value.as_str())
        .expect("session id c");

    assert_eq!(session_id_a, session_id_b);
    assert_ne!(session_id_a, session_id_c);
}

#[test]
fn codex_normalizes_session_id_header_name() {
    let payload = serde_json::to_vec(&json!({
        "method": "POST",
        "headers": {
            "extra": {
                "session-id": "sess-123"
            }
        },
        "body": {
            "model": "gpt-5.3-codex",
            "input": [{"role": "user", "content": "hello"}]
        }
    }))
    .expect("serialize payload");

    let prepared = CodexPreparedRequest::from_payload(
        OperationFamily::GenerateContent,
        ProtocolKind::OpenAi,
        payload.as_slice(),
    )
    .expect("prepare payload");

    assert_eq!(
        prepared.extra_headers,
        vec![(SESSION_ID_HEADER.to_string(), "sess-123".to_string())]
    );
}

#[test]
fn codex_priority_tier_override_sets_priority_service_tier() {
    let body = serde_json::to_vec(&json!({
        "model": "gpt-5.3-codex",
        "service_tier": "auto",
        "input": [{"role": "user", "content": "hello"}]
    }))
    .expect("serialize body");

    let overridden = apply_codex_priority_tier_override(Some(body.as_slice()), Some(true))
        .expect("override body");
    let overridden_json = serde_json::from_slice::<serde_json::Value>(overridden.as_slice())
        .expect("parse overridden body");

    assert_eq!(
        overridden_json
            .get("service_tier")
            .and_then(|value| value.as_str()),
        Some("priority")
    );
}

#[test]
fn codex_priority_tier_override_false_preserves_service_tier() {
    let body = serde_json::to_vec(&json!({
        "model": "gpt-5.3-codex",
        "service_tier": "auto",
        "input": [{"role": "user", "content": "hello"}]
    }))
    .expect("serialize body");

    let overridden = apply_codex_priority_tier_override(Some(body.as_slice()), Some(false))
        .expect("override body");
    let overridden_json = serde_json::from_slice::<serde_json::Value>(overridden.as_slice())
        .expect("parse overridden body");
    assert_eq!(
        overridden_json
            .get("service_tier")
            .and_then(|value| value.as_str()),
        Some("auto")
    );
}
