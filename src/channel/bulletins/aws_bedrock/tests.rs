use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use super::*;
use crate::protocol::{ContentGenerationKind, OperationKey};
use crate::transform::{TransformContext, dispatch, resolve};

fn prepare(op: OperationKey, model: &str, body: Bytes, stream: bool) -> http::Request<Bytes> {
    let secret = json!({ "api_key": "bedrock-key" });
    let settings = json!({ "region": "us-west-2" });
    AwsBedrockChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op,
            stream,
            upstream_model_id: model,
            method: if op.operation == Operation::ListModels {
                Method::GET
            } else {
                Method::POST
            },
            path: "/ignored",
            query: None,
            headers: &HeaderMap::new(),
            body,
        })
        .unwrap()
        .into_http()
}

#[test]
fn routes_generation_to_converse_and_compact_to_invoke() {
    let op = OperationKey::content_generation(
        Operation::GenerateContent,
        ContentGenerationKind::ClaudeMessages,
    );
    let converse = prepare(op, "us.anthropic.claude-sonnet-4-6", Bytes::new(), false);
    assert_eq!(
        converse.uri(),
        "https://bedrock-runtime.us-west-2.amazonaws.com/model/us.anthropic.claude-sonnet-4-6/converse"
    );
    assert_eq!(converse.headers()["authorization"], "Bearer bedrock-key");

    let compact = prepare(
        op,
        "us.anthropic.claude-sonnet-4-6",
        Bytes::from_static(br#"{"context_management":{"edits":[{"type":"compact_20260112"}]}}"#),
        false,
    );
    assert!(compact.uri().path().ends_with("/invoke"));
}

#[test]
fn shapes_claude_messages_to_converse_and_back() {
    let settings = json!({});
    let ctx = ShapeCtx {
        op: OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        stream: false,
        status: StatusCode::OK,
        settings: &settings,
    };
    let request = AwsBedrockChannel.shape_request(Bytes::from_static(
        br#"{"model":"x","max_tokens":8,"messages":[{"role":"user","content":[{"type":"text","text":"hello","cache_control":{"type":"ephemeral"}}]}],"tools":[{"name":"get_weather","description":"Get weather","input_schema":{"type":"object","properties":{"city":{"type":"string"}}}}],"tool_choice":{"type":"tool","name":"get_weather"}}"#,
    ), &mut HeaderMap::new(), &ctx);
    let request: Value = serde_json::from_slice(&request).unwrap();
    assert_eq!(request["inferenceConfig"]["maxTokens"], 8);
    assert_eq!(request["messages"][0]["content"][0]["text"], "hello");
    assert_eq!(
        request["messages"][0]["content"][1]["cachePoint"]["type"],
        "default"
    );
    assert_eq!(
        request["toolConfig"]["tools"][0]["toolSpec"]["name"],
        "get_weather"
    );
    assert_eq!(
        request["toolConfig"]["toolChoice"]["tool"]["name"],
        "get_weather"
    );

    let response = AwsBedrockChannel.shape_response(Bytes::from_static(
        br#"{"output":{"message":{"role":"assistant","content":[{"toolUse":{"toolUseId":"tool_1","name":"get_weather","input":{"city":"Paris"}}}]}},"stopReason":"tool_use","usage":{"inputTokens":9,"outputTokens":4}}"#,
    ), &ctx);
    let response: Value = serde_json::from_slice(&response).unwrap();
    assert_eq!(response["type"], "message");
    assert_eq!(response["content"][0]["type"], "tool_use");
    assert_eq!(response["content"][0]["input"]["city"], "Paris");
    assert_eq!(response["usage"]["input_tokens"], 9);
}

#[test]
fn count_tokens_uses_converse_input() {
    let settings = json!({});
    let ctx = ShapeCtx {
        op: OperationKey::provider(Operation::CountTokens, Provider::Claude),
        stream: false,
        status: StatusCode::OK,
        settings: &settings,
    };
    let body = AwsBedrockChannel.shape_request(
        Bytes::from_static(br#"{"model":"x","messages":[{"role":"user","content":"hello"}]}"#),
        &mut HeaderMap::new(),
        &ctx,
    );
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        value["input"]["converse"]["messages"][0]["content"][0]["text"],
        "hello"
    );
}

#[test]
fn forbidden_model_error_is_relayed_instead_of_failed_over() {
    assert_eq!(
        AwsBedrockChannel.classify(
            StatusCode::FORBIDDEN,
            &HeaderMap::new(),
            &Bytes::from_static(br#"{"message":"model unavailable"}"#),
        ),
        Disposition::Permanent
    );
}

#[test]
fn compact_transform_becomes_bedrock_invoke_body() {
    let source = OperationKey::provider(Operation::CompactContent, Provider::OpenAi);
    let target = OperationKey::content_generation(
        Operation::GenerateContent,
        ContentGenerationKind::ClaudeMessages,
    );
    let transformed = dispatch::request_bytes(
        resolve(source, target).unwrap(),
        &TransformContext::new(source, target),
        br#"{"model":"us.anthropic.claude-sonnet-4-6","input":"hello"}"#,
    )
    .unwrap();
    let settings = json!({});
    let shaped = AwsBedrockChannel.shape_request(
        Bytes::from(transformed),
        &mut HeaderMap::new(),
        &ShapeCtx {
            op: target,
            stream: false,
            status: StatusCode::OK,
            settings: &settings,
        },
    );
    let shaped: Value = serde_json::from_slice(&shaped).unwrap();
    assert_eq!(shaped["anthropic_version"], "bedrock-2023-05-31");
    assert_eq!(shaped["anthropic_beta"][0], "compact-2026-01-12");
    assert_eq!(
        shaped["context_management"]["edits"][0]["type"],
        "compact_20260112"
    );
}
