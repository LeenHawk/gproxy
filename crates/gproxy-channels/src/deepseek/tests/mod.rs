//! DeepSeek operations, endpoint, and shaping budget tests.

mod support;

use bytes::Bytes;
use gproxy_channel_api::{Channel, ResponseShapeCtx, StreamCtx, StreamEnd, UsageCtx};
use gproxy_protocol::{ContentGenerationKind as C, Operation as O, OperationKey, StreamFraming};
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

use super::DeepSeekChannel;
use support::{content, family, prepare};

#[test]
fn declares_exactly_the_available_native_and_transformed_routes() {
    let expected = [
        family(O::ListModels),
        family(O::GetModel),
        content(O::GenerateContent, C::OpenAiChat),
        content(O::StreamGenerateContent, C::OpenAiChat),
        content(O::GenerateContent, C::OpenAiResponses),
        content(O::StreamGenerateContent, C::OpenAiResponses),
        content(O::GenerateContent, C::ClaudeMessages),
        content(O::StreamGenerateContent, C::ClaudeMessages),
    ];
    let supports = DeepSeekChannel.descriptor().supports;
    assert_eq!(supports.len(), 12);
    assert!(expected.iter().all(|key| {
        supports
            .iter()
            .any(|row| row.source == *key && row.target == *key)
    }));
    for (source, target) in [
        (
            OperationKey::family(O::ListModels, gproxy_protocol::WireFamily::Claude),
            family(O::ListModels),
        ),
        (
            content(O::GenerateContent, C::GeminiGenerateContent),
            content(O::GenerateContent, C::OpenAiChat),
        ),
    ] {
        assert!(
            supports
                .iter()
                .any(|row| row.source == source && row.target == target)
        );
    }
}

#[test]
fn resolves_default_paths_and_uses_target_key_auth_on_override() {
    let headers = HeaderMap::new();
    let chat_body = Bytes::from_static(br#"{"model":"route","messages":[]}"#);
    let chat = prepare(
        content(O::GenerateContent, C::OpenAiChat),
        "deepseek-v4-flash",
        &headers,
        &chat_body,
        &json!({}),
    );
    assert_eq!(
        chat.request.uri(),
        "https://api.deepseek.com/v1/chat/completions"
    );
    assert_eq!(chat.request.method(), Method::POST);
    assert_eq!(
        chat.request.headers()["authorization"],
        "Bearer deepseek-key"
    );
    let chat_body: Value = serde_json::from_slice(chat.request.body()).unwrap();
    assert_eq!(chat_body["model"], "deepseek-v4-flash");

    let responses_body = Bytes::from_static(br#"{"model":"route","input":"hi"}"#);
    let responses = prepare(
        content(O::GenerateContent, C::OpenAiResponses),
        "deepseek-v4-flash",
        &headers,
        &responses_body,
        &json!({}),
    );
    assert_eq!(
        responses.request.uri(),
        "https://api.deepseek.com/responses"
    );

    let claude_body = Bytes::from_static(br#"{"model":"route","messages":[],"max_tokens":8}"#);
    let claude = prepare(
        content(O::GenerateContent, C::ClaudeMessages),
        "claude/model",
        &headers,
        &claude_body,
        &json!({
            "base_url":"https://unused.example",
            "endpoints":{"claude_messages":"https://relay.example/responses/{model}"}
        }),
    );
    assert_eq!(
        claude.request.uri(),
        "https://relay.example/responses/claude%2Fmodel"
    );
    assert_eq!(claude.request.headers()["x-api-key"], "deepseek-key");
    assert!(claude.request.headers().get("authorization").is_none());
}

#[test]
fn shapes_chat_and_observes_raw_cache_usage_in_buffered_and_sse() {
    let key = content(O::GenerateContent, C::OpenAiChat);
    let request = Bytes::from(
        json!({
            "model":"route", "messages":[
                {"role":"developer","content":"rule"},
                {"role":"user","content":"hi"}
            ],
            "max_completion_tokens":500_000,
            "parallel_tool_calls":true, "store":true,
            "extra_body":{"extra_body":{"thinking":{"type":"adaptive"}}},
            "tools":[{"type":"retrieval","retrieval":{}}],
            "tool_choice":"auto", "future":{"kept":true}
        })
        .to_string(),
    );
    let prepared = prepare(
        key,
        "deepseek-v4-pro",
        &HeaderMap::new(),
        &request,
        &json!({}),
    );
    let shaped: Value = serde_json::from_slice(prepared.request.body()).unwrap();
    assert_eq!(shaped["model"], "deepseek-v4-pro");
    assert_eq!(shaped["max_tokens"], 384_000);
    assert_eq!(shaped["messages"][0]["role"], "system");
    assert_eq!(shaped["thinking"]["type"], "enabled");
    assert_eq!(shaped["tool_choice"], "none");
    assert!(shaped.get("tools").is_none() && shaped.get("store").is_none());
    assert_eq!(shaped["future"]["kept"], true);

    let raw = Bytes::from_static(br#"{"choices":[{"finish_reason":"insufficient_system_resource"}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15,"prompt_cache_hit_tokens":3}}"#);
    let headers = HeaderMap::new();
    let usage = DeepSeekChannel
        .extract_usage(UsageCtx {
            key,
            request_body: &request,
            response_headers: &headers,
            response_body: &raw,
        })
        .unwrap();
    assert_eq!(
        (
            usage.input_tokens,
            usage.output_tokens,
            usage.cached_input_tokens
        ),
        (10, 5, 3)
    );
    let outward = DeepSeekChannel
        .shape_response(ResponseShapeCtx {
            key,
            status: StatusCode::OK,
            headers: &headers,
            body: &raw,
        })
        .unwrap();
    let outward: Value = serde_json::from_slice(&outward).unwrap();
    assert_eq!(outward["choices"][0]["finish_reason"], "length");
    assert_eq!(
        outward["usage"]["prompt_tokens_details"]["cached_tokens"],
        3
    );

    let stream_key = content(O::StreamGenerateContent, C::OpenAiChat);
    let mut decoder = DeepSeekChannel
        .stream_decoder(StreamCtx {
            key: stream_key,
            framing: StreamFraming::Sse,
            request_body: &request,
            response_headers: &headers,
        })
        .unwrap();
    let sse = Bytes::from(format!(
        "data: {}\n\ndata: [DONE]\n\n",
        String::from_utf8_lossy(&raw)
    ));
    assert!(decoder.push(sse.slice(..20)).unwrap().is_empty());
    let frames = decoder.push(sse.slice(20..)).unwrap();
    let mut relayed = Vec::new();
    for frame in frames {
        relayed.extend_from_slice(&frame.0);
    }
    let relayed = String::from_utf8(relayed).unwrap();
    assert!(relayed.contains("\"finish_reason\":\"length\""));
    assert!(!relayed.contains("insufficient_system_resource"));
    assert!(relayed.ends_with("data: [DONE]\n\n"));
    let tail = decoder.finish(StreamEnd::Complete).unwrap();
    assert!(tail.frames.is_empty());
    assert_eq!(tail.usage.unwrap().cached_input_tokens, 3);
}
