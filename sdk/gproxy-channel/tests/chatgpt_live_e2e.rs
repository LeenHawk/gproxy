//! Live end-to-end test for the ChatGPT web channel.
//!
//! This test is ignored by default; run manually with:
//!   cargo test -p gproxy-channel --features chatgpt \
//!       --test chatgpt_live_e2e -- --ignored --nocapture
//!
//! Requires a file at `target/.chatgpt_token` containing a valid
//! chatgpt.com `accessToken` (fetched from `/api/auth/session`).
//!
//! The test runs the full pipeline:
//!   refresh_credential (sentinel dance)
//!     → execute_once (prepare_request + POST + normalize_response)
//!     → parse the resulting chat.completion JSON.

#![cfg(feature = "chatgpt")]

use std::path::PathBuf;

use gproxy_channel::channel::Channel;
use gproxy_channel::channels::chatgpt::{
    ChatGptChannel, ChatGptCredential, ChatGptSettings,
};
use gproxy_channel::executor::execute_once;
use gproxy_channel::request::PreparedRequest;
use gproxy_channel::routing::RouteKey;
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

fn token_path() -> PathBuf {
    let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    here.join("../../target/.chatgpt_token")
}

fn load_token() -> Option<String> {
    let path = token_path();
    let raw = std::fs::read_to_string(&path).ok()?;
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "hits live chatgpt.com; requires target/.chatgpt_token"]
async fn live_chat_completion_roundtrip() {
    let access_token = match load_token() {
        Some(t) => t,
        None => {
            panic!(
                "target/.chatgpt_token not found at {:?}; populate it with \
                 a chatgpt.com accessToken",
                token_path()
            );
        }
    };
    tracing_subscriber::fmt()
        .with_env_filter("gproxy_channel=debug,chatgpt=debug")
        .with_test_writer()
        .try_init()
        .ok();

    let channel = ChatGptChannel;
    let settings = ChatGptSettings::default();
    let mut credential = ChatGptCredential {
        access_token,
        ..Default::default()
    };

    let http_client = wreq::Client::builder()
        .emulation(wreq_util::Emulation::Chrome136)
        .cookie_store(true)
        .redirect(wreq::redirect::Policy::limited(10))
        .build()
        .expect("build http client");

    // 1. Sentinel dance → populate sentinel tokens.
    let refreshed = channel
        .refresh_credential(&http_client, &mut credential)
        .await
        .expect("refresh_credential");
    assert!(refreshed, "refresh_credential should return true");
    assert!(
        !credential.chat_req_token.is_empty(),
        "chat_req_token should be populated after refresh"
    );
    println!(
        "[sentinel] persona={:?} chat_req_token_len={} proof_token_len={}",
        credential.persona,
        credential.chat_req_token.len(),
        credential.proof_token.len()
    );

    // 2. Build a minimal chat completion request.
    let request = PreparedRequest {
        method: http::Method::POST,
        route: RouteKey::new(
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ),
        model: Some("gpt-5".to_string()),
        body: serde_json::to_vec(&serde_json::json!({
            "model": "gpt-5",
            "messages": [{"role": "user", "content": "reply with the single word hi"}]
        }))
        .unwrap(),
        headers: http::HeaderMap::new(),
    };

    // 3. Send the request.
    let outcome = execute_once(&channel, &credential, &settings, &http_client, request)
        .await
        .expect("execute_once");
    println!(
        "[http] status={} bytes={} latency_ms={}",
        outcome.response.status,
        outcome.response.body.len(),
        outcome.response.total_latency_ms
    );
    let body_str = String::from_utf8_lossy(&outcome.response.body);
    println!("[body] {}", body_str);

    assert!(
        (200..300).contains(&outcome.response.status),
        "expected 2xx, got {} body={}",
        outcome.response.status,
        body_str
    );

    // 4. Body should parse as a chat.completion JSON with non-empty content.
    let parsed: serde_json::Value = serde_json::from_slice(&outcome.response.body)
        .expect("response body should be JSON chat.completion");
    assert_eq!(parsed["object"], "chat.completion");
    let content = parsed["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("");
    assert!(!content.is_empty(), "assistant content should be non-empty");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "hits live chatgpt.com; requires target/.chatgpt_token"]
async fn live_streaming_chat_completion() {
    use futures_util::StreamExt;
    use gproxy_channel::executor::{SendAttemptStreamOutcome, execute_once_stream};

    let access_token = load_token().expect("need target/.chatgpt_token");
    tracing_subscriber::fmt()
        .with_env_filter("gproxy_channel=debug")
        .with_test_writer()
        .try_init()
        .ok();

    let channel = ChatGptChannel;
    let settings = ChatGptSettings::default();
    let mut credential = ChatGptCredential {
        access_token,
        ..Default::default()
    };
    let http_client = wreq::Client::builder()
        .emulation(wreq_util::Emulation::Chrome136)
        .cookie_store(true)
        .redirect(wreq::redirect::Policy::limited(10))
        .build()
        .expect("client");

    channel
        .refresh_credential(&http_client, &mut credential)
        .await
        .expect("refresh");

    let request = PreparedRequest {
        method: http::Method::POST,
        route: RouteKey::new(
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ),
        model: Some("gpt-5".to_string()),
        body: serde_json::to_vec(&serde_json::json!({
            "model": "gpt-5",
            "stream": true,
            "messages": [{"role": "user", "content": "count from 1 to 5, one number per word"}]
        }))
        .unwrap(),
        headers: http::HeaderMap::new(),
    };

    let outcome = execute_once_stream(
        &channel,
        &credential,
        &settings,
        &http_client,
        request,
    )
    .await
    .expect("execute_once_stream");

    let mut stream = match outcome {
        SendAttemptStreamOutcome::Streaming(s) => s.body,
        SendAttemptStreamOutcome::Buffered(b) => panic!(
            "expected streaming, got buffered status={} body={}",
            b.response.status,
            String::from_utf8_lossy(&b.response.body)
        ),
    };

    let mut concatenated = Vec::<u8>::new();
    let mut chunk_count = 0usize;
    while let Some(item) = stream.next().await {
        let bytes = item.expect("chunk ok");
        concatenated.extend_from_slice(&bytes);
        chunk_count += 1;
    }

    let text = String::from_utf8_lossy(&concatenated);
    println!(
        "[stream] chunks={} total_bytes={}",
        chunk_count,
        concatenated.len()
    );
    println!(
        "[stream] tail={}",
        &text[text.len().saturating_sub(400)..]
    );

    // Note: at the raw send_attempt_stream level the channel reshaper
    // is NOT applied (that happens in the higher-level engine stream
    // pipeline). What we get here is raw ChatGPT SSE. Validate the
    // `resume_conversation_token` frame is present as proof we reached
    // the streaming endpoint.
    assert!(
        text.contains("resume_conversation_token") || text.contains("delta_encoding"),
        "expected SSE banner / resume token in stream tail: {}",
        &text[..text.len().min(200)]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "hits live chatgpt.com; requires target/.chatgpt_token"]
async fn live_streaming_via_reshaper() {
    use futures_util::StreamExt;
    use gproxy_channel::channel::Channel;
    use gproxy_channel::executor::{SendAttemptStreamOutcome, execute_once_stream};

    let access_token = load_token().expect("need target/.chatgpt_token");
    tracing_subscriber::fmt()
        .with_env_filter("gproxy_channel=debug")
        .with_test_writer()
        .try_init()
        .ok();

    let channel = ChatGptChannel;
    let settings = ChatGptSettings::default();
    let mut credential = ChatGptCredential {
        access_token,
        ..Default::default()
    };
    let http_client = wreq::Client::builder()
        .emulation(wreq_util::Emulation::Chrome136)
        .cookie_store(true)
        .redirect(wreq::redirect::Policy::limited(10))
        .build()
        .expect("client");

    channel
        .refresh_credential(&http_client, &mut credential)
        .await
        .expect("refresh");

    let request = PreparedRequest {
        method: http::Method::POST,
        route: RouteKey::new(
            OperationFamily::StreamGenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ),
        model: Some("gpt-5".to_string()),
        body: serde_json::to_vec(&serde_json::json!({
            "model": "gpt-5",
            "stream": true,
            "messages": [{"role": "user", "content": "say hi in three words"}]
        }))
        .unwrap(),
        headers: http::HeaderMap::new(),
    };

    // Build the channel's reshaper explicitly so this test mirrors what
    // the engine's stream wrapper does without requiring the full engine.
    let mut reshaper = channel
        .stream_reshaper(&request)
        .expect("chatgpt should return reshaper for StreamGenerateContent");

    let outcome = execute_once_stream(&channel, &credential, &settings, &http_client, request)
        .await
        .expect("execute_once_stream");
    let mut stream = match outcome {
        SendAttemptStreamOutcome::Streaming(s) => s.body,
        SendAttemptStreamOutcome::Buffered(b) => panic!(
            "expected streaming, got status={} body={}",
            b.response.status,
            String::from_utf8_lossy(&b.response.body)
        ),
    };

    let mut reshaped = Vec::<u8>::new();
    while let Some(item) = stream.next().await {
        let raw = item.expect("chunk ok");
        reshaped.extend_from_slice(&reshaper.push_chunk(&raw));
    }
    reshaped.extend_from_slice(&reshaper.finish());

    let text = String::from_utf8_lossy(&reshaped);
    println!("[reshaped] total_bytes={}", reshaped.len());
    println!(
        "[reshaped] head={}",
        &text[..text.len().min(400)]
    );

    // The reshaped output must be OpenAI chat.completion.chunk SSE with a
    // trailing `[DONE]` marker.
    assert!(text.contains("\"object\":\"chat.completion.chunk\""));
    assert!(text.ends_with("data: [DONE]\n\n"));

    // Reassemble content from delta.content fields.
    let mut content = String::new();
    for line in text.lines() {
        let Some(payload) = line.strip_prefix("data: ") else {
            continue;
        };
        if payload == "[DONE]" {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(payload)
            && let Some(s) = v["choices"][0]["delta"]["content"].as_str()
        {
            content.push_str(s);
        }
    }
    assert!(!content.is_empty(), "reassembled content is empty");
    println!("[reshaped] content={content:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "hits live chatgpt.com; generates one image — slow (~30s)"]
async fn live_image_generation() {
    let access_token = load_token().expect("need target/.chatgpt_token");
    tracing_subscriber::fmt()
        .with_env_filter("gproxy_channel=debug")
        .with_test_writer()
        .try_init()
        .ok();

    let channel = ChatGptChannel;
    let settings = ChatGptSettings::default();
    let mut credential = ChatGptCredential {
        access_token,
        ..Default::default()
    };

    let http_client = wreq::Client::builder()
        .emulation(wreq_util::Emulation::Chrome136)
        .cookie_store(true)
        .redirect(wreq::redirect::Policy::limited(10))
        .build()
        .expect("build http client");

    channel
        .refresh_credential(&http_client, &mut credential)
        .await
        .expect("refresh");

    let request = PreparedRequest {
        method: http::Method::POST,
        route: RouteKey::new(OperationFamily::CreateImage, ProtocolKind::OpenAi),
        model: Some("gpt-image-1".to_string()),
        body: serde_json::to_vec(&serde_json::json!({
            "prompt": "a tiny cartoon cat wearing a red hat, simple line art",
            "n": 1,
            "size": "1024x1024"
        }))
        .unwrap(),
        headers: http::HeaderMap::new(),
    };

    let outcome = execute_once(&channel, &credential, &settings, &http_client, request)
        .await
        .expect("execute_once");
    println!(
        "[image] status={} bytes={} latency_ms={}",
        outcome.response.status,
        outcome.response.body.len(),
        outcome.response.total_latency_ms
    );
    let body_str = String::from_utf8_lossy(&outcome.response.body);
    println!(
        "[image] full_body (first 4KB)={}",
        &body_str[..body_str.len().min(4000)]
    );
    println!(
        "[image] last 1KB={}",
        &body_str[body_str.len().saturating_sub(1000)..]
    );

    assert!((200..300).contains(&outcome.response.status), "expected 2xx");

    let parsed: serde_json::Value = serde_json::from_slice(&outcome.response.body)
        .expect("image response should be JSON");
    let data = parsed["data"].as_array().expect("data array");
    assert!(!data.is_empty(), "at least one image in data");
    let b64 = data[0]["b64_json"].as_str().unwrap_or("");
    assert!(
        b64.len() > 1000,
        "b64_json should be non-trivial (got {} bytes)",
        b64.len()
    );
}
