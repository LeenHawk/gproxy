use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind as Kind, Operation, OperationKey, WireFamily};
use http::{HeaderMap, Method};
use serde_json::{Value, json};

use super::OpenRouterChannel;

const CHAT: OperationKey = OperationKey::content(Operation::GenerateContent, Kind::OpenAiChat);
const MAGIC: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH";

fn family(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::OpenAi)
}

fn prepare(
    key: OperationKey,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &Bytes,
    model: &str,
    settings: &Value,
) -> http::Request<Bytes> {
    let method = if matches!(
        key.operation,
        Operation::ListModels
            | Operation::GetModel
            | Operation::RetrieveVideo
            | Operation::DownloadVideoContent
    ) {
        &Method::GET
    } else {
        &Method::POST
    };
    OpenRouterChannel
        .prepare(PrepareCtx {
            key,
            stream: key.operation == Operation::StreamGenerateContent,
            method,
            path,
            query,
            headers,
            body,
            upstream_model: model,
            provider_settings: settings,
            secret: &json!({"api_key":"or-test"}),
        })
        .unwrap()
        .request
}

#[test]
fn descriptor_declares_exact_native_operations() {
    let descriptor = OpenRouterChannel.descriptor();
    assert_eq!(descriptor.id, "openrouter");
    assert_eq!(descriptor.supports.len(), 17);
    assert_eq!(
        descriptor
            .supports
            .iter()
            .filter(|support| support.source == support.target)
            .count(),
        17
    );
    for key in [
        family(Operation::ListModels),
        family(Operation::GetModel),
        OperationKey::content(Operation::GenerateContent, Kind::OpenAiChat),
        OperationKey::content(Operation::StreamGenerateContent, Kind::OpenAiChat),
        OperationKey::content(Operation::GenerateContent, Kind::OpenAiResponses),
        OperationKey::content(Operation::StreamGenerateContent, Kind::OpenAiResponses),
        OperationKey::content(Operation::GenerateContent, Kind::ClaudeMessages),
        OperationKey::content(Operation::StreamGenerateContent, Kind::ClaudeMessages),
        family(Operation::CreateEmbedding),
        family(Operation::Rerank),
        family(Operation::CreateImage),
        family(Operation::EditImage),
        family(Operation::CreateSpeech),
        family(Operation::CreateTranscription),
        family(Operation::CreateVideo),
        family(Operation::RetrieveVideo),
        family(Operation::DownloadVideoContent),
    ] {
        assert!(
            descriptor
                .supports
                .iter()
                .any(|support| support.source == key)
        );
    }
}

#[test]
fn prepare_resolves_default_and_exact_endpoints_with_safe_headers_and_query() {
    let mut headers = HeaderMap::new();
    headers.insert("http-referer", "https://app.example".parse().unwrap());
    headers.insert("x-title", "Example".parse().unwrap());
    headers.insert("cookie", "drop=me".parse().unwrap());
    let empty = Bytes::new();
    let default = prepare(
        family(Operation::DownloadVideoContent),
        "/v1/videos/job_1/content",
        Some("index=2&key=downstream"),
        &headers,
        &empty,
        "",
        &json!({}),
    );
    assert_eq!(
        default.uri(),
        "https://openrouter.ai/api/v1/videos/job_1/content?index=2"
    );
    assert_eq!(default.headers()["authorization"], "Bearer or-test");
    assert_eq!(default.headers()["http-referer"], "https://app.example");
    assert!(default.headers().get("cookie").is_none());

    let exact = prepare(
        family(Operation::GetModel),
        "/v1/models/public",
        None,
        &headers,
        &empty,
        "anthropic/claude-sonnet-5",
        &json!({
            "base_url":"https://unused.example",
            "endpoints":{"openai_get_model":"https://relay.example/models/{model}"}
        }),
    );
    assert_eq!(
        exact.uri(),
        "https://relay.example/models/anthropic%2Fclaude-sonnet-5"
    );
}

#[test]
fn prepare_applies_service_tier_image_edit_and_video_wire_shaping() {
    let chat_body = Bytes::from_static(br#"{"model":"route","messages":[],"service_tier":"fast"}"#);
    let chat = prepare(
        CHAT,
        "/v1/chat/completions",
        None,
        &HeaderMap::new(),
        &chat_body,
        "openai/gpt-5.6",
        &json!({}),
    );
    let chat: Value = serde_json::from_slice(chat.body()).unwrap();
    assert_eq!(chat["model"], "openai/gpt-5.6");
    assert_eq!(chat["service_tier"], "priority");

    let claude_body = Bytes::from(
        json!({
            "model":"route","max_tokens":8,
            "messages":[{"role":"user","content":[{"type":"text","text":format!("stable {MAGIC}")}]}]
        })
        .to_string(),
    );
    let claude = prepare(
        OperationKey::content(Operation::GenerateContent, Kind::ClaudeMessages),
        "/v1/messages",
        None,
        &HeaderMap::new(),
        &claude_body,
        "anthropic/claude-fable-5",
        &json!({"enable_claude_magic_cache":true,"claude_fable_fallbacks":"default"}),
    );
    let claude_value: Value = serde_json::from_slice(claude.body()).unwrap();
    assert_eq!(
        claude_value["messages"][0]["content"][0]["cache_control"]["type"],
        "ephemeral"
    );
    assert_eq!(
        claude_value["fallbacks"][0]["model"],
        "anthropic/claude-opus-4-8"
    );
    assert!(claude.headers().get("anthropic-beta").is_none());

    let mut multipart_headers = HeaderMap::new();
    multipart_headers.insert(
        "content-type",
        "multipart/form-data; boundary=x".parse().unwrap(),
    );
    let multipart = Bytes::from_static(
        b"--x\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nroute\r\n--x\r\nContent-Disposition: form-data; name=\"prompt\"\r\n\r\nedit\r\n--x\r\nContent-Disposition: form-data; name=\"image\"; filename=\"a.png\"\r\nContent-Type: image/png\r\n\r\n\x00\xff\r\n--x\r\nContent-Disposition: form-data; name=\"mask\"; filename=\"m.png\"\r\nContent-Type: image/png\r\n\r\nmask\r\n--x--\r\n",
    );
    let image = prepare(
        family(Operation::EditImage),
        "/v1/images/edits",
        None,
        &multipart_headers,
        &multipart,
        "google/gemini-image",
        &json!({}),
    );
    assert_eq!(image.uri(), "https://openrouter.ai/api/v1/images");
    assert_eq!(image.headers()["content-type"], "application/json");
    let image: Value = serde_json::from_slice(image.body()).unwrap();
    assert_eq!(image["model"], "google/gemini-image");
    assert_eq!(image["input_references"][0]["type"], "image_url");
    assert!(image.get("mask").is_none() && image.get("images").is_none());

    let video = Bytes::from_static(
        br#"{"model":"route","prompt":"cat","seconds":"8","input_reference":"https://example/a.png"}"#,
    );
    let video = prepare(
        family(Operation::CreateVideo),
        "/v1/videos",
        None,
        &HeaderMap::new(),
        &video,
        "google/veo-3.1",
        &json!({}),
    );
    let video: Value = serde_json::from_slice(video.body()).unwrap();
    assert_eq!(video["duration"], 8);
    assert_eq!(
        video["input_references"][0]["image_url"]["url"],
        "https://example/a.png"
    );
}
