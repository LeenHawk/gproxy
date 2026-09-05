use bytes::Bytes;
use gproxy_channel_api::{Channel, ChannelSupport, PrepareCtx};
use gproxy_protocol::{ContentGenerationKind as Kind, Operation, OperationKey, WireFamily};
use http::{HeaderMap, HeaderValue, Method};
use serde_json::{Value, json};

use super::OpenCodeChannel;

const fn family(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::OpenAi)
}

const fn content(operation: Operation, kind: Kind) -> OperationKey {
    OperationKey::content(operation, kind)
}

#[test]
fn declares_truthful_merged_operations() {
    let expected = [
        ChannelSupport::passthrough(family(Operation::ListModels)),
        ChannelSupport::transform(
            OperationKey::family(Operation::ListModels, WireFamily::Claude),
            family(Operation::ListModels),
        ),
        ChannelSupport::passthrough(content(Operation::GenerateContent, Kind::OpenAiChat)),
        ChannelSupport::passthrough(content(Operation::GenerateContent, Kind::OpenAiResponses)),
        ChannelSupport::passthrough(content(Operation::GenerateContent, Kind::ClaudeMessages)),
        ChannelSupport::transform(
            content(Operation::GenerateContent, Kind::GeminiGenerateContent),
            content(Operation::GenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::passthrough(content(Operation::StreamGenerateContent, Kind::OpenAiChat)),
        ChannelSupport::passthrough(content(
            Operation::StreamGenerateContent,
            Kind::OpenAiResponses,
        )),
        ChannelSupport::passthrough(content(
            Operation::StreamGenerateContent,
            Kind::ClaudeMessages,
        )),
        ChannelSupport::transform(
            content(
                Operation::StreamGenerateContent,
                Kind::GeminiGenerateContent,
            ),
            content(Operation::StreamGenerateContent, Kind::OpenAiChat),
        ),
    ];
    assert_eq!(OpenCodeChannel.descriptor().supports, expected);
    assert_eq!(crate::canonical_channel_id("opencodezen"), "opencode");
    assert_eq!(crate::canonical_channel_id("opencodego"), "opencode");
    let migrated =
        crate::canonical_provider_settings("opencodego", &json!({"tier":"zen","future":true}))
            .unwrap();
    assert_eq!(migrated["tier"], "go");
    assert_eq!(migrated["future"], true);
    assert!(crate::canonical_provider_settings("opencode", &json!({"tier":"typo"})).is_err());
}

#[test]
fn resolves_tier_defaults_and_exact_override() {
    let mut headers = HeaderMap::new();
    headers.insert("accept", HeaderValue::from_static("text/event-stream"));
    let secret = json!({"api_key":"oc-key"});
    let zen_settings = json!({});
    let zen = OpenCodeChannel
        .prepare(PrepareCtx {
            session_id: Some("opencode-test-session"),
            key: family(Operation::ListModels),
            stream: false,
            method: &Method::GET,
            path: "/v1/models",
            query: None,
            headers: &headers,
            body: &Bytes::new(),
            upstream_model: "",
            provider_settings: &zen_settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(zen.request.uri(), "https://opencode.ai/zen/v1/models");

    let go_settings = json!({"tier":"go"});
    let go = OpenCodeChannel
        .prepare(PrepareCtx {
            session_id: Some("opencode-test-session"),
            key: content(Operation::GenerateContent, Kind::OpenAiChat),
            stream: false,
            method: &Method::POST,
            path: "/v1/chat/completions",
            query: None,
            headers: &headers,
            body: &Bytes::from_static(br#"{"model":"client","messages":[]}"#),
            upstream_model: "qwen3-coder",
            provider_settings: &go_settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        go.request.uri(),
        "https://opencode.ai/zen/go/v1/chat/completions"
    );

    let settings = json!({
        "base_url":"https://ignored.example",
        "endpoints":{"claude_messages":"https://override.example/messages/{model}"}
    });
    let exact = OpenCodeChannel
        .prepare(PrepareCtx {
            session_id: Some("opencode-test-session"),
            key: content(Operation::GenerateContent, Kind::ClaudeMessages),
            stream: false,
            method: &Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &headers,
            body: &Bytes::from_static(br#"{"model":"client","messages":[]}"#),
            upstream_model: "claude/sonnet",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        exact.request.uri(),
        "https://override.example/messages/claude%2Fsonnet"
    );
    assert_eq!(exact.request.headers()["x-api-key"], "oc-key");
    assert_eq!(exact.request.headers()["accept"], "text/event-stream");
}

#[test]
fn leaves_claude_cache_markers_to_the_central_pass() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_static("context-1m-2025-08-07"),
    );
    let settings = json!({});
    let secret = json!({"api_key":"oc-key"});
    let prepared = OpenCodeChannel
        .prepare(PrepareCtx {
            session_id: Some("opencode-test-session"),
            key: content(Operation::GenerateContent, Kind::ClaudeMessages),
            stream: false,
            method: &Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &headers,
            body: &Bytes::from_static(concat!(
                r#"{"model":"client","messages":[{"role":"assistant","content":[{"type":"text","text":"prefix "#,
                r#"GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF"}]}],"temperature":0.7,"top_p":0.9,"top_k":40}"#
            ).as_bytes()),
            upstream_model: "claude-sonnet-4-6",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    let body: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert_eq!(body["model"], "claude-sonnet-4-6");
    assert_eq!(body["messages"][0]["role"], "assistant");
    // Claude markers are shaped once, centrally, after the process rules; the
    // channel must not take a second pass at them.
    assert!(body["messages"][0]["content"][0]["cache_control"].is_null());
    assert!(
        body["messages"][0]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("GPROXY_MAGIC_STRING")
    );
    assert_eq!(body["temperature"], 0.7);
    assert_eq!(body["top_p"], 0.9);
    assert_eq!(body["top_k"], 40);
    assert!(prepared.request.headers().get("anthropic-beta").is_none());

    let raw = Bytes::from_static(br#"{ "model":"same", "messages":[] }"#);
    let disabled = json!({});
    let ctx = PrepareCtx {
        session_id: Some("opencode-test-session"),
        key: content(Operation::GenerateContent, Kind::ClaudeMessages),
        stream: false,
        method: &Method::POST,
        path: "/v1/messages",
        query: None,
        headers: &HeaderMap::new(),
        body: &raw,
        upstream_model: "same",
        provider_settings: &disabled,
        secret: &secret,
    };
    assert_eq!(
        super::shape::request(&ctx, &mut HeaderMap::new(), raw.clone()).unwrap(),
        raw
    );
}

#[test]
fn session_header_covers_all_inference_formats_and_preserves_client_identity() {
    let secret = json!({"api_key":"oc-key"});
    for tier in ["go", "zen"] {
        let settings = json!({"tier":tier});
        for kind in [
            Kind::OpenAiChat,
            Kind::OpenAiResponses,
            Kind::ClaudeMessages,
        ] {
            for stream in [false, true] {
                let operation = if stream {
                    Operation::StreamGenerateContent
                } else {
                    Operation::GenerateContent
                };
                let body =
                    Bytes::from_static(br#"{"model":"test","messages":[],"input":"question"}"#);
                for explicit in [false, true] {
                    let mut headers = HeaderMap::new();
                    if explicit {
                        headers.insert(
                            "x-opencode-session",
                            HeaderValue::from_static("client-conversation"),
                        );
                    }
                    let prepared = OpenCodeChannel
                        .prepare(PrepareCtx {
                            key: content(operation, kind),
                            session_id: Some("core-conversation"),
                            stream,
                            method: &Method::POST,
                            path: "/",
                            query: None,
                            headers: &headers,
                            body: &body,
                            upstream_model: "test",
                            provider_settings: &settings,
                            secret: &secret,
                        })
                        .unwrap();
                    assert_eq!(
                        prepared.request.headers()["x-opencode-session"],
                        if explicit {
                            "client-conversation"
                        } else {
                            "core-conversation"
                        }
                    );
                }
            }
        }
    }
    let missing = OpenCodeChannel.prepare(PrepareCtx {
        key: content(Operation::GenerateContent, Kind::OpenAiResponses),
        session_id: None,
        stream: false,
        method: &Method::POST,
        path: "/v1/responses",
        query: None,
        headers: &HeaderMap::new(),
        body: &Bytes::from_static(br#"{"previous_response_id":"prior","input":"next"}"#),
        upstream_model: "test",
        provider_settings: &json!({}),
        secret: &secret,
    });
    assert!(
        matches!(missing, Err(gproxy_channel_api::ChannelError::Prepare(message)) if message.contains("x-opencode-session"))
    );
}
