use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use super::*;
use crate::protocol::{Operation, OperationKey};

#[test]
fn native_claude_uses_encoded_invoke_model_path() {
    let secret = json!({ "api_key": "bedrock-key" });
    let settings = json!({ "region": "eu-west-1" });
    let headers = HeaderMap::new();
    let req = BedrockRuntimeChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op: OperationKey::content_generation(
                Operation::GenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
            stream: false,
            upstream_model_id: "arn:aws:bedrock:eu-west-1:123:model/example",
            method: Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &headers,
            body: Bytes::new(),
        })
        .unwrap()
        .into_http();
    assert_eq!(
        req.uri().to_string(),
        "https://bedrock-runtime.eu-west-1.amazonaws.com/model/arn%3Aaws%3Abedrock%3Aeu-west-1%3A123%3Amodel%2Fexample/invoke"
    );
    assert_eq!(req.headers()["authorization"], "Bearer bedrock-key");
}

#[test]
fn native_claude_stream_uses_eventstream_endpoint_and_headers() {
    let secret = json!({ "api_key": "bedrock-key" });
    let settings = json!({ "region": "us-east-1" });
    let headers = HeaderMap::new();
    let req = BedrockRuntimeChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op: OperationKey::content_generation(
                Operation::StreamGenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
            stream: true,
            upstream_model_id: "us.anthropic.claude-sonnet-4-6",
            method: Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &headers,
            body: Bytes::new(),
        })
        .unwrap()
        .into_http();
    assert_eq!(
        req.uri().to_string(),
        "https://bedrock-runtime.us-east-1.amazonaws.com/model/us.anthropic.claude-sonnet-4-6/invoke-with-response-stream"
    );
    assert_eq!(
        req.headers()[http::header::ACCEPT],
        "application/vnd.amazon.eventstream"
    );
    assert_eq!(req.headers()["x-amzn-bedrock-accept"], "application/json");
    assert_eq!(req.headers()["authorization"], "Bearer bedrock-key");
}

#[test]
fn runtime_openai_and_claude_stream_routes_remain_streaming() {
    for kind in [
        ContentGenerationKind::OpenAiChatCompletions,
        ContentGenerationKind::ClaudeMessages,
    ] {
        let source = OperationKey::content_generation(Operation::StreamGenerateContent, kind);
        let decision = BedrockRuntimeChannel
            .routing_table()
            .into_iter()
            .find(|(candidate, _)| *candidate == source)
            .unwrap()
            .1;
        assert_eq!(
            decision,
            crate::transform::routing::RoutingDecision::Passthrough
        );
    }
}

#[test]
fn shapes_native_claude_and_compaction_beta() {
    let settings = json!({ "enable_magic_cache": true });
    let ctx = ShapeCtx {
        op: OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        stream: false,
        status: StatusCode::OK,
        settings: &settings,
    };
    let mut headers = HeaderMap::new();
    let body = Bytes::from_static(
        br#"{"model":"anthropic.claude-sonnet-4-6","stream":true,"messages":[],"context_management":{"edits":[{"type":"compact_20260112"}]}}"#,
    );
    let shaped = BedrockRuntimeChannel.shape_request(body, &mut headers, &ctx);
    let value: Value = serde_json::from_slice(&shaped).unwrap();
    assert_eq!(value["anthropic_version"], "bedrock-2023-05-31");
    assert_eq!(value["anthropic_beta"], json!(["compact-2026-01-12"]));
    assert!(value.get("model").is_none());
    assert!(value.get("stream").is_none());
}

#[test]
fn shapes_bedrock_fable_fallback_and_beta() {
    let settings = json!({ "enable_claude_fable_fallback": true });
    let ctx = ShapeCtx {
        op: OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        stream: false,
        status: StatusCode::OK,
        settings: &settings,
    };
    let mut headers = HeaderMap::new();
    let shaped = BedrockRuntimeChannel.shape_request(
        Bytes::from_static(
            br#"{"model":"anthropic.claude-fable-5","messages":[],"max_tokens":32}"#,
        ),
        &mut headers,
        &ctx,
    );
    let value: Value = serde_json::from_slice(&shaped).unwrap();
    assert_eq!(
        value["fallbacks"],
        json!([{ "model": "anthropic.claude-opus-4-8" }])
    );
    assert_eq!(
        value["anthropic_beta"],
        json!(["server-side-fallback-2026-06-01"])
    );
}

#[test]
fn compact_targets_native_claude() {
    let source = OperationKey::provider(Operation::CompactContent, Provider::OpenAi);
    let decision = BedrockRuntimeChannel
        .routing_table()
        .into_iter()
        .find(|(candidate, _)| *candidate == source)
        .unwrap()
        .1;
    assert_eq!(
        decision,
        crate::transform::routing::RoutingDecision::TransformTo(OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ))
    );
}

#[test]
fn count_tokens_uses_runtime_wrapper_and_normalizes_response() {
    let settings = json!({});
    let ctx = ShapeCtx {
        op: OperationKey::provider(Operation::CountTokens, Provider::Claude),
        stream: false,
        status: StatusCode::OK,
        settings: &settings,
    };
    let mut headers = HeaderMap::new();
    let body = BedrockRuntimeChannel.shape_request(
        Bytes::from_static(br#"{"model":"anthropic.claude-sonnet-4-6","messages":[]}"#),
        &mut headers,
        &ctx,
    );
    let value: Value = serde_json::from_slice(&body).unwrap();
    let encoded = value["input"]["invokeModel"]["body"].as_str().unwrap();
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .unwrap();
    let invoke: Value = serde_json::from_slice(&decoded).unwrap();
    assert_eq!(invoke["anthropic_version"], "bedrock-2023-05-31");
    assert!(invoke.get("model").is_none());

    let response =
        BedrockRuntimeChannel.shape_response(Bytes::from_static(br#"{"inputTokens":42}"#), &ctx);
    assert_eq!(
        serde_json::from_slice::<Value>(&response).unwrap(),
        json!({ "input_tokens": 42 })
    );
}
