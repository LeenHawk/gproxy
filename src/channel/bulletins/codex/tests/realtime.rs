use bytes::Bytes;
use http::{HeaderMap, Method, Request};
use serde_json::{Value, json};

use super::super::CodexChannel;
use crate::channel::{Channel, PrepareCtx};
use crate::protocol::{ContentGenerationKind, Operation, OperationKey, Provider};

fn prepare(path: &str, query: &str, model: &str, settings: &Value) -> Request<Bytes> {
    let secret = json!({ "access_token": "oauth-token", "account_id": "acct-1" });
    let headers = HeaderMap::new();
    CodexChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: settings,
            op: OperationKey::provider(Operation::ConnectRealtime, Provider::OpenAi),
            stream: true,
            upstream_model_id: model,
            method: Method::GET,
            path,
            query: Some(query),
            headers: &headers,
            body: Bytes::new(),
        })
        .unwrap()
        .into_http()
        .unwrap()
}

#[test]
fn defaults_to_api_openai_and_preserves_v1_path_and_rewritten_query() {
    let query = crate::channel::realtime_websocket::rewrite_model_query(
        Some("key=gproxy-secret&model=public-route&intent=quicksilver"),
        "gpt-realtime-1.5",
    )
    .unwrap();
    let request = prepare(
        "/v1/realtime",
        &query,
        "gpt-realtime-1.5",
        &json!({ "base_url": "https://must-not-be-used.example/codex" }),
    );

    assert_eq!(
        request.uri(),
        "wss://api.openai.com/v1/realtime?model=gpt-realtime-1.5&intent=quicksilver"
    );
    assert_eq!(request.headers()["authorization"], "Bearer oauth-token");
    assert_eq!(request.headers()["chatgpt-account-id"], "acct-1");
    assert_eq!(request.headers()["originator"], "codex_cli_rs");

    let live = prepare(
        "/v1/live",
        "model=gpt-live-1-boulder-alpha",
        "gpt-live-1-boulder-alpha",
        &json!({}),
    );
    assert_eq!(
        live.uri(),
        "wss://api.openai.com/v1/live?model=gpt-live-1-boulder-alpha"
    );
}

#[test]
fn normal_codex_request_drops_realtime_only_headers() {
    let secret = json!({ "access_token": "oauth-token", "account_id": "acct-1" });
    let settings = json!({});
    let mut headers = HeaderMap::new();
    headers.insert("openai-alpha", "quicksilver=v2".parse().unwrap());
    headers.insert("x-session-id", "realtime-session".parse().unwrap());
    headers.insert("thread-id", "thread-1".parse().unwrap());
    let request = CodexChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op: OperationKey::content_generation(
                Operation::StreamGenerateContent,
                ContentGenerationKind::OpenAiResponses,
            ),
            stream: true,
            upstream_model_id: "gpt-5.4",
            method: Method::POST,
            path: "/v1/responses",
            query: None,
            headers: &headers,
            body: Bytes::new(),
        })
        .unwrap()
        .into_http()
        .unwrap();

    assert!(request.headers().get("openai-alpha").is_none());
    assert!(request.headers().get("x-session-id").is_none());
    assert_eq!(request.headers()["thread-id"], "thread-1");
    assert_eq!(request.headers()["originator"], "codex_cli_rs");
}

#[test]
fn exact_endpoint_override_preserves_query() {
    let request = prepare(
        "/v1/live",
        "model=gpt-live-1-boulder-alpha&openai-alpha=quicksilver%3Dv2",
        "gpt-live-1-boulder-alpha",
        &json!({
            "base_url": "https://must-not-be-used.example/codex",
            "endpoints": {
                "openai_realtime": "https://future.example/realtime-socket?fixed=1"
            }
        }),
    );

    assert_eq!(
        request.uri(),
        "wss://future.example/realtime-socket?fixed=1&model=gpt-live-1-boulder-alpha&openai-alpha=quicksilver%3Dv2"
    );
}
