use bytes::Bytes;
use gproxy_channel_api::{Channel, ChannelSupport, PrepareCtx, StreamCtx, StreamEnd};
use gproxy_protocol::{
    ContentGenerationKind as Kind, Operation, OperationKey, StreamFraming, WireFamily,
};
use http::{HeaderMap, HeaderValue, Method};
use serde_json::{Value, json};

use super::VercelChannel;

const fn family(operation: Operation, family: WireFamily) -> OperationKey {
    OperationKey::family(operation, family)
}

const fn content(operation: Operation, kind: Kind) -> OperationKey {
    OperationKey::content(operation, kind)
}

#[test]
fn gateway_fallback_is_not_injected_as_an_anthropic_parameter() {
    for (key, _) in [
        (family(Operation::CountTokens, WireFamily::Claude), false),
        (
            content(Operation::GenerateContent, Kind::ClaudeMessages),
            true,
        ),
        (
            content(Operation::StreamGenerateContent, Kind::ClaudeMessages),
            true,
        ),
    ] {
        let mut headers = HeaderMap::new();
        let body = Bytes::from_static(br#"{"model":"anthropic/claude-fable-5","messages":[]}"#);
        let shaped = super::shape::request(
            &PrepareCtx {
                session_id: None,
                key,
                stream: key.operation() == Operation::StreamGenerateContent,
                method: &Method::POST,
                path: "/v1/messages",
                query: None,
                headers: &HeaderMap::new(),
                body: &body,
                upstream_model: "anthropic/claude-fable-5",
                provider_settings: &json!({"claude_fallback_mode":"default"}),
                secret: &Value::Null,
            },
            &mut headers,
            body.clone(),
        )
        .unwrap();
        let shaped: Value = serde_json::from_slice(&shaped).unwrap();
        assert!(shaped.get("fallbacks").is_none());
        assert!(!headers.contains_key("anthropic-beta"));
    }
}

#[test]
fn declares_truthful_operations() {
    let supports = VercelChannel.descriptor().supports;
    assert_eq!(supports.len(), 15);
    for support in [
        ChannelSupport::transform(
            family(Operation::ListModels, WireFamily::Claude),
            family(Operation::ListModels, WireFamily::OpenAi),
        ),
        ChannelSupport::transform(
            family(Operation::CountTokens, WireFamily::OpenAi),
            family(Operation::CountTokens, WireFamily::Claude),
        ),
        ChannelSupport::passthrough(content(Operation::GenerateContent, Kind::OpenAiChat)),
        ChannelSupport::passthrough(content(Operation::GenerateContent, Kind::OpenAiResponses)),
        ChannelSupport::passthrough(content(Operation::GenerateContent, Kind::ClaudeMessages)),
        ChannelSupport::transform(
            content(Operation::GenerateContent, Kind::GeminiGenerateContent),
            content(Operation::GenerateContent, Kind::OpenAiResponses),
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
            content(Operation::StreamGenerateContent, Kind::OpenAiResponses),
        ),
        ChannelSupport::passthrough(family(Operation::CreateEmbedding, WireFamily::OpenAi)),
    ] {
        assert!(supports.contains(&support), "missing {support:?}");
    }
    assert!(
        supports
            .iter()
            .all(|support| support.source.operation() != Operation::CompactContent)
    );
}

#[test]
fn resolves_default_and_exact_override_with_dual_auth() {
    let headers = HeaderMap::new();
    let secret = json!({"api_key":"vercel-key"});
    let defaults = json!({});
    let claude = VercelChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: content(Operation::GenerateContent, Kind::ClaudeMessages),
            stream: false,
            method: &Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &headers,
            body: &Bytes::from_static(br#"{"model":"client","messages":[]}"#),
            upstream_model: "anthropic/claude-sonnet-4-6",
            provider_settings: &defaults,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        claude.request.uri(),
        "https://ai-gateway.vercel.sh/v1/messages"
    );
    assert_eq!(
        claude.request.headers()["authorization"],
        "Bearer vercel-key"
    );
    assert_eq!(claude.request.headers()["x-api-key"], "vercel-key");

    let settings = json!({
        "base_url":"https://ignored.example",
        "endpoints":{"openai_get_model":"https://override.example/models/{model}"}
    });
    let exact = VercelChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: family(Operation::GetModel, WireFamily::OpenAi),
            stream: false,
            method: &Method::GET,
            path: "/v1/models/client",
            query: None,
            headers: &headers,
            body: &Bytes::new(),
            upstream_model: "openai/gpt 5",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        exact.request.uri(),
        "https://override.example/models/openai%2Fgpt%205"
    );
}

#[test]
fn applies_claude_policy_and_observes_stream_usage() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_static("context-1m-2025-08-07"),
    );
    let settings = json!({});
    let secret = json!({"api_key":"vercel-key"});
    let key = content(Operation::StreamGenerateContent, Kind::ClaudeMessages);
    let prepared = VercelChannel
        .prepare(PrepareCtx {
            session_id: None,
            key,
            stream: true,
            method: &Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &headers,
            body: &Bytes::from_static(concat!(
                r#"{"model":"client","messages":[{"role":"assistant","content":[{"type":"text","text":"prefix "#,
                r#"GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF"}]}],"temperature":0.7,"top_p":0.9,"top_k":40,"stream":true}"#
            ).as_bytes()),
            upstream_model: "anthropic/claude-opus-4-8",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    let body: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert_eq!(body["model"], "anthropic/claude-opus-4-8");
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(
        body["messages"][0]["content"][0]["cache_control"],
        Value::Null
    );
    for field in ["temperature", "top_p", "top_k"] {
        assert!(body.get(field).is_none());
    }
    assert!(prepared.request.headers().get("anthropic-beta").is_none());

    let request_body = prepared.request.body().clone();
    let response_headers = HeaderMap::new();
    let mut decoder = VercelChannel
        .stream_decoder(StreamCtx {
            key,
            framing: StreamFraming::Sse,
            request_body: &request_body,
            response_headers: &response_headers,
        })
        .unwrap();
    decoder.push(Bytes::from_static(
        b"data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":5,\"output_tokens\":0,\"cache_read_input_tokens\":2}}}\n\n",
    )).unwrap();
    decoder
        .push(Bytes::from_static(
            b"data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":3}}\n\n",
        ))
        .unwrap();
    let usage = decoder.finish(StreamEnd::Complete).unwrap().usage.unwrap();
    assert_eq!((usage.input_tokens, usage.output_tokens), (7, 3));
}
