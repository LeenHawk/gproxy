use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx};
use http::{HeaderMap, Method};
use serde_json::{Value, json};

use super::{CodexChannel, RESPONSES};

const CACHE_MAGIC: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH";

#[test]
fn prepare_keeps_opt_in_cache_breakpoint_through_codex_shaping() {
    let body = Bytes::from(
        json!({
            "model":"route",
            "instructions":format!("stable policy {CACHE_MAGIC}"),
            "input":"hello"
        })
        .to_string(),
    );
    let prepared = CodexChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: RESPONSES,
            stream: true,
            method: &Method::POST,
            path: "/v1/responses",
            query: None,
            headers: &HeaderMap::new(),
            body: &body,
            upstream_model: "gpt-5.4",
            provider_settings: &json!({"enable_openai_magic_cache":true}),
            secret: &json!({"access_token":"token"}),
        })
        .unwrap();
    let shaped: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert_eq!(shaped["instructions"], "stable policy ");
    assert_eq!(
        shaped["input"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
        "explicit"
    );
    assert!(!shaped.to_string().contains(CACHE_MAGIC));
}

#[test]
fn prepare_strips_downstream_cache_breakpoints_without_opt_in() {
    let body = Bytes::from_static(
        br#"{"model":"route","input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"hello","prompt_cache_breakpoint":{"mode":"explicit"}}]}]}"#,
    );
    let prepared = CodexChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: RESPONSES,
            stream: true,
            method: &Method::POST,
            path: "/v1/responses",
            query: None,
            headers: &HeaderMap::new(),
            body: &body,
            upstream_model: "gpt-5.6-luna",
            provider_settings: &json!({}),
            secret: &json!({"access_token":"token"}),
        })
        .unwrap();
    let shaped: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert!(!shaped.to_string().contains("prompt_cache_breakpoint"));
}
