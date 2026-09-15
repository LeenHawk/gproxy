use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx, StreamCtx, StreamEnd};
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
fn resolves_daily_default_and_exact_override_urls() {
    let secret = json!({"access_token":"access","project_id":"p1"});
    let defaults = json!({});
    let list = AntigravityChannel
        .prepare(PrepareCtx {
            session_id: None,
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
            session_id: None,
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
            session_id: None,
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
            session_id: None,
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
fn preserves_only_claude_generation_output_limits() {
    let secret = json!({"access_token":"access","project_id":"p1"});
    let settings = json!({});
    let headers = HeaderMap::new();
    for operation in [Operation::GenerateContent, Operation::StreamGenerateContent] {
        for model in [
            "claude-opus-4-6-thinking",
            "claude-sonnet-4-6",
            "models/claude-opus-4-6-thinking",
            "gemini-pro-agent",
            "gemini-3.1-pro-high",
        ] {
            for field in ["maxOutputTokens", "max_output_tokens"] {
                for limit in [None, Some(16), Some(128_000)] {
                    let mut input = json!({
                        "contents":[{"role":"user","parts":[{"text":"hi"}]}],
                        "store":true,
                        "generationConfig":{
                            "temperature":0.4,
                            "thinkingConfig":{"thinkingBudget":0,"includeThoughts":false},
                            "logprobs":5,
                            "responseLogprobs":true,
                            "response_logprobs":true
                        }
                    });
                    if let Some(limit) = limit {
                        input["generationConfig"][field] = json!(limit);
                    }
                    let prepared = AntigravityChannel
                        .prepare(PrepareCtx {
                            session_id: None,
                            key: gemini(operation),
                            stream: operation == Operation::StreamGenerateContent,
                            method: &Method::POST,
                            path: "/v1beta/models/client:generateContent",
                            query: None,
                            headers: &headers,
                            body: &Bytes::from(serde_json::to_vec(&input).unwrap()),
                            upstream_model: model,
                            provider_settings: &settings,
                            secret: &secret,
                        })
                        .unwrap();
                    let envelope: Value = serde_json::from_slice(prepared.request.body()).unwrap();
                    let config = &envelope["request"]["generationConfig"];
                    let expected = limit
                        .filter(|_| {
                            crate::shared::gemini::model::model_id(model).starts_with("claude-")
                        })
                        .map(Value::from);
                    assert_eq!(config.get(field), expected.as_ref(), "{model}: {field}");
                    if limit.is_none() {
                        assert!(config.get("maxOutputTokens").is_none());
                        assert!(config.get("max_output_tokens").is_none());
                    }
                    assert_eq!(config["temperature"], 0.4);
                    assert_eq!(
                        config["thinkingConfig"],
                        json!({"thinkingBudget":0,"includeThoughts":false})
                    );
                    for unsupported in ["logprobs", "responseLogprobs", "response_logprobs"] {
                        assert!(config.get(unsupported).is_none());
                    }
                    assert!(envelope["request"].get("store").is_none());
                }
            }
        }
    }
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
            session_id: None,
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
