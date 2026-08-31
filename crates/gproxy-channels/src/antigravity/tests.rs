use bytes::Bytes;
use gproxy_channel_api::{Channel, ChannelSupport, PrepareCtx, StreamCtx, StreamEnd};
use gproxy_protocol::{
    ContentGenerationKind as Kind, Operation, OperationKey, StreamFraming, WireFamily,
};
use http::{HeaderMap, HeaderValue, Method};
use serde_json::{Value, json};

use super::AntigravityChannel;

const fn family(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::Gemini)
}

const fn gemini(operation: Operation) -> OperationKey {
    OperationKey::content(operation, Kind::GeminiGenerateContent)
}

#[test]
fn declares_truthful_operations() {
    let expected = [
        ChannelSupport::passthrough(family(Operation::ListModels)),
        ChannelSupport::passthrough(family(Operation::CountTokens)),
        ChannelSupport::passthrough(gemini(Operation::GenerateContent)),
        ChannelSupport::passthrough(gemini(Operation::StreamGenerateContent)),
        ChannelSupport::transform(
            OperationKey::content(Operation::GenerateContent, Kind::OpenAiChat),
            gemini(Operation::GenerateContent),
        ),
        ChannelSupport::transform(
            OperationKey::content(Operation::GenerateContent, Kind::OpenAiResponses),
            gemini(Operation::GenerateContent),
        ),
        ChannelSupport::transform(
            OperationKey::content(Operation::GenerateContent, Kind::ClaudeMessages),
            gemini(Operation::GenerateContent),
        ),
        ChannelSupport::transform(
            OperationKey::content(Operation::StreamGenerateContent, Kind::OpenAiChat),
            gemini(Operation::StreamGenerateContent),
        ),
        ChannelSupport::transform(
            OperationKey::content(Operation::StreamGenerateContent, Kind::OpenAiResponses),
            gemini(Operation::StreamGenerateContent),
        ),
        ChannelSupport::transform(
            OperationKey::content(Operation::StreamGenerateContent, Kind::ClaudeMessages),
            gemini(Operation::StreamGenerateContent),
        ),
    ];
    assert_eq!(AntigravityChannel.descriptor().supports, expected);
}

#[test]
fn resolves_daily_default_and_exact_override_urls() {
    let secret = json!({"access_token":"access","project_id":"p1"});
    let defaults = json!({});
    let list = AntigravityChannel
        .prepare(PrepareCtx {
            key: family(Operation::ListModels),
            stream: false,
            method: &Method::GET,
            path: "/v1beta/models",
            query: None,
            headers: &HeaderMap::new(),
            body: &Bytes::new(),
            upstream_model: "",
            provider_settings: &defaults,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        list.request.uri(),
        "https://daily-cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels"
    );

    let settings = json!({
        "base_url":"https://ignored.example",
        "endpoints":{"gemini_stream_generate_content":"https://relay.example/stream"}
    });
    let mut stream_headers = HeaderMap::new();
    stream_headers.insert("accept", HeaderValue::from_static("text/event-stream"));
    let stream = AntigravityChannel
        .prepare(PrepareCtx {
            key: gemini(Operation::StreamGenerateContent),
            stream: true,
            method: &Method::POST,
            path: "/v1beta/models/client:streamGenerateContent",
            query: None,
            headers: &stream_headers,
            body: &Bytes::from_static(br#"{"contents":[]}"#),
            upstream_model: "gemini-3-pro",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(stream.request.uri(), "https://relay.example/stream?alt=sse");
    assert_eq!(stream.request.headers()["authorization"], "Bearer access");
    assert_eq!(
        stream.request.headers()["user-agent"],
        "antigravity/cli/1.0.6 linux/amd64"
    );
    assert_eq!(stream.request.headers()["accept"], "text/event-stream");
    assert_eq!(
        stream.profile.unwrap().preserve_tls13_cipher_list,
        Some(true)
    );
}

#[test]
fn removes_only_root_store_and_unwraps_stream_frames() {
    let secret = json!({"access_token":"access","project_id":"p1"});
    let settings = json!({});
    let body = Bytes::from_static(
        br#"{"contents":[{"parts":[{"text":"hi","store":"nested"}]}],"store":true,"generationConfig":{"maxOutputTokens":8,"temperature":0.4}}"#,
    );
    let key = gemini(Operation::StreamGenerateContent);
    let prepared = AntigravityChannel
        .prepare(PrepareCtx {
            key,
            stream: true,
            method: &Method::POST,
            path: "/v1beta/models/client:streamGenerateContent",
            query: None,
            headers: &HeaderMap::new(),
            body: &body,
            upstream_model: "gemini-3-pro",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    let envelope: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert!(envelope["request"].get("store").is_none());
    assert_eq!(
        envelope["request"]["contents"][0]["parts"][0]["store"],
        "nested"
    );
    assert_eq!(envelope["request"]["generationConfig"]["temperature"], 0.4);
    assert!(
        envelope["request"]["generationConfig"]
            .get("maxOutputTokens")
            .is_none()
    );

    let count = AntigravityChannel
        .prepare(PrepareCtx {
            key: family(Operation::CountTokens),
            stream: false,
            method: &Method::POST,
            path: "/v1beta/models/client:countTokens",
            query: None,
            headers: &HeaderMap::new(),
            body: &Bytes::from_static(
                br#"{"generateContentRequest":{"contents":[],"store":true,"generationConfig":{"maxOutputTokens":8,"temperature":0.2}}}"#,
            ),
            upstream_model: "gemini-3-pro",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    let count: Value = serde_json::from_slice(count.request.body()).unwrap();
    assert!(count["request"].get("store").is_none());
    assert!(
        count["request"]["generationConfig"]
            .get("maxOutputTokens")
            .is_none()
    );
    assert_eq!(count["request"]["generationConfig"]["temperature"], 0.2);

    let request_body = prepared.request.body().clone();
    let response_headers = HeaderMap::new();
    let mut decoder = AntigravityChannel
        .stream_decoder(StreamCtx {
            key,
            framing: StreamFraming::Sse,
            request_body: &request_body,
            response_headers: &response_headers,
        })
        .unwrap();
    let frames = decoder
        .push(Bytes::from_static(
            b"data: {\"response\":{\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":2}}}\n\n",
        ))
        .unwrap();
    assert_eq!(frames.len(), 1);
    assert!(
        !std::str::from_utf8(&frames[0].0)
            .unwrap()
            .contains("\"response\"")
    );
    let usage = decoder.finish(StreamEnd::Complete).unwrap().usage.unwrap();
    assert_eq!((usage.input_tokens, usage.output_tokens), (4, 2));
}
