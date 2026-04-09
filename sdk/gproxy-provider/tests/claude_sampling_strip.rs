//! End-to-end tests that the `claude` (anthropic direct) and `claudecode`
//! channels strip client-supplied sampling parameters (`temperature`,
//! `top_p`, `top_k`) from the outbound request body inside
//! `finalize_request`. These sit next to `dispatch_alignment.rs` because
//! they exercise the same `Channel` trait integration boundary.

use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};
use gproxy_provider::channel::Channel;
use gproxy_provider::channels::anthropic::{AnthropicChannel, AnthropicSettings};
use gproxy_provider::channels::claudecode::{ClaudeCodeChannel, ClaudeCodeSettings};
use gproxy_provider::dispatch::RouteKey;
use gproxy_provider::request::PreparedRequest;
use http::{HeaderMap, Method};
use serde_json::Value;

const SAMPLING_PAYLOAD: &str = r#"{
    "model": "claude-sonnet-4-5",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "hi"}],
    "temperature": 0.7,
    "top_p": 0.9,
    "top_k": 40
}"#;

fn generate_content_request(body: &str) -> PreparedRequest {
    PreparedRequest {
        method: Method::POST,
        route: RouteKey::new(OperationFamily::GenerateContent, ProtocolKind::Claude),
        model: Some("claude-sonnet-4-5".to_string()),
        body: body.as_bytes().to_vec(),
        headers: HeaderMap::new(),
    }
}

fn assert_sampling_fields_stripped(body_bytes: &[u8]) {
    let body: Value = serde_json::from_slice(body_bytes).expect("body is JSON object");
    let map = body.as_object().expect("body is object");
    assert!(
        !map.contains_key("temperature"),
        "temperature must be stripped; got: {body}"
    );
    assert!(
        !map.contains_key("top_p"),
        "top_p must be stripped; got: {body}"
    );
    assert!(
        !map.contains_key("top_k"),
        "top_k must be stripped; got: {body}"
    );
    // Non-sampling fields should remain.
    assert_eq!(
        map.get("model").and_then(Value::as_str),
        Some("claude-sonnet-4-5"),
        "model must be preserved"
    );
    assert!(map.get("messages").is_some(), "messages must be preserved");
    assert_eq!(
        map.get("max_tokens").and_then(Value::as_u64),
        Some(1024),
        "max_tokens must be preserved"
    );
}

#[test]
fn anthropic_channel_strips_sampling_params_in_finalize_request() {
    let settings = AnthropicSettings::default();
    let prepared = generate_content_request(SAMPLING_PAYLOAD);

    let finalized = AnthropicChannel
        .finalize_request(&settings, prepared)
        .expect("anthropic finalize_request should succeed");

    assert_sampling_fields_stripped(&finalized.body);
}

#[test]
fn claudecode_channel_strips_sampling_params_in_finalize_request() {
    let settings = ClaudeCodeSettings::default();
    let prepared = generate_content_request(SAMPLING_PAYLOAD);

    let finalized = ClaudeCodeChannel
        .finalize_request(&settings, prepared)
        .expect("claudecode finalize_request should succeed");

    assert_sampling_fields_stripped(&finalized.body);
}

#[test]
fn anthropic_channel_leaves_body_unchanged_when_no_sampling_params() {
    let settings = AnthropicSettings::default();
    let payload = r#"{
        "model": "claude-sonnet-4-5",
        "max_tokens": 1024,
        "messages": [{"role": "user", "content": "hi"}]
    }"#;
    let prepared = generate_content_request(payload);

    let finalized = AnthropicChannel
        .finalize_request(&settings, prepared)
        .expect("finalize_request should succeed");

    let body: Value = serde_json::from_slice(&finalized.body).expect("body is JSON");
    let map = body.as_object().unwrap();
    assert_eq!(
        map.get("model").and_then(Value::as_str),
        Some("claude-sonnet-4-5")
    );
    assert_eq!(map.get("max_tokens").and_then(Value::as_u64), Some(1024));
    assert!(!map.contains_key("temperature"));
    assert!(!map.contains_key("top_p"));
    assert!(!map.contains_key("top_k"));
}
