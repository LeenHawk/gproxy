use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx, PreparedRequest};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};
use http::{HeaderMap, Method};
use serde_json::{Value, json};

use super::AzureChannel;

fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

fn prepare(
    key: OperationKey,
    model: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &Bytes,
    settings: &Value,
) -> PreparedRequest {
    let secret = json!({"api_key":"azure-key"});
    AzureChannel
        .prepare(PrepareCtx {
            key,
            stream: key.operation == Operation::StreamGenerateContent,
            method: &Method::PATCH,
            path: "/client/path",
            query,
            headers,
            body,
            upstream_model: model,
            provider_settings: settings,
            secret: &secret,
        })
        .unwrap()
}

#[test]
fn declares_exactly_the_verified_native_targets_and_pairs() {
    use ContentGenerationKind as C;
    use Operation as O;
    use WireFamily as W;

    let supports = AzureChannel.descriptor().supports;
    let native = [
        family(O::ListModels, W::OpenAi),
        family(O::GetModel, W::OpenAi),
        family(O::CountTokens, W::Claude),
        content(O::GenerateContent, C::OpenAiChat),
        content(O::StreamGenerateContent, C::OpenAiChat),
        content(O::GenerateContent, C::OpenAiResponses),
        content(O::StreamGenerateContent, C::OpenAiResponses),
        content(O::GenerateContent, C::ClaudeMessages),
        content(O::StreamGenerateContent, C::ClaudeMessages),
        family(O::CreateEmbedding, W::OpenAi),
        family(O::CreateImage, W::OpenAi),
        family(O::EditImage, W::OpenAi),
    ];
    let pairs = [
        (
            family(O::ListModels, W::Claude),
            family(O::ListModels, W::OpenAi),
        ),
        (
            family(O::GetModel, W::Claude),
            family(O::GetModel, W::OpenAi),
        ),
        (
            content(O::GenerateContent, C::GeminiGenerateContent),
            content(O::GenerateContent, C::OpenAiChat),
        ),
        (
            content(O::StreamGenerateContent, C::GeminiGenerateContent),
            content(O::StreamGenerateContent, C::OpenAiChat),
        ),
    ];
    assert_eq!(supports.len(), native.len() + pairs.len());
    assert!(native.iter().all(|key| {
        supports
            .iter()
            .any(|row| row.source == *key && row.target == *key)
    }));
    assert!(pairs.iter().all(|(source, target)| {
        supports
            .iter()
            .any(|row| row.source == *source && row.target == *target)
    }));
}

#[test]
fn resolves_documented_base_paths_and_an_exact_claude_endpoint() {
    let empty = Bytes::new();
    let listed = prepare(
        family(Operation::ListModels, WireFamily::OpenAi),
        "",
        Some("after=model-1&limit=20&ignored=yes"),
        &HeaderMap::new(),
        &empty,
        &json!({"base_url":"https://resource.openai.azure.com/"}),
    );
    assert_eq!(
        listed.request.uri(),
        "https://resource.openai.azure.com/openai/v1/models?after=model-1&limit=20"
    );
    assert_eq!(listed.request.method(), Method::GET);

    let mut headers = HeaderMap::new();
    headers.insert("authorization", "Bearer downstream".parse().unwrap());
    headers.insert("anthropic-beta", "feature-x".parse().unwrap());
    headers.insert("content-type", "application/json".parse().unwrap());
    let body = Bytes::from_static(br#"{"model":"route","max_tokens":8,"messages":[]}"#);
    let claude = prepare(
        content(
            Operation::GenerateContent,
            ContentGenerationKind::ClaudeMessages,
        ),
        "claude-deployment",
        None,
        &headers,
        &body,
        &json!({
            "base_url":"https://unused.example",
            "endpoints":{"claude_messages":"https://claude.example/native?fixed=1"}
        }),
    );
    assert_eq!(
        claude.request.uri(),
        "https://claude.example/native?fixed=1"
    );
    assert_eq!(claude.request.headers()["x-api-key"], "azure-key");
    assert_eq!(claude.request.headers()["anthropic-version"], "2023-06-01");
    assert!(claude.request.headers().get("authorization").is_none());
    let shaped: Value = serde_json::from_slice(claude.request.body()).unwrap();
    assert_eq!(shaped["model"], "claude-deployment");
}

#[test]
fn keeps_create_model_but_uses_deployment_path_for_multipart_edit() {
    let settings = json!({"base_url":"https://resource.openai.azure.com"});
    let create_body = Bytes::from_static(br#"{"model":"route","prompt":"draw"}"#);
    let created = prepare(
        family(Operation::CreateImage, WireFamily::OpenAi),
        "gpt-image-2",
        None,
        &HeaderMap::new(),
        &create_body,
        &settings,
    );
    assert_eq!(
        created.request.uri(),
        "https://resource.openai.azure.com/openai/v1/images/generations?api-version=preview"
    );
    let created_body: Value = serde_json::from_slice(created.request.body()).unwrap();
    assert_eq!(created_body["model"], "gpt-image-2");

    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        "multipart/form-data; boundary=x".parse().unwrap(),
    );
    let edit_body = Bytes::from_static(
        b"--x\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nroute\r\n--x--\r\n",
    );
    let edited = prepare(
        family(Operation::EditImage, WireFamily::OpenAi),
        "image/deployment",
        None,
        &headers,
        &edit_body,
        &settings,
    );
    assert_eq!(
        edited.request.uri(),
        "https://resource.openai.azure.com/openai/deployments/image%2Fdeployment/images/edits?api-version=2025-04-01"
    );
    let edited_body = String::from_utf8_lossy(edited.request.body());
    assert!(edited_body.contains("\r\n\r\nimage/deployment\r\n--x--"));
    assert!(!edited_body.contains("\r\n\r\nroute\r\n--x--"));
}
