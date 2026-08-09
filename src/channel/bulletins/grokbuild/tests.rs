use bytes::Bytes;
use http::{HeaderMap, Method};
use serde_json::{Value, json};

use super::{GrokBuildChannel, shape};
use crate::channel::{Channel, PrepareCtx};
use crate::protocol::{
    ContentGenerationKind as Kind, Operation, OperationKey, OperationKind, Provider,
};
use crate::routing::RoutingDecision;

fn route(operation: Operation, kind: OperationKind) -> RoutingDecision {
    GrokBuildChannel
        .routing_table()
        .into_iter()
        .find(|(source, _)| source.operation() == operation && source.kind() == kind)
        .map(|(_, decision)| decision)
        .expect("missing Grok Build route")
}

#[test]
fn responses_preserve_native_stream_mode() {
    assert_eq!(
        route(
            Operation::GenerateContent,
            OperationKind::ContentGeneration(Kind::OpenAiResponses),
        ),
        RoutingDecision::Passthrough,
    );
    assert_eq!(
        route(
            Operation::StreamGenerateContent,
            OperationKind::ContentGeneration(Kind::OpenAiResponses),
        ),
        RoutingDecision::Passthrough,
    );
}

#[test]
fn responses_websocket_routes_to_http_responses_stream() {
    for operation in [Operation::GenerateContent, Operation::StreamGenerateContent] {
        let RoutingDecision::TransformTo(target) = route(
            operation,
            OperationKind::ContentGeneration(Kind::OpenAiResponsesWebSocket),
        ) else {
            panic!("Responses WebSocket should use the existing HTTP Responses transform");
        };
        assert_eq!(target.operation(), Operation::StreamGenerateContent);
        assert_eq!(
            target.kind(),
            OperationKind::ContentGeneration(Kind::OpenAiResponses),
        );
    }
}

#[test]
fn compact_routes_to_non_stream_http_responses() {
    let RoutingDecision::TransformTo(target) = route(
        Operation::CompactContent,
        OperationKind::Provider(Provider::OpenAi),
    ) else {
        panic!("Compact should use the existing OpenAI Compact to Responses transform");
    };
    assert_eq!(
        target,
        OperationKey::content_generation(Operation::GenerateContent, Kind::OpenAiResponses),
    );
}

#[test]
fn response_shape_preserves_stream_while_retaining_grok_hygiene() {
    for (stream, expected) in [
        (Some(false), Some(false)),
        (Some(true), Some(true)),
        (None, None),
    ] {
        let mut body = json!({
            "model": "grok-4.5",
            "input": "hello",
            "metadata": {"private": true},
            "previous_response_id": "resp-old",
        });
        if let Some(stream) = stream {
            body["stream"] = Value::Bool(stream);
        }

        let shaped: Value = serde_json::from_slice(&shape::shape_responses_request(Bytes::from(
            body.to_string(),
        )))
        .unwrap();

        assert_eq!(shaped.get("stream").and_then(Value::as_bool), expected);
        assert!(shaped.get("metadata").is_none());
        assert!(shaped.get("previous_response_id").is_none());
    }
}

#[test]
fn prepare_no_longer_opens_an_upstream_responses_websocket() {
    let secret = json!({"access_token": "oauth-token", "sub": "user-1"});
    let settings = json!({});
    let headers = HeaderMap::new();
    let request = GrokBuildChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op: OperationKey::content_generation(
                Operation::StreamGenerateContent,
                Kind::OpenAiResponsesWebSocket,
            ),
            stream: true,
            upstream_model_id: "grok-4.5",
            method: Method::GET,
            path: "/v1/responses",
            query: None,
            headers: &headers,
            body: Bytes::from_static(
                br#"{"type":"response.create","model":"grok-4.5","input":"hello"}"#,
            ),
        })
        .unwrap()
        .into_http()
        .unwrap();

    assert_eq!(request.uri().scheme_str(), Some("https"));
    assert_eq!(request.uri().path(), "/v1/responses");
}
