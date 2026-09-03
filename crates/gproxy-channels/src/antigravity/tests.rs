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
        br#"{"contents":[{"parts":[{"text":"hi","store":"nested"}]}],"store":true,"generationConfig":{"maxOutputTokens":8,"temperature":0.4},"tools":[{"functionDeclarations":[{"name":"lookup","description":"lookup"}]},{"googleSearch":{},"urlContext":{}}]}"#,
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
            upstream_model: "gemini-3.1-pro-high",
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
    assert_eq!(
        envelope["request"]["generationConfig"]["thinkingConfig"]["thinkingBudget"],
        10_001
    );
    let explicit = super::prepare::apply_model_defaults(
        &Bytes::from_static(
            br#"{"generationConfig":{"thinkingConfig":{"includeThoughts":true,"thinkingBudget":4096}}}"#,
        ),
        "gemini-3.1-pro-high",
    )
    .unwrap();
    let explicit: Value = serde_json::from_slice(&explicit).unwrap();
    assert_eq!(
        explicit["generationConfig"]["thinkingConfig"]["thinkingBudget"],
        4096
    );
    assert_eq!(envelope["request"]["tools"].as_array().unwrap().len(), 1);
    assert!(
        envelope["request"]["tools"][0]
            .get("functionDeclarations")
            .is_some()
    );
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
    let partial = decoder
        .push(Bytes::from_static(
            b"data: {\"response\":{\"candidates\":[{\"content\":{\"role\":\"model\",\"parts\":[{\"thought\":true,\"text\":\"checking\"}]} }],\"usageMetadata\":{\"promptTokenCount\":4,\"totalTokenCount\":4}}}\n\n",
        ))
        .unwrap();
    assert_eq!(partial.len(), 1);
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

#[test]
fn claude_code_uses_buffered_antigravity_25_flash() {
    let secret = json!({"access_token":"access","project_id":"p1"});
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::USER_AGENT,
        HeaderValue::from_static("claude-cli/2.1.258 (external, sdk-cli)"),
    );
    let key = gemini(Operation::StreamGenerateContent);
    let prepared = AntigravityChannel
        .prepare(PrepareCtx {
            key,
            stream: true,
            method: &Method::POST,
            path: "/v1beta/models/gemini-2.5-flash:streamGenerateContent",
            query: None,
            headers: &headers,
            body: &Bytes::from_static(br#"{"contents":[{"role":"user","parts":[{"text":"hi"}]}]}"#),
            upstream_model: "gemini-2.5-flash",
            provider_settings: &json!({}),
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        prepared.request.uri(),
        "https://daily-cloudcode-pa.googleapis.com/v1internal:generateContent"
    );
    assert_eq!(prepared.framing, Some(StreamFraming::JsonArray));

    let request_body = prepared.request.body().clone();
    let response_headers = HeaderMap::new();
    let mut decoder = AntigravityChannel
        .stream_decoder(StreamCtx {
            key,
            framing: StreamFraming::JsonArray,
            request_body: &request_body,
            response_headers: &response_headers,
        })
        .unwrap();
    assert!(
        decoder
            .push(Bytes::from_static(br#"{"response":{"candidates":[{"content":{"role":"model","parts":[{"text":"ok"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":4,"candidatesTokenCount":2,"totalTokenCount":6}}}"#))
            .unwrap()
            .is_empty()
    );
    let tail = decoder.finish(StreamEnd::Complete).unwrap();
    assert_eq!(tail.frames.len(), 1);
    assert_eq!(
        (tail.usage.unwrap().input_tokens, tail.frames[0].0[0]),
        (4, b'[')
    );
}
