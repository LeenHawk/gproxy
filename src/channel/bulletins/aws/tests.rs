use base64::Engine;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use super::*;
use crate::protocol::OperationKey;

fn prepare(op: OperationKey, model: &str, path: &str, stream: bool) -> http::Request<Bytes> {
    let secret = json!({ "api_key": "aws-key" });
    let settings = json!({ "region": "us-west-2" });
    let headers = HeaderMap::new();
    AwsChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op,
            stream,
            upstream_model_id: model,
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
fn mantle_openai_and_claude_use_native_surfaces() {
    let responses = prepare(
        OperationKey::content_generation(
            Operation::StreamGenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        "openai.gpt-oss-120b",
        "/v1/responses",
        true,
    );
    assert_eq!(
        responses.uri().to_string(),
        "https://bedrock-mantle.us-west-2.api.aws/v1/responses"
    );
    assert_eq!(responses.headers()["authorization"], "Bearer aws-key");

    let claude = prepare(
        OperationKey::content_generation(
            Operation::StreamGenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        "anthropic.claude-sonnet-4-6",
        "/v1/messages",
        true,
    );
    assert_eq!(
        claude.uri().to_string(),
        "https://bedrock-mantle.us-west-2.api.aws/anthropic/v1/messages"
    );
    assert_eq!(claude.headers()["x-api-key"], "aws-key");
    assert_eq!(claude.headers()["anthropic-version"], "2023-06-01");
}

#[test]
fn count_tokens_alone_uses_runtime() {
    let req = prepare(
        OperationKey::provider(Operation::CountTokens, Provider::Claude),
        "us.anthropic.claude-sonnet-4-6",
        "/v1/messages/count_tokens",
        false,
    );
    assert_eq!(
        req.uri().to_string(),
        "https://bedrock-runtime.us-west-2.amazonaws.com/model/us.anthropic.claude-sonnet-4-6/count-tokens"
    );
    assert_eq!(req.headers()["authorization"], "Bearer aws-key");
    assert!(!req.headers().contains_key("x-api-key"));
}

#[test]
fn count_tokens_body_and_response_use_runtime_schema() {
    let settings = json!({});
    let ctx = ShapeCtx {
        op: OperationKey::provider(Operation::CountTokens, Provider::Claude),
        stream: false,
        status: StatusCode::OK,
        settings: &settings,
    };
    let mut headers = HeaderMap::new();
    let body = AwsChannel.shape_request(
        Bytes::from_static(br#"{"model":"anthropic.claude-sonnet-4-6","messages":[]}"#),
        &mut headers,
        &ctx,
    );
    let value: Value = serde_json::from_slice(&body).unwrap();
    let encoded = value["input"]["invokeModel"]["body"].as_str().unwrap();
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .unwrap();
    let invoke: Value = serde_json::from_slice(&decoded).unwrap();
    assert_eq!(invoke["anthropic_version"], "bedrock-2023-05-31");
    assert!(invoke.get("model").is_none());

    let response = AwsChannel.shape_response(Bytes::from_static(br#"{"inputTokens":42}"#), &ctx);
    assert_eq!(
        serde_json::from_slice::<Value>(&response).unwrap(),
        json!({ "input_tokens": 42 })
    );
}

#[test]
fn compact_uses_mantle_responses() {
    let source = OperationKey::provider(Operation::CompactContent, Provider::OpenAi);
    let decision = AwsChannel
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
