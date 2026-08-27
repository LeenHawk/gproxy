use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, StreamFraming, WireFamily};
use http::{HeaderMap, Method};
use serde_json::{Value, json};

use super::CustomChannel;

const MAGIC: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH";

fn content(operation: Operation, kind: ContentGenerationKind) -> OperationKey {
    OperationKey::content(operation, kind)
}

#[test]
fn declares_exactly_the_classified_universal_wire_cells() {
    let supports = CustomChannel.descriptor().supports;
    assert_eq!(supports.len(), 37);
    assert!(
        supports
            .iter()
            .all(|support| support.source == support.target)
    );
    for family in [WireFamily::OpenAi, WireFamily::Claude, WireFamily::Gemini] {
        for operation in [
            Operation::ListModels,
            Operation::GetModel,
            Operation::CountTokens,
        ] {
            assert!(
                supports
                    .iter()
                    .any(|support| { support.source == OperationKey::family(operation, family) })
            );
        }
    }
    assert!(!supports.iter().any(|support| {
        support.source == OperationKey::family(Operation::CreateFile, WireFamily::OpenAi)
    }));
}

#[test]
fn resolves_base_and_exact_endpoints_with_target_wire_auth() {
    let secret = json!({"api_key":"upstream-key"});
    let settings = json!({"base_url":"https://gemini.example/"});
    let body = Bytes::from_static(br#"{"model":"route","contents":[]}"#);
    let gemini = CustomChannel
        .prepare(PrepareCtx {
            key: content(
                Operation::StreamGenerateContent,
                ContentGenerationKind::GeminiGenerateContent,
            ),
            stream: true,
            method: &Method::POST,
            path: "/v1beta/models/route:streamGenerateContent",
            query: Some("alt=sse&key=downstream&ignored=1"),
            headers: &HeaderMap::new(),
            body: &body,
            upstream_model: "models/gemini-3-flash",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        gemini.request.uri(),
        "https://gemini.example/v1beta/models/gemini-3-flash:streamGenerateContent?alt=sse"
    );
    assert_eq!(gemini.request.headers()["x-goog-api-key"], "upstream-key");
    assert_eq!(gemini.framing, Some(StreamFraming::Sse));

    let exact = json!({
        "endpoints":{"claude_messages":"https://claude.example/{model}?fixed=1"}
    });
    let claude_body = Bytes::from_static(br#"{"model":"route","max_tokens":8,"messages":[]}"#);
    let claude = CustomChannel
        .prepare(PrepareCtx {
            key: content(
                Operation::GenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
            stream: false,
            method: &Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &HeaderMap::new(),
            body: &claude_body,
            upstream_model: "claude/model one",
            provider_settings: &exact,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        claude.request.uri(),
        "https://claude.example/claude%2Fmodel%20one?fixed=1"
    );
    assert_eq!(claude.request.headers()["x-api-key"], "upstream-key");
    assert_eq!(claude.request.headers()["anthropic-version"], "2023-06-01");
}

#[test]
fn applies_only_the_enabled_protocol_cache_and_fallback_shaping() {
    let secret = json!({"api_key":"key"});
    let openai_settings = json!({
        "base_url":"https://openai.example",
        "enable_openai_magic_cache":true
    });
    let openai_body = Bytes::from(
        json!({
            "model":"route",
            "messages":[{"role":"user","content":format!("stable {MAGIC}")}]
        })
        .to_string(),
    );
    let openai = CustomChannel
        .prepare(PrepareCtx {
            key: content(
                Operation::GenerateContent,
                ContentGenerationKind::OpenAiChat,
            ),
            stream: false,
            method: &Method::POST,
            path: "/v1/chat/completions",
            query: None,
            headers: &HeaderMap::new(),
            body: &openai_body,
            upstream_model: "gpt-custom",
            provider_settings: &openai_settings,
            secret: &secret,
        })
        .unwrap();
    let openai: Value = serde_json::from_slice(openai.request.body()).unwrap();
    assert_eq!(
        openai["messages"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
        "explicit"
    );
    assert!(
        !openai["messages"][0]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains(MAGIC)
    );

    let claude_settings = json!({
        "base_url":"https://claude.example",
        "enable_claude_magic_cache":true,
        "claude_fallback_mode":"default"
    });
    let claude_body = Bytes::from(
        json!({
            "model":"route","max_tokens":8,
            "messages":[{"role":"user","content":[{"type":"text","text":format!("stable {MAGIC}")}]}]
        })
        .to_string(),
    );
    let claude = CustomChannel
        .prepare(PrepareCtx {
            key: content(
                Operation::GenerateContent,
                ContentGenerationKind::ClaudeMessages,
            ),
            stream: false,
            method: &Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &HeaderMap::new(),
            body: &claude_body,
            upstream_model: "claude-fable-5",
            provider_settings: &claude_settings,
            secret: &secret,
        })
        .unwrap();
    let value: Value = serde_json::from_slice(claude.request.body()).unwrap();
    assert_eq!(
        value["messages"][0]["content"][0]["cache_control"]["type"],
        "ephemeral"
    );
    assert_eq!(value["fallbacks"], "default");
    assert_eq!(
        claude.request.headers()["anthropic-beta"],
        "server-side-fallback-2026-07-01"
    );
}
