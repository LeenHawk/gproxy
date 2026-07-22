use bytes::Bytes;
use http::{HeaderMap, Method};
use serde_json::json;

use super::*;
use crate::protocol::{OperationKey, Provider};

const MAGIC_TRIGGER: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH";

fn prepare(op: OperationKey, model: &str, path: &str, query: Option<&str>) -> http::Request<Bytes> {
    let secret = json!({ "api_key": "azure-key" });
    let settings = json!({ "base_url": "https://resource.openai.azure.com" });
    let headers = HeaderMap::new();
    AzureChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op,
            stream: false,
            upstream_model_id: model,
            method: Method::POST,
            path,
            query,
            headers: &headers,
            body: Bytes::from_static(br#"{"model":"deployment"}"#),
        })
        .unwrap()
        .into_http()
}

#[test]
fn openai_v1_uses_api_key_header() {
    let req = prepare(
        OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::OpenAiResponses,
        ),
        "gpt-deployment",
        "/v1/responses",
        None,
    );
    assert_eq!(
        req.uri().to_string(),
        "https://resource.openai.azure.com/openai/v1/responses"
    );
    assert_eq!(req.headers()["api-key"], "azure-key");
    assert!(!req.headers().contains_key("authorization"));
}

#[test]
fn shapes_openai_magic_cache_for_chat_and_responses() {
    let settings = json!({ "enable_openai_magic_cache": true });
    let cases = [
        (
            ContentGenerationKind::OpenAiChatCompletions,
            format!(
                r#"{{"model":"gpt","messages":[{{"role":"user","content":"stable {MAGIC_TRIGGER}"}}]}}"#
            ),
            "/messages/0/content/0/prompt_cache_breakpoint/mode",
        ),
        (
            ContentGenerationKind::OpenAiResponses,
            format!(r#"{{"model":"gpt","input":"stable {MAGIC_TRIGGER}"}}"#),
            "/input/0/content/0/prompt_cache_breakpoint/mode",
        ),
    ];

    for (kind, body, pointer) in cases {
        let mut headers = HeaderMap::new();
        let shaped = AzureChannel.shape_request(
            Bytes::from(body),
            &mut headers,
            &ShapeCtx {
                op: OperationKey::content_generation(Operation::GenerateContent, kind),
                stream: false,
                status: http::StatusCode::OK,
                settings: &settings,
            },
        );
        let value: serde_json::Value = serde_json::from_slice(&shaped).unwrap();
        assert_eq!(value.pointer(pointer).unwrap(), "explicit");
        assert!(!String::from_utf8_lossy(&shaped).contains(MAGIC_TRIGGER));
    }
}

#[test]
fn claude_uses_anthropic_surface() {
    let req = prepare(
        OperationKey::content_generation(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        "claude-deployment",
        "/v1/messages",
        None,
    );
    assert_eq!(
        req.uri().to_string(),
        "https://resource.openai.azure.com/anthropic/v1/messages"
    );
    assert_eq!(req.headers()["x-api-key"], "azure-key");
    assert_eq!(req.headers()["anthropic-version"], "2023-06-01");
}

#[test]
fn shapes_and_forwards_claude_magic_cache_and_fable_fallback() {
    let secret = json!({ "api_key": "azure-key" });
    let settings = json!({
        "base_url": "https://resource.services.ai.azure.com",
        "enable_claude_magic_cache": true,
        "enable_claude_fable_fallback": true
    });
    let op = OperationKey::content_generation(
        Operation::GenerateContent,
        ContentGenerationKind::ClaudeMessages,
    );
    let mut headers = HeaderMap::new();
    let shaped = AzureChannel.shape_request(
        Bytes::from(format!(
            r#"{{"model":"claude-fable-5","system":[{{"type":"text","text":"stable {MAGIC_TRIGGER}"}}],"messages":[],"max_tokens":32}}"#
        )),
        &mut headers,
        &ShapeCtx {
            op,
            stream: false,
            status: http::StatusCode::OK,
            settings: &settings,
        },
    );
    let value: serde_json::Value = serde_json::from_slice(&shaped).unwrap();
    assert_eq!(value["system"][0]["cache_control"]["type"], "ephemeral");
    assert_eq!(value["fallbacks"], json!([{ "model": "claude-opus-4-8" }]));
    assert!(!String::from_utf8_lossy(&shaped).contains(MAGIC_TRIGGER));
    assert_eq!(headers["anthropic-beta"], "server-side-fallback-2026-06-01");

    let req = AzureChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op,
            stream: false,
            upstream_model_id: "claude-fable-5",
            method: Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &headers,
            body: shaped,
        })
        .unwrap()
        .into_http();
    assert_eq!(
        req.uri().to_string(),
        "https://resource.services.ai.azure.com/anthropic/v1/messages"
    );
    assert_eq!(
        req.headers()["anthropic-beta"],
        "server-side-fallback-2026-06-01"
    );
}

#[test]
fn image_path_uses_deployment_and_default_api_version() {
    let req = prepare(
        OperationKey::provider(Operation::CreateImage, Provider::OpenAi),
        "gpt image/production",
        "/v1/images/generations",
        None,
    );
    assert_eq!(
        req.uri().to_string(),
        "https://resource.openai.azure.com/openai/deployments/gpt%20image%2Fproduction/images/generations?api-version=2025-04-01-preview"
    );
}

#[test]
fn embedding_and_compact_use_openai_v1_paths() {
    for (operation, path) in [
        (Operation::CreateEmbedding, "/v1/embeddings"),
        (Operation::CompactContent, "/v1/responses/compact"),
    ] {
        let req = prepare(
            OperationKey::provider(operation, Provider::OpenAi),
            "deployment",
            path,
            Some("api-version=preview"),
        );
        assert_eq!(
            req.uri().to_string(),
            format!("https://resource.openai.azure.com/openai{path}?api-version=preview")
        );
    }
}

#[test]
fn image_shape_removes_model_from_json_body() {
    let mut headers = HeaderMap::new();
    let body = AzureChannel.shape_request(
        Bytes::from_static(br#"{"model":"gpt-image","prompt":"draw"}"#),
        &mut headers,
        &ShapeCtx {
            op: OperationKey::provider(Operation::CreateImage, Provider::OpenAi),
            stream: false,
            status: http::StatusCode::OK,
            settings: &serde_json::Value::Null,
        },
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
        json!({ "prompt": "draw" })
    );

    let settings = json!({
        "endpoints": { "image_generations": "https://images.example/v1/images/generations" }
    });
    let body = AzureChannel.shape_request(
        Bytes::from_static(br#"{"model":"gpt-image","prompt":"draw"}"#),
        &mut headers,
        &ShapeCtx {
            op: OperationKey::provider(Operation::CreateImage, Provider::OpenAi),
            stream: false,
            status: http::StatusCode::OK,
            settings: &settings,
        },
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
        json!({ "model": "gpt-image", "prompt": "draw" })
    );
}

#[test]
fn exact_image_endpoint_keeps_its_static_api_version() {
    let secret = json!({ "api_key": "azure-key" });
    let settings = json!({
        "endpoints": {
            "image_generations": "https://images.example/generate?api-version=custom"
        }
    });
    let headers = HeaderMap::new();
    let req = AzureChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op: OperationKey::provider(Operation::CreateImage, Provider::OpenAi),
            stream: false,
            upstream_model_id: "deployment",
            method: Method::POST,
            path: "/v1/images/generations",
            query: None,
            headers: &headers,
            body: Bytes::new(),
        })
        .unwrap()
        .into_http();
    assert_eq!(
        req.uri().to_string(),
        "https://images.example/generate?api-version=custom"
    );
}
