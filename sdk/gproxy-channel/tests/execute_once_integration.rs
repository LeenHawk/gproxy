//! Integration test: run one full request through
//! [`gproxy_channel::executor::execute_once`] against a local axum mock
//! server, verifying the complete L1 pipeline
//! (finalize → sanitize/rewrite → prepare_request → HTTP send →
//! normalize_response → classify_response) works end-to-end with only
//! `gproxy-channel` — no `gproxy-engine`.
//!
//! This is the integration-test arm of the SDK layer refactor's
//! spec Step 7 success criterion: "enables a single channel feature
//! and runs one request through `execute_once`". The test is gated by
//! the `openai` feature and runs under the default feature set in CI.

#![cfg(feature = "openai")]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Json;
use axum::routing::post;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use gproxy_channel::channels::openai::{OpenAiChannel, OpenAiCredential, OpenAiSettings};
use gproxy_channel::dispatch::RouteKey;
use gproxy_channel::executor::execute_once;
use gproxy_channel::request::PreparedRequest;
use gproxy_channel::response::ResponseClassification;
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

/// State captured by the mock server so the test can assert against
/// exactly what hit the wire.
#[derive(Default)]
struct ServerState {
    calls: AtomicUsize,
    last_authorization: std::sync::Mutex<Option<String>>,
    last_body: std::sync::Mutex<Option<Value>>,
}

async fn chat_completions_handler(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> (StatusCode, Json<Value>) {
    state.calls.fetch_add(1, Ordering::Relaxed);

    *state.last_authorization.lock().unwrap() = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    *state.last_body.lock().unwrap() = Some(payload.clone());

    // Return a minimal valid OpenAI ChatCompletions success body.
    let response = json!({
        "id": "chatcmpl-test-123",
        "object": "chat.completion",
        "created": 0,
        "model": payload.get("model").and_then(Value::as_str).unwrap_or("gpt-4o-mini"),
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "hello from mock" },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 3, "completion_tokens": 3, "total_tokens": 6 }
    });
    (StatusCode::OK, Json(response))
}

/// Start the mock server on a random local port and return
/// (base_url, shared state handle).
async fn start_mock_server() -> (String, Arc<ServerState>) {
    let state = Arc::new(ServerState::default());
    let app = Router::new()
        .route("/v1/chat/completions", post(chat_completions_handler))
        .with_state(state.clone());

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("axum::serve");
    });
    (format!("http://{addr}"), state)
}

#[tokio::test]
async fn execute_once_hits_mock_upstream_and_parses_success() {
    let (base_url, server_state) = start_mock_server().await;

    let channel = OpenAiChannel;
    let settings = OpenAiSettings {
        base_url,
        ..Default::default()
    };
    let credential = OpenAiCredential {
        api_key: "sk-integration-test-key".to_string(),
    };

    // Use wreq built-in default — same thing the real gproxy engine uses.
    let http_client = wreq::Client::new();

    let request = PreparedRequest {
        method: http::Method::POST,
        route: RouteKey::new(
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ),
        model: Some("gpt-4o-mini".to_string()),
        body: serde_json::to_vec(&json!({
            "model": "gpt-4o-mini",
            "messages": [{"role": "user", "content": "hi"}]
        }))
        .unwrap(),
        headers: http::HeaderMap::new(),
    };

    let outcome = execute_once(&channel, &credential, &settings, &http_client, request)
        .await
        .expect("execute_once should succeed against the mock server");

    // Classification side.
    assert_eq!(outcome.response.status, 200);
    assert!(
        matches!(outcome.classification, ResponseClassification::Success),
        "expected Success classification, got {:?}",
        outcome.classification
    );

    // Response body side: the mock echoes the model name, confirming the
    // full finalize → prepare_request → send → normalize path ran.
    let body: Value =
        serde_json::from_slice(&outcome.response.body).expect("response body is valid JSON");
    assert_eq!(body["id"], "chatcmpl-test-123");
    assert_eq!(body["model"], "gpt-4o-mini");
    assert_eq!(body["choices"][0]["message"]["content"], "hello from mock");

    // Request side: prepare_request must have attached the credential as
    // a Bearer token and must have forwarded the body we asked for.
    assert_eq!(server_state.calls.load(Ordering::Relaxed), 1);
    let auth = server_state
        .last_authorization
        .lock()
        .unwrap()
        .clone()
        .expect("Authorization header missing on mock");
    assert!(
        auth.starts_with("Bearer sk-integration-test-key"),
        "Authorization header did not use Bearer scheme: {auth}"
    );
    let last_body = server_state
        .last_body
        .lock()
        .unwrap()
        .clone()
        .expect("mock never saw a body");
    assert_eq!(last_body["model"], "gpt-4o-mini");
    assert_eq!(last_body["messages"][0]["content"], "hi");
}
