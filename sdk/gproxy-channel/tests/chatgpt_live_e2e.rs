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
