//! End-to-end tests that the `claude` (anthropic direct) and `claudecode`
//! channels normalize client-supplied sampling parameters in
//! `finalize_request`.
//!
//! Rules:
//!   - Full-strip models (e.g. `claude-opus-4-6`): remove `temperature`,
//!     `top_p`, and `top_k` unconditionally.
//!   - Other models: if `temperature` is present, remove `top_p` only.

use gproxy_channel::channel::Channel;
use gproxy_channel::channels::anthropic::{AnthropicChannel, AnthropicSettings};
use gproxy_channel::channels::claudecode::{ClaudeCodeChannel, ClaudeCodeSettings};
use gproxy_channel::dispatch::RouteKey;
use gproxy_channel::request::PreparedRequest;
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};
use http::{HeaderMap, Method};
use serde_json::Value;

fn generate_content_request(body: &str, model: &str) -> PreparedRequest {
    PreparedRequest {
        method: Method::POST,
        route: RouteKey::new(OperationFamily::GenerateContent, ProtocolKind::Claude),
        model: Some(model.to_string()),
        body: body.as_bytes().to_vec(),
        headers: HeaderMap::new(),
    }
}

// ── Full-strip model: claude-opus-4-6 ────────────────────────────────

const OPUS_46_PAYLOAD: &str = r#"{
    "model": "claude-opus-4-6",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "hi"}],
    "temperature": 0.7,
    "top_p": 0.9,
    "top_k": 40
}"#;

fn assert_all_sampling_stripped(body_bytes: &[u8]) {
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
    assert!(map.get("messages").is_some(), "messages must be preserved");
    assert_eq!(map.get("max_tokens").and_then(Value::as_u64), Some(1024));
}

#[test]
fn anthropic_channel_strips_all_for_opus_46() {
    let settings = AnthropicSettings::default();
    let prepared = generate_content_request(OPUS_46_PAYLOAD, "claude-opus-4-6");

    let finalized = AnthropicChannel
        .finalize_request(&settings, prepared)
        .expect("anthropic finalize_request should succeed");

    assert_all_sampling_stripped(&finalized.body);
}

#[test]
fn claudecode_channel_strips_all_for_opus_46() {
    let settings = ClaudeCodeSettings::default();
    let prepared = generate_content_request(OPUS_46_PAYLOAD, "claude-opus-4-6");

    let finalized = ClaudeCodeChannel
        .finalize_request(&settings, prepared)
        .expect("claudecode finalize_request should succeed");

    assert_all_sampling_stripped(&finalized.body);
}

// ── Other model with temperature: strip top_p only ───────────────────

const SONNET_WITH_TEMP_PAYLOAD: &str = r#"{
    "model": "claude-sonnet-4-6",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "hi"}],
    "temperature": 0.7,
    "top_p": 0.9,
    "top_k": 40
}"#;

fn assert_only_top_p_stripped(body_bytes: &[u8]) {
    let body: Value = serde_json::from_slice(body_bytes).expect("body is JSON object");
    let map = body.as_object().expect("body is object");
    assert!(
        map.contains_key("temperature"),
        "temperature must be kept; got: {body}"
    );
    assert!(
        !map.contains_key("top_p"),
        "top_p must be stripped; got: {body}"
    );
    assert!(map.contains_key("top_k"), "top_k must be kept; got: {body}");
    assert!(map.get("messages").is_some(), "messages must be preserved");
    assert_eq!(map.get("max_tokens").and_then(Value::as_u64), Some(1024));
}

#[test]
fn anthropic_channel_strips_top_p_only_for_other_model_with_temp() {
    let settings = AnthropicSettings::default();
    let prepared = generate_content_request(SONNET_WITH_TEMP_PAYLOAD, "claude-sonnet-4-6");

    let finalized = AnthropicChannel
        .finalize_request(&settings, prepared)
        .expect("anthropic finalize_request should succeed");

    assert_only_top_p_stripped(&finalized.body);
}

#[test]
fn claudecode_channel_strips_top_p_only_for_other_model_with_temp() {
    let settings = ClaudeCodeSettings::default();
    let prepared = generate_content_request(SONNET_WITH_TEMP_PAYLOAD, "claude-sonnet-4-6");

    let finalized = ClaudeCodeChannel
        .finalize_request(&settings, prepared)
        .expect("claudecode finalize_request should succeed");

    assert_only_top_p_stripped(&finalized.body);
}

// ── Other model without temperature: no stripping ────────────────────

#[test]
fn anthropic_channel_keeps_all_for_tolerant_model_without_temperature() {
    let settings = AnthropicSettings::default();
    let payload = r#"{
        "model": "claude-sonnet-4-5",
        "max_tokens": 1024,
        "messages": [{"role": "user", "content": "hi"}],
        "top_p": 0.9,
        "top_k": 40
    }"#;
    let prepared = generate_content_request(payload, "claude-sonnet-4-5");

    let finalized = AnthropicChannel
        .finalize_request(&settings, prepared)
        .expect("finalize_request should succeed");

    let body: Value = serde_json::from_slice(&finalized.body).expect("body is JSON");
    let map = body.as_object().unwrap();
    assert!(!map.contains_key("temperature"));
    assert!(
        map.contains_key("top_p"),
        "top_p must be kept without temperature"
    );
    assert!(
        map.contains_key("top_k"),
        "top_k must be kept without temperature"
    );
}
