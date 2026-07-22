use bytes::Bytes;
use http::{HeaderMap, Method};
use serde_json::json;

use super::*;
use crate::protocol::{Operation, OperationKey};

fn prepare(op: OperationKey, path: &str, settings: &serde_json::Value) -> http::Request<Bytes> {
    let secret = json!({ "api_key": "bedrock-key" });
    let headers = HeaderMap::new();
    BedrockMantleChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: settings,
            op,
            stream: false,
            upstream_model_id: "anthropic.claude-sonnet-4-6-v1",
            method: Method::POST,
            path,
            query: None,
            headers: &headers,
            body: Bytes::new(),
        })
        .unwrap()
        .into_http()
}

#[test]
fn openai_surface_uses_regional_host_and_bearer() {
    let settings = json!({ "region": "us-west-2" });
    let req = prepare(
        OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        "/v1/responses",
        &settings,
    );
    assert_eq!(
        req.uri().to_string(),
        "https://bedrock-mantle.us-west-2.api.aws/v1/responses"
    );
    assert_eq!(req.headers()["authorization"], "Bearer bedrock-key");
}

#[test]
fn anthropic_surface_uses_x_api_key() {
    let settings = json!({});
    let req = prepare(
        OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        "/v1/messages",
        &settings,
    );
    assert_eq!(
        req.uri().to_string(),
        "https://bedrock-mantle.us-east-1.api.aws/anthropic/v1/messages"
    );
    assert_eq!(req.headers()["x-api-key"], "bedrock-key");
    assert_eq!(req.headers()["anthropic-version"], "2023-06-01");
    assert!(!req.headers().contains_key("authorization"));
}

#[test]
fn compact_falls_back_to_responses() {
    let source = OperationKey::provider(Operation::CompactContent, Provider::OpenAi);
    let decision = BedrockMantleChannel
        .routing_table()
        .into_iter()
        .find(|(candidate, _)| *candidate == source)
        .unwrap()
        .1;
    assert_eq!(
        decision,
        crate::transform::routing::RoutingDecision::TransformTo(OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ))
    );
}
