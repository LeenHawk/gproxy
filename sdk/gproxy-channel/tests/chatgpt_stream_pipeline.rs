//! Integration test that mirrors the engine's stream pipeline for the
//! chatgpt channel without spinning up the full engine.
//!
//! Runs real HAR bytes through the protocol-layer stream transformer
//! with the channel's `normalize_response` bound as the per-chunk
//! normalizer — exactly how the engine wires it in
//! [`gproxy_engine::engine::wrap_upstream_response_stream`].

#![cfg(feature = "chatgpt")]

use std::sync::Arc;

use gproxy_channel::channel::Channel;
use gproxy_channel::channels::chatgpt::ChatGptChannel;
use gproxy_channel::request::PreparedRequest;
use gproxy_channel::routing::RouteKey;
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};
use gproxy_protocol::transform::dispatch::{
    create_stream_response_transformer,
};

#[test]
fn stream_pipeline_reshapes_real_har_to_openai_chunks() {
    let sse_bytes: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../target/samples/05_sse_response_text.txt"
    ));

    let channel = Arc::new(ChatGptChannel);
    let mut prepared = PreparedRequest {
        method: http::Method::POST,
        route: RouteKey::new(
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ),
        model: Some("gpt-5".to_string()),
        body: Vec::new(),
        headers: http::HeaderMap::new(),
    };
    let turn_id = uuid::Uuid::new_v4().to_string();
    prepared.headers.insert(
        http::HeaderName::from_static("x-chatgpt-turn-id"),
        http::HeaderValue::from_str(&turn_id).unwrap(),
    );

    // Bind channel.normalize_response as the per-JSON-chunk normalizer,
    // mirroring what `GproxyEngine::execute_stream_inner` does.
    let channel_clone = channel.clone();
    let prepared_clone = prepared.clone();
    let normalizer: gproxy_protocol::transform::dispatch::StreamChunkNormalizer =
        Arc::new(move |body: Vec<u8>| channel_clone.normalize_response(&prepared_clone, body));

    let mut transformer = create_stream_response_transformer(
        OperationFamily::StreamGenerateContent,
        ProtocolKind::OpenAiChatCompletion,
        OperationFamily::StreamGenerateContent,
        ProtocolKind::OpenAiChatCompletion,
        Some(normalizer),
    )
    .expect("build transformer");

    // Feed the real HAR body in arbitrary chunks to simulate streaming.
    let mut out = Vec::new();
    for piece in sse_bytes.chunks(512) {
        let bytes = transformer.push_chunk(piece).expect("push_chunk");
        out.extend_from_slice(&bytes);
    }
    let tail = transformer.finish().expect("finish");
    out.extend_from_slice(&tail);

    let text = String::from_utf8_lossy(&out);

    // Output MUST be valid OpenAI chat.completion.chunk SSE.
    assert!(
        text.contains("\"object\":\"chat.completion.chunk\""),
        "no chat.completion.chunk seen; head={}",
        &text[..text.len().min(400)]
    );
    assert!(
        text.ends_with("data: [DONE]\n\n"),
        "tail: {}",
        &text[text.len().saturating_sub(200)..]
    );

    // Reassembling content from delta.content fields should reproduce
    // the original Chinese bubble-sort reply.
    let mut reassembled = String::new();
    for line in text.lines() {
        let Some(payload) = line.strip_prefix("data: ") else {
            continue;
        };
        if payload == "[DONE]" {
            continue;
        }
        let Ok(v): Result<serde_json::Value, _> = serde_json::from_str(payload) else {
            continue;
        };
        if let Some(s) = v["choices"][0]["delta"]["content"].as_str() {
            reassembled.push_str(s);
        }
    }
    assert!(
        reassembled.contains("冒泡"),
        "expected bubble-sort Chinese reply, got: {}",
        reassembled.chars().take(80).collect::<String>()
    );

    // The final chunk should carry finish_reason=stop. Scan chunks and
    // check at least one has it.
    let mut saw_stop = false;
    for line in text.lines() {
        let Some(payload) = line.strip_prefix("data: ") else {
            continue;
        };
        if payload == "[DONE]" {
            continue;
        }
        let Ok(v): Result<serde_json::Value, _> = serde_json::from_str(payload) else {
            continue;
        };
        if v["choices"][0]["finish_reason"].as_str() == Some("stop") {
            saw_stop = true;
            break;
        }
    }
    assert!(saw_stop, "no chunk carried finish_reason=stop");
}
