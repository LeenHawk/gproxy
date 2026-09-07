use bytes::Bytes;
use gproxy_channel_api::{Channel, Disposition, PrepareCtx, ResponseView};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};
use http::{HeaderMap, Method};
use serde_json::{Value, json};

use super::ClaudeApiChannel;

const MESSAGES: OperationKey = OperationKey::content(
    Operation::GenerateContent,
    ContentGenerationKind::ClaudeMessages,
);

#[test]
fn keeps_unsupported_embeddings_and_terminal_error_classification() {
    let supports = ClaudeApiChannel.descriptor().supports;
    assert!(!supports.iter().any(|support| {
        support.source == OperationKey::family(Operation::CreateEmbedding, WireFamily::OpenAi)
    }));
    assert_eq!(
        ClaudeApiChannel.classify(ResponseView {
            status: http::StatusCode::FORBIDDEN,
            headers: &HeaderMap::new(),
            body: &[],
        }),
        Disposition::Terminal
    );
}

#[test]
fn builds_documented_default_and_exact_override_urls() {
    let secret = json!({"api_key":" upstream-key "});
    let empty = Bytes::new();
    let listed = ClaudeApiChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: OperationKey::family(Operation::ListModels, WireFamily::Claude),
            stream: false,
            method: &Method::GET,
            path: "/v1/models",
            query: Some("limit=20&after_id=model_1&key=downstream&ignored=yes"),
            headers: &HeaderMap::new(),
            body: &empty,
            upstream_model: "",
            provider_settings: &json!({}),
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        listed.request.uri(),
        "https://api.anthropic.com/v1/models?limit=20&after_id=model_1"
    );

    let settings = json!({
        "base_url":"https://unused.example",
        "endpoints":{"claude_messages":"https://relay.example/native?fixed=1"}
    });
    let body = Bytes::from_static(br#"{"model":"route","max_tokens":8,"messages":[]}"#);
    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer downstream".parse().unwrap());
    headers.insert("anthropic-user-profile-id", "profile-1".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    let prepared = ClaudeApiChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: MESSAGES,
            stream: false,
            method: &Method::GET,
            path: "/v1/messages",
            query: Some("ignored=yes"),
            headers: &headers,
            body: &body,
            upstream_model: "claude-sonnet-4-6",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        prepared.request.uri(),
        "https://relay.example/native?fixed=1"
    );
    assert_eq!(prepared.request.method(), Method::POST);
    assert_eq!(prepared.request.headers()["x-api-key"], "upstream-key");
    assert_eq!(
        prepared.request.headers()["anthropic-version"],
        "2023-06-01"
    );
    assert_eq!(
        prepared.request.headers()["anthropic-user-profile-id"],
        "profile-1"
    );
    assert!(prepared.request.headers().get("authorization").is_none());
}

#[test]
fn shapes_cache_sampling_prefill_and_feature_betas_together() {
    let secret = json!({"api_key":"key"});
    let settings = json!({});
    let mut headers = HeaderMap::new();
    headers.insert(
        "anthropic-beta",
        "context-1m-2025-08-07,files-api-2025-04-14"
            .parse()
            .unwrap(),
    );
    let body = Bytes::from(
        json!({
            "model":"route-model",
            "speed":"fast",
            "thinking":{"type":"adaptive","display":"updates"},
            "temperature":0.7,
            "top_p":0.9,
            "top_k":40,
            "max_tokens":32,
            "system":[
                {"type":"text","text":" policy "},
                {"type":"text","text":" ","cache_control":{"type":"ephemeral"}}
            ],
            "messages":[{"role":"assistant","content":"prefix"}],
            "future_request_field":{"kept":true}
        })
        .to_string(),
    );
    let prepared = ClaudeApiChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: MESSAGES,
            stream: false,
            method: &Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &headers,
            body: &body,
            upstream_model: "claude-opus-4-8",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    let shaped: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert_eq!(shaped["model"], "claude-opus-4-8");
    assert_eq!(shaped["messages"][0]["role"], "user");
    assert_eq!(shaped["system"][0]["text"], "policy");
    assert_eq!(shaped["system"][0]["cache_control"]["type"], "ephemeral");
    assert!(shaped.get("temperature").is_none());
    assert!(shaped.get("top_p").is_none());
    assert!(shaped.get("top_k").is_none());
    assert_eq!(shaped["future_request_field"]["kept"], true);
    assert_eq!(
        prepared.request.headers()["anthropic-beta"],
        "files-api-2025-04-14,fast-mode-2026-02-01,thinking-display-updates-2026-08-18"
    );
}

#[test]
fn shapes_configured_and_existing_fallbacks_with_required_beta() {
    let secret = json!({"api_key":"key"});
    let body = Bytes::from_static(
        br#"{"model":"route","max_tokens":32,"fallbacks":[{"model":"claude-opus-4-8"}],"messages":[{"role":"user","content":"hello"}]}"#,
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        "anthropic-beta",
        "files-api-2025-04-14,server-side-fallback-2026-07-01"
            .parse()
            .unwrap(),
    );
    let prepared = ClaudeApiChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: MESSAGES,
            stream: false,
            method: &Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &headers,
            body: &body,
            upstream_model: "claude-fable-5",
            provider_settings: &json!({"claude_fallback_mode":"default"}),
            secret: &secret,
        })
        .unwrap();
    let shaped: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert_eq!(shaped["fallbacks"][0]["model"], "claude-opus-4-8");
    assert_eq!(
        prepared.request.headers()["anthropic-beta"],
        "files-api-2025-04-14,server-side-fallback-2026-06-01"
    );

    let body = Bytes::from_static(
        br#"{"model":"route","fallbacks":null,"fallback_credit_token":null,"max_tokens":32,"messages":[{"role":"user","content":"hello"}]}"#,
    );
    let defaulted = ClaudeApiChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: MESSAGES,
            stream: false,
            method: &Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &HeaderMap::new(),
            body: &body,
            upstream_model: "claude-fable-5",
            provider_settings: &json!({
                "claude_fallback_mode":"models",
                "claude_fallback_models":[]
            }),
            secret: &secret,
        })
        .unwrap();
    let shaped: Value = serde_json::from_slice(defaulted.request.body()).unwrap();
    assert_eq!(shaped["fallbacks"], "default");
    assert_eq!(
        defaulted.request.headers()["anthropic-beta"],
        "server-side-fallback-2026-07-01"
    );
}
