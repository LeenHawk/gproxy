//! DashScope operations, endpoint, and image shaping budget tests.

mod support;

use bytes::Bytes;
use gproxy_channel_api::{Channel, ResponseShapeCtx, UsageCtx};
use gproxy_protocol::{ContentGenerationKind as C, Operation as O, WireFamily as W};
use http::{HeaderMap, Method, StatusCode};
use rust_decimal::Decimal;
use serde_json::{Value, json};

use super::DashScopeChannel;
use support::{content, family, prepare};

#[test]
fn declares_exactly_verified_native_operations_and_existing_pairs() {
    let native = [
        family(O::ListModels, W::OpenAi),
        family(O::GetModel, W::OpenAi),
        content(O::GenerateContent, C::OpenAiChat),
        content(O::StreamGenerateContent, C::OpenAiChat),
        content(O::GenerateContent, C::OpenAiResponses),
        content(O::StreamGenerateContent, C::OpenAiResponses),
        content(O::GenerateContent, C::ClaudeMessages),
        content(O::StreamGenerateContent, C::ClaudeMessages),
        family(O::CreateEmbedding, W::OpenAi),
        family(O::Rerank, W::OpenAi),
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
            content(O::GenerateContent, C::OpenAiResponses),
        ),
        (
            content(O::StreamGenerateContent, C::GeminiGenerateContent),
            content(O::StreamGenerateContent, C::OpenAiResponses),
        ),
    ];
    let supports = DashScopeChannel.descriptor().supports;
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
fn resolves_documented_surfaces_and_exact_override_with_bearer_auth() {
    let chat_body = Bytes::from_static(br#"{"model":"route","messages":[]}"#);
    let chat = prepare(
        content(O::GenerateContent, C::OpenAiChat),
        "qwen-plus",
        &chat_body,
        &json!({}),
    );
    assert_eq!(
        chat.request.uri(),
        "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
    );
    assert_eq!(chat.request.method(), Method::POST);
    assert_eq!(
        chat.request.headers()["authorization"],
        "Bearer dashscope-key"
    );
    let chat_body: Value = serde_json::from_slice(chat.request.body()).unwrap();
    assert_eq!(chat_body["model"], "qwen-plus");

    let claude_body = Bytes::from_static(br#"{"model":"route","messages":[],"max_tokens":8}"#);
    let claude = prepare(
        content(O::GenerateContent, C::ClaudeMessages),
        "qwen3-max",
        &claude_body,
        &json!({}),
    );
    assert_eq!(
        claude.request.uri(),
        "https://dashscope.aliyuncs.com/apps/anthropic/v1/messages"
    );
    assert_eq!(
        claude.request.headers()["authorization"],
        "Bearer dashscope-key"
    );
    assert!(claude.request.headers().get("x-api-key").is_none());

    let rerank_body = Bytes::from_static(br#"{"model":"route","query":"q","documents":["a"]}"#);
    let rerank = prepare(
        family(O::Rerank, W::OpenAi),
        "gte/rerank",
        &rerank_body,
        &json!({
            "base_url":"https://unused.example",
            "endpoints":{"openai_rerank":"https://rank.example/{model}"}
        }),
    );
    assert_eq!(rerank.request.uri(), "https://rank.example/gte%2Frerank");
}

#[test]
fn preserves_image_parameters_and_observes_raw_usage_without_created() {
    let settings = json!({});
    let create_body = Bytes::from_static(
        br#"{"model":"route","prompt":"cat","n":2,"size":"1024x1024","seed":7,"thinking_mode":true,"color_palette":["blue"]}"#,
    );
    let created = prepare(
        family(O::CreateImage, W::OpenAi),
        "qwen-image-3.0-pro",
        &create_body,
        &settings,
    );
    assert_eq!(
        created.request.uri(),
        "https://dashscope.aliyuncs.com/api/v1/services/aigc/multimodal-generation/generation"
    );
    let shaped: Value = serde_json::from_slice(created.request.body()).unwrap();
    assert_eq!(shaped["model"], "qwen-image-3.0-pro");
    assert_eq!(shaped["input"]["messages"][0]["content"][0]["text"], "cat");
    assert_eq!(shaped["parameters"]["size"], "1024*1024");
    assert_eq!(shaped["parameters"]["n"], 2);
    assert_eq!(shaped["parameters"]["seed"], 7);
    assert_eq!(shaped["parameters"]["thinking_mode"], true);
    assert_eq!(shaped["parameters"]["watermark"], false);

    let edit_body = Bytes::from_static(
        br#"{"model":"route","prompt":"blue","image":"data:image/png;base64,AAEC","bbox_list":[[0,0,1,1]]}"#,
    );
    let edited = prepare(
        family(O::EditImage, W::OpenAi),
        "wan2.7-image",
        &edit_body,
        &settings,
    );
    let edited: Value = serde_json::from_slice(edited.request.body()).unwrap();
    assert_eq!(
        edited["input"]["messages"][0]["content"][0]["image"],
        "data:image/png;base64,AAEC"
    );
    assert_eq!(edited["input"]["messages"][0]["content"][1]["text"], "blue");
    assert!(edited["parameters"]["bbox_list"].is_array());

    let raw = Bytes::from_static(br#"{"output":{"choices":[{"message":{"content":[{"image":"https://example.com/out.png"}]}}]},"usage":{"input_tokens":12,"output_tokens":4,"image_count":1,"size":"1024*1024"},"request_id":"req-1"}"#);
    let headers = HeaderMap::new();
    let key = family(O::CreateImage, W::OpenAi);
    let usage = DashScopeChannel
        .extract_usage(UsageCtx {
            key,
            request_body: &create_body,
            response_headers: &headers,
            response_body: &raw,
        })
        .unwrap();
    assert_eq!((usage.input_tokens, usage.output_tokens), (12, 4));
    assert_eq!(usage.metrics["image_outputs"], Decimal::ONE);
    assert_eq!(usage.dimensions["size"], "1024*1024");
    let outward = DashScopeChannel
        .shape_response(ResponseShapeCtx {
            key,
            status: StatusCode::OK,
            headers: &headers,
            body: &raw,
        })
        .unwrap();
    let outward: Value = serde_json::from_slice(&outward).unwrap();
    assert_eq!(outward["data"][0]["url"], "https://example.com/out.png");
    assert_eq!(outward["request_id"], "req-1");
    assert!(outward.get("created").is_none() && outward.get("usage").is_none());
    assert_eq!(outward["dashscope_usage"]["image_count"], 1);
}
