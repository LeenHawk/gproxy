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
use gproxy_channel::channels::chatgpt::{ChatGptChannel, ChatGptCredential, ChatGptSettings};
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

    assert!(
        (200..300).contains(&outcome.response.status),
        "expected 2xx"
    );

    let parsed: serde_json::Value =
        serde_json::from_slice(&outcome.response.body).expect("image response should be JSON");
    let data = parsed["data"].as_array().expect("data array");
    assert!(!data.is_empty(), "at least one image in data");
    let b64 = data[0]["b64_json"].as_str().unwrap_or("");
    assert!(
        b64.len() > 1000,
        "b64_json should be non-trivial (got {} bytes)",
        b64.len()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "hits live chatgpt.com; uploads a tiny image then asks for an edit — very slow (~60s)"]
async fn live_image_edit_with_upload() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

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

    // Minimal 4x4 red PNG (encoded offline with a tiny PNG encoder).
    let png_bytes: Vec<u8> = vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x04, 0x08, 0x02, 0x00, 0x00, 0x00, 0x26,
        0x93, 0x09, 0x29, 0x00, 0x00, 0x00, 0x19, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0xFC,
        0xCF, 0x80, 0x15, 0x30, 0x02, 0x86, 0x03, 0x86, 0x03, 0x86, 0x03, 0x86, 0x03, 0x86, 0x03,
        0x06, 0x00, 0x00, 0xEE, 0x00, 0x03, 0x41, 0xDB, 0x14, 0x29, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    let data_url = format!("data:image/png;base64,{}", STANDARD.encode(&png_bytes));

    let request = PreparedRequest {
        method: http::Method::POST,
        route: RouteKey::new(OperationFamily::CreateImageEdit, ProtocolKind::OpenAi),
        model: Some("gpt-image-1".to_string()),
        body: serde_json::to_vec(&serde_json::json!({
            "image": data_url,
            "prompt": "turn the red square into a blue square with a yellow border",
            "n": 1
        }))
        .unwrap(),
        headers: http::HeaderMap::new(),
    };

    let outcome = execute_once(&channel, &credential, &settings, &http_client, request)
        .await
        .expect("execute_once");
    println!(
        "[edit] status={} bytes={} latency_ms={}",
        outcome.response.status,
        outcome.response.body.len(),
        outcome.response.total_latency_ms
    );
    let body_str = String::from_utf8_lossy(&outcome.response.body);
    println!("[edit] body head={}", &body_str[..body_str.len().min(200)]);

    assert!(
        (200..300).contains(&outcome.response.status),
        "expected 2xx, body={}",
        &body_str[..body_str.len().min(500)]
    );
    let parsed: serde_json::Value =
        serde_json::from_slice(&outcome.response.body).expect("images.response JSON");
    let data = parsed["data"].as_array().expect("data array");
    assert!(!data.is_empty());
    let b64 = data[0]["b64_json"].as_str().unwrap_or("");
    assert!(b64.len() > 1000, "b64_json too short: {}", b64.len());
}
