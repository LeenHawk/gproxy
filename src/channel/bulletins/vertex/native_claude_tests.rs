use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method, Request, StatusCode};
use serde_json::{Value, json};

use super::*;
use crate::protocol::{ContentGenerationKind, OperationKey};
use crate::transform::routing::RoutingDecision;

fn secret() -> Value {
    json!({
        "access_token": "tok-abc",
        "project_id": "proj-123",
    })
}

fn shape_ctx(op: Operation, kind: OperationKind, stream: bool) -> ShapeCtx<'static> {
    ShapeCtx {
        op: OperationKey {
            operation: op,
            kind,
        },
        stream,
        status: StatusCode::OK,
        settings: &Value::Null,
    }
}

fn prepare(path: &str, model: &str, body: Bytes, headers: &HeaderMap) -> Request<Bytes> {
    let secret = secret();
    let settings = json!({ "location": "us-east5" });
    VertexChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op: OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
            stream: false,
            upstream_model_id: model,
            method: Method::POST,
            path,
            query: None,
            headers,
            body,
        })
        .unwrap()
        .into_http()
}

#[test]
fn claude_routes_are_native_passthroughs() {
    let routes = VertexChannel.routing_table();
    for key in [
        OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        OperationKey::content_generation(
            Operation::StreamGenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        OperationKey::provider(Operation::CountTokens, Provider::Claude),
    ] {
        assert_eq!(
            routes.iter().find(|(source, _)| *source == key).unwrap().1,
            RoutingDecision::Passthrough
        );
    }
}

#[test]
fn messages_use_anthropic_raw_predict() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_static("prompt-caching-2024-07-31"),
    );
    headers.insert("x-api-key", HeaderValue::from_static("client-key"));
    let body = Bytes::from_static(
        br#"{"model":"claude-sonnet-4-5@20250929","max_tokens":64,"messages":[]}"#,
    );
    let shaped = VertexChannel.shape_request(
        body,
        &mut headers,
        &shape_ctx(
            Operation::GenerateContent,
            OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages),
            false,
        ),
    );
    let value: Value = serde_json::from_slice(&shaped).unwrap();
    assert!(value.get("model").is_none());
    assert_eq!(value["anthropic_version"], "vertex-2023-10-16");

    let req = prepare(
        "/v1/messages",
        "claude-sonnet-4-5@20250929",
        shaped,
        &headers,
    );
    assert_eq!(
        req.uri().to_string(),
        "https://us-east5-aiplatform.googleapis.com/v1/projects/proj-123/locations/us-east5/publishers/anthropic/models/claude-sonnet-4-5@20250929:rawPredict"
    );
    assert_eq!(
        req.headers().get("anthropic-beta").unwrap(),
        "prompt-caching-2024-07-31"
    );
    assert!(req.headers().get("x-api-key").is_none());
    assert_eq!(
        req.headers().get("authorization").unwrap(),
        "Bearer tok-abc"
    );
}

#[test]
fn streaming_messages_use_stream_raw_predict() {
    let mut headers = HeaderMap::new();
    let body = Bytes::from_static(
        br#"{"model":"claude-opus-4-1@20250805","messages":[],"stream":true,"anthropic_version":"custom-version"}"#,
    );
    let shaped = VertexChannel.shape_request(
        body,
        &mut headers,
        &shape_ctx(
            Operation::StreamGenerateContent,
            OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages),
            true,
        ),
    );
    let value: Value = serde_json::from_slice(&shaped).unwrap();
    assert_eq!(value["anthropic_version"], "custom-version");
    assert_eq!(value["stream"], true);

    let req = prepare("/v1/messages", "claude-opus-4-1@20250805", shaped, &headers);
    assert!(
        req.uri()
            .to_string()
            .ends_with("/publishers/anthropic/models/claude-opus-4-1@20250805:streamRawPredict")
    );
}

#[test]
fn count_tokens_uses_fixed_model_and_keeps_body_model() {
    let mut headers = HeaderMap::new();
    let body = Bytes::from_static(br#"{"model":"claude-sonnet-4-5@20250929","messages":[]}"#);
    let shaped = VertexChannel.shape_request(
        body,
        &mut headers,
        &shape_ctx(
            Operation::CountTokens,
            OperationKind::Provider(Provider::Claude),
            false,
        ),
    );
    let value: Value = serde_json::from_slice(&shaped).unwrap();
    assert_eq!(value["model"], "claude-sonnet-4-5@20250929");
    assert_eq!(value["anthropic_version"], "vertex-2023-10-16");

    let req = prepare(
        "/v1/messages/count_tokens",
        "claude-sonnet-4-5@20250929",
        shaped,
        &headers,
    );
    assert!(
        req.uri()
            .to_string()
            .ends_with("/publishers/anthropic/models/count-tokens:rawPredict")
    );
}
