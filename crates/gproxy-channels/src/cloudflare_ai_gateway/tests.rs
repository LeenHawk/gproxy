use bytes::Bytes;
use gproxy_channel_api::{Channel, ChannelSupport, PrepareCtx, StreamCtx, StreamEnd};
use gproxy_protocol::{ContentGenerationKind as Kind, Operation, OperationKey, StreamFraming};
use http::{HeaderMap, HeaderValue, Method};
use serde_json::{Value, json};

use super::CloudflareAiGatewayChannel;

const fn content(operation: Operation, kind: Kind) -> OperationKey {
    OperationKey::content(operation, kind)
}

#[test]
fn declares_native_content_and_available_gemini_pairs() {
    let expected = [
        ChannelSupport::passthrough(content(Operation::GenerateContent, Kind::OpenAiChat)),
        ChannelSupport::passthrough(content(Operation::GenerateContent, Kind::OpenAiResponses)),
        ChannelSupport::passthrough(content(Operation::GenerateContent, Kind::ClaudeMessages)),
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
            content(Operation::GenerateContent, Kind::GeminiGenerateContent),
            content(Operation::GenerateContent, Kind::OpenAiChat),
        ),
        ChannelSupport::transform(
            content(
                Operation::StreamGenerateContent,
                Kind::GeminiGenerateContent,
            ),
            content(Operation::StreamGenerateContent, Kind::OpenAiChat),
        ),
    ];
    assert_eq!(
        CloudflareAiGatewayChannel.descriptor().id,
        "cloudflare-ai-gateway"
    );
    assert_eq!(CloudflareAiGatewayChannel.descriptor().supports, expected);
}

#[test]
fn resolves_rest_path_and_exact_override_without_account() {
    let settings = json!({});
    let secret = json!({
        "api_key":"cf-token",
        "account_id":"account/id",
        "gateway_id":"production"
    });
    let mut headers = HeaderMap::new();
    headers.insert("cf-aig-cache-ttl", HeaderValue::from_static("300"));
    headers.insert("cf-aig-gateway-id", HeaderValue::from_static("untrusted"));
    headers.insert("x-api-key", HeaderValue::from_static("untrusted"));
    let default = CloudflareAiGatewayChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: content(Operation::GenerateContent, Kind::OpenAiChat),
            stream: false,
            method: &Method::POST,
            path: "/v1/chat/completions",
            query: None,
            headers: &headers,
            body: &Bytes::from_static(br#"{"model":"client","messages":[]}"#),
            upstream_model: "openai/gpt-5-mini",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        default.request.uri(),
        "https://api.cloudflare.com/client/v4/accounts/account%2Fid/ai/v1/chat/completions"
    );
    assert_eq!(
        default.request.headers()["authorization"],
        "Bearer cf-token"
    );
    assert_eq!(default.request.headers()["cf-aig-gateway-id"], "production");
    assert_eq!(default.request.headers()["cf-aig-cache-ttl"], "300");
    assert!(default.request.headers().get("x-api-key").is_none());

    let exact_settings = json!({
        "base_url":"https://ignored.example",
        "endpoints":{"claude_messages":"https://override.example/v1/messages"}
    });
    let no_account = json!({"api_key":"cf-token"});
    let exact = CloudflareAiGatewayChannel
        .prepare(PrepareCtx {
            session_id: None,
            key: content(Operation::GenerateContent, Kind::ClaudeMessages),
            stream: false,
            method: &Method::POST,
            path: "/v1/messages",
            query: None,
            headers: &HeaderMap::new(),
            body: &Bytes::from_static(br#"{"model":"client","messages":[]}"#),
            upstream_model: "anthropic/claude-sonnet-4-6",
            provider_settings: &exact_settings,
            secret: &no_account,
        })
        .unwrap();
    assert_eq!(exact.request.uri(), "https://override.example/v1/messages");
}

#[test]
fn rewrites_models_and_observes_stream_usage() {
    let settings = json!({
        "endpoints":{"openai_chat_completions":"https://override.example/chat"}
    });
    let secret = json!({"api_key":"cf-token"});
    let headers = HeaderMap::new();
    let key = content(Operation::StreamGenerateContent, Kind::OpenAiChat);
    let prepared = CloudflareAiGatewayChannel
        .prepare(PrepareCtx {
            session_id: None,
            key,
            stream: true,
            method: &Method::POST,
            path: "/v1/chat/completions",
            query: None,
            headers: &headers,
            body: &Bytes::from_static(br#"{"model":"client","messages":[],"stream":true}"#),
            upstream_model: "google/gemini-3-flash",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    let body: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert_eq!(body["model"], "google/gemini-3-flash");
    assert_eq!(body["stream_options"]["include_usage"], true);

    let request_body = prepared.request.body().clone();
    let response_headers = HeaderMap::new();
    let mut decoder = CloudflareAiGatewayChannel
        .stream_decoder(StreamCtx {
            key,
            framing: StreamFraming::Sse,
            request_body: &request_body,
            response_headers: &response_headers,
        })
        .unwrap();
    decoder.push(Bytes::from_static(
        b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":4,\"total_tokens\":13}}\n\n",
    )).unwrap();
    let usage = decoder.finish(StreamEnd::Complete).unwrap().usage.unwrap();
    assert_eq!((usage.input_tokens, usage.output_tokens), (9, 4));
}
