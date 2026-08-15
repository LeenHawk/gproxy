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
            method: if op.operation() == Operation::ListModels {
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
        .unwrap()
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
        br#"{"model":"x","max_tokens":8,"messages":[{"role":"user","content":[{"type":"text","text":"hello","cache_control":{"type":"ephemeral","ttl":"1h"}}]}],"tools":[{"name":"get_weather","description":"Get weather","input_schema":{"type":"object","properties":{"city":{"type":"string"}}}}],"tool_choice":{"type":"tool","name":"get_weather"}}"#,
    ), &mut HeaderMap::new(), &ctx);
    let request: Value = serde_json::from_slice(&request).unwrap();
    assert_eq!(request["inferenceConfig"]["maxTokens"], 8);
    assert_eq!(request["messages"][0]["content"][0]["text"], "hello");
    assert_eq!(
        request["messages"][0]["content"][1]["cachePoint"]["type"],
        "default"
    );
    assert_eq!(
        request["messages"][0]["content"][1]["cachePoint"]["ttl"],
        "1h"
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
        br#"{"output":{"message":{"role":"assistant","content":[{"toolUse":{"toolUseId":"tool_1","name":"get_weather","input":{"city":"Paris"}}}]}},"stopReason":"tool_use","serviceTier":{"type":"priority"},"usage":{"inputTokens":9,"outputTokens":4,"totalTokens":13,"cacheReadInputTokens":2,"cacheWriteInputTokens":3,"cacheDetails":[{"inputTokens":1,"ttl":"5m"},{"inputTokens":2,"ttl":"1h"}]}}"#,
    ), &ctx);
    let response: Value = serde_json::from_slice(&response).unwrap();
    assert_eq!(response["type"], "message");
    assert_eq!(response["content"][0]["type"], "tool_use");
    assert_eq!(response["content"][0]["input"]["city"], "Paris");
    assert_eq!(response["usage"]["input_tokens"], 9);
    assert_eq!(response["usage"]["service_tier"], "priority");
    assert_eq!(response["usage"]["speed"], "fast");
    let usage = crate::usage::extract::from_response(Provider::Claude, &response).unwrap();
    assert_eq!(usage.input, 9);
    assert_eq!(usage.output, 4);
    assert_eq!(usage.cache_read, 2);
    assert_eq!(usage.cache_creation_5m, 1);
    assert_eq!(usage.cache_creation_1h, 2);
}

#[test]
fn openai_fast_becomes_bedrock_priority() {
    let source = OperationKey::content_generation(
        Operation::GenerateContent,
        ContentGenerationKind::OpenAiChatCompletions,
    );
    let target = OperationKey::content_generation(
        Operation::GenerateContent,
        ContentGenerationKind::ClaudeMessages,
    );
    let transformed = dispatch::request_bytes(
        resolve(source, target).unwrap(),
        &TransformContext::new(source, target),
        br#"{"model":"us.anthropic.claude-sonnet-4-6","messages":[{"role":"user","content":"hi"}],"max_completion_tokens":32,"service_tier":"fast"}"#,
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
    assert_eq!(shaped["serviceTier"]["type"], "priority");
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
        Bytes::from_static(br#"{"model":"x","max_tokens":100,"temperature":0.7,"messages":[{"role":"user","content":"hello"}]}"#),
        &mut HeaderMap::new(),
        &ctx,
    );
    let value: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        value["input"]["converse"]["messages"][0]["content"][0]["text"],
        "hello"
    );
    assert!(value["input"]["converse"].get("inferenceConfig").is_none());
}

#[test]
fn incomplete_bedrock_usage_is_not_treated_as_authoritative() {
    let shaped = json!({
        "usage": converse::usage(json!({ "outputTokens": 4 }))
    });
    assert!(crate::usage::extract::from_response(Provider::Claude, &shaped).is_none());
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

#[test]
fn shapes_and_prepares_nova_reel_async_invoke() {
    let op = OperationKey::provider(Operation::CreateVideo, Provider::OpenAi);
    let settings = json!({
        "region": "us-west-2",
        "video_output_s3_uri": "s3://video-output/jobs"
    });
    let ctx = ShapeCtx {
        op,
        stream: false,
        status: StatusCode::OK,
        settings: &settings,
    };
    let shaped = AwsBedrockChannel.shape_request(
        Bytes::from_static(
            br#"{"model":"amazon.nova-reel-v1:1","prompt":"cat","seconds":"6","size":"1280x720","seed":7}"#,
        ),
        &mut HeaderMap::new(),
        &ctx,
    );
    let value: Value = serde_json::from_slice(&shaped).unwrap();
    assert_eq!(value["modelId"], "amazon.nova-reel-v1:1");
    assert_eq!(value["modelInput"]["taskType"], "TEXT_VIDEO");
    assert_eq!(
        value["modelInput"]["videoGenerationConfig"]["durationSeconds"],
        6
    );
    assert_eq!(
        value["outputDataConfig"]["s3OutputDataConfig"]["s3Uri"],
        "s3://video-output/jobs"
    );

    let secret = json!({ "api_key": "bedrock-key" });
    let request = AwsBedrockChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op,
            stream: false,
            upstream_model_id: "amazon.nova-reel-v1:1",
            method: Method::POST,
            path: "/v1/videos",
            query: None,
            headers: &HeaderMap::new(),
            body: shaped,
        })
        .unwrap()
        .into_http()
        .unwrap();
    assert_eq!(
        request.uri(),
        "https://bedrock-runtime.us-west-2.amazonaws.com/async-invoke"
    );
}

#[test]
fn reshapes_and_polls_bedrock_video_job() {
    let settings = json!({ "region": "us-west-2" });
    let arn = "arn:aws:bedrock:us-west-2:123:async-invoke/job-1";
    let id = common::encode_video_task_id(arn);
    let op = OperationKey::provider(Operation::RetrieveVideo, Provider::OpenAi);
    let secret = json!({ "api_key": "bedrock-key" });
    let request = AwsBedrockChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op,
            stream: false,
            upstream_model_id: "amazon.nova-reel-v1:1",
            method: Method::GET,
            path: &format!("/v1/videos/{id}"),
            query: None,
            headers: &HeaderMap::new(),
            body: Bytes::new(),
        })
        .unwrap()
        .into_http()
        .unwrap();
    assert_eq!(request.method(), Method::GET);
    assert!(request.uri().path().contains("arn%3Aaws%3Abedrock"));

    let ctx = ShapeCtx {
        op,
        stream: false,
        status: StatusCode::OK,
        settings: &settings,
    };
    let response = AwsBedrockChannel.shape_response(
        Bytes::from(
            json!({
                "invocationArn": arn,
                "status": "Completed",
                "outputDataConfig": {
                    "s3OutputDataConfig": { "s3Uri": "s3://video-output/jobs" }
                }
            })
            .to_string(),
        ),
        &ctx,
    );
    let value: Value = serde_json::from_slice(&response).unwrap();
    assert_eq!(value["id"], id);
    assert_eq!(value["status"], "completed");
    assert_eq!(value["url"], "s3://video-output/jobs/job-1/output.mp4");
}
