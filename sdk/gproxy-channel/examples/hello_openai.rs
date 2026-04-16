//! Minimal `gproxy-channel` example: hit OpenAI's `/v1/chat/completions`
//! through [`gproxy_channel::executor::execute_once`] with zero help from
//! `gproxy-engine`.
//!
//! This is the "hello world" proof that L1 (single-channel) is actually
//! usable on its own — you get a strongly typed client that handles the
//! full finalize / sanitize / rewrite / prepare_request / send /
//! normalize / classify pipeline, without pulling in the multi-channel
//! engine, store, retry, or affinity code.
//!
//! Run with:
//!
//! ```text
//! OPENAI_API_KEY=sk-... cargo run \
//!     -p gproxy-channel \
//!     --example hello_openai \
//!     --no-default-features --features openai
//! ```
//!
//! The `--no-default-features --features openai` flags are what make this
//! a meaningful smoke test — they prove the single-channel use case
//! doesn't accidentally require any of the other 13 channels to compile.

use gproxy_channel::channels::openai::{OpenAiChannel, OpenAiCredential, OpenAiSettings};
use gproxy_channel::executor::execute_once;
use gproxy_channel::request::PreparedRequest;
use gproxy_channel::response::ResponseClassification;
use gproxy_channel::routing::RouteKey;
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};
use http::HeaderMap;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| {
        "OPENAI_API_KEY environment variable not set. \
         Set it to a valid OpenAI API key to run this example."
    })?;

    let channel = OpenAiChannel;
    let settings = OpenAiSettings::default();
    let credential = OpenAiCredential { api_key };
    let http_client = wreq::Client::new();

    // Minimal chat-completions request body.
    let body = br#"{
        "model": "gpt-4o-mini",
        "messages": [
            {"role": "system", "content": "You are a terse assistant."},
            {"role": "user", "content": "Say hello in one word."}
        ],
        "max_tokens": 16
    }"#
    .to_vec();

    let request = PreparedRequest {
        method: http::Method::POST,
        route: RouteKey::new(
            OperationFamily::GenerateContent,
            ProtocolKind::OpenAiChatCompletion,
        ),
        model: Some("gpt-4o-mini".to_string()),
        body,
        headers: HeaderMap::new(),
    };

    let outcome = execute_once(&channel, &credential, &settings, &http_client, request).await?;

    println!("status = {}", outcome.response.status);
    println!("classification = {:?}", outcome.classification);
    println!(
        "body (first 500 bytes) = {}",
        String::from_utf8_lossy(&outcome.response.body[..outcome.response.body.len().min(500)],)
    );

    match outcome.classification {
        ResponseClassification::Success => {
            println!("✓ request succeeded");
            Ok(())
        }
        other => Err(format!("upstream returned non-success classification: {other:?}").into()),
    }
}
