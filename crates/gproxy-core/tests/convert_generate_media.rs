#![cfg(not(target_arch = "wasm32"))]

//! Media references in a converted generation request. A reference the
//! upstream dialect reads itself is passed through untouched; any other is
//! fetched in the caller's scope and sent inline, and a failed fetch fails the
//! request instead of silently dropping the media.

mod support;
use support::*;

use gproxy_core::{AllowAllFetchPolicy, Core, FetchPolicy};
use gproxy_protocol::{Dialect, Operation, OperationKey};
use http::StatusCode;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A 1x1 PNG.
const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";

/// The same seeded engine under another fetch policy.
fn with_policy(h: Harness, policy: Arc<dyn FetchPolicy>) -> Harness {
    let Harness {
        core,
        observer,
        client,
        channel,
    } = h;
    let parts = core.into_parts();
    let core = Core::builder(parts.store)
        .cache(parts.cache)
        .observer(parts.observer)
        .secret_codec(parts.codec)
        .channel(channel.clone())
        .unwrap()
        .fetch_policy(policy)
        .snapshot(parts.data)
        .build()
        .unwrap();
    Harness {
        core,
        observer,
        client,
        channel,
    }
}

/// Serves the PNG on 127.0.0.1 to every connection and counts requests.
async fn image_server() -> (String, Arc<Mutex<usize>>) {
    use base64::Engine;
    let body = base64::engine::general_purpose::STANDARD
        .decode(PNG)
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/image.png", listener.local_addr().unwrap());
    let hits = Arc::new(Mutex::new(0));
    let seen = hits.clone();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let mut head = Vec::new();
            let mut buf = [0u8; 1024];
            while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                match socket.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => head.extend_from_slice(&buf[..n]),
                }
            }
            *seen.lock().unwrap() += 1;
            let head = format!(
                "HTTP/1.1 200 OK\r\nconnection: close\r\ncontent-type: image/png\r\ncontent-length: {}\r\n\r\n",
                body.len()
            );
            let _ = socket.write_all(head.as_bytes()).await;
            let _ = socket.write_all(&body).await;
            let _ = socket.shutdown().await;
        }
    });
    (url, hits)
}

/// The JSON body the scripted client recorded for the `index`th native call.
fn sent_body(h: &Harness, index: usize) -> Value {
    let line = h.client.seen.lines()[index].clone();
    let (_, body) = line.split_once(" body=").unwrap();
    serde_json::from_str(body).unwrap()
}

fn claude_request(url: &str) -> String {
    json!({
        "model": "alias", "max_tokens": 64,
        "messages": [{"role": "user", "content": [
            {"type": "text", "text": "describe"},
            {"type": "image", "source": {"type": "url", "url": url}}
        ]}]
    })
    .to_string()
}

async fn generate(h: &Harness, provider: &str, url: &str) -> (StatusCode, String) {
    let ctx = h.context_for(
        provider,
        OperationKey {
            operation: Operation::GenerateContent,
            dialect: Dialect::Claude,
        },
        "media",
        1,
        None,
    );
    match h
        .core
        .generate_content(ctx, request(&claude_request(url)))
        .await
    {
        Ok(execution) => {
            let (response, _) = execution.into_parts();
            (response.status, read(response.body).await)
        }
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

fn gemini_reply() -> Reply {
    json_reply(
        StatusCode::OK,
        json!({
            "responseId": "r", "modelVersion": "m",
            "candidates": [{"content": {"role": "model", "parts": [{"text": "a pixel"}]}, "finishReason": "STOP"}],
            "usageMetadata": {"promptTokenCount": 3, "candidatesTokenCount": 2, "totalTokenCount": 5}
        }),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn image_url_gemini_cannot_read_is_fetched_and_sent_inline() {
    tokio::spawn(async {
        let (url, hits) = image_server().await;
        let h = with_policy(
            harness(full(), "round_robin").await,
            Arc::new(AllowAllFetchPolicy),
        );
        seed_provider(&h, "gem", "https://gemini.example", "gemini").await;
        h.script(vec![gemini_reply()]);
        let (status, body) = generate(&h, "gem", &url).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body.contains("a pixel"), "{body}");
        assert_eq!(*hits.lock().unwrap(), 1);
        let sent = sent_body(&h, 0);
        let image = &sent["contents"][0]["parts"][1]["inlineData"];
        assert_eq!(image["mimeType"], "image/png", "{sent}");
        assert_eq!(image["data"], PNG);
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn image_url_chat_reads_itself_is_passed_through_unfetched() {
    tokio::spawn(async {
        let (url, hits) = image_server().await;
        let h = harness(full(), "round_robin").await;
        seed_provider(&h, "chat", "https://chat.example", "openai_chat").await;
        h.script(vec![json_reply(
            StatusCode::OK,
            json!({
                "id": "c", "object": "chat.completion", "created": 1, "model": "m",
                "choices": [{"index": 0, "message": {"role": "assistant", "content": "a pixel"}, "finish_reason": "stop"}],
                "usage": {"prompt_tokens": 3, "completion_tokens": 2, "total_tokens": 5}
            }),
        )]);
        let (status, body) = generate(&h, "chat", &url).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(*hits.lock().unwrap(), 0);
        let sent = sent_body(&h, 0);
        assert_eq!(
            sent["messages"][0]["content"][1]["image_url"]["url"], url,
            "{sent}"
        );
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn refused_fetch_fails_the_request_before_any_upstream_call() {
    tokio::spawn(async {
        let (url, hits) = image_server().await;
        // The default policy refuses loopback addresses.
        let h = harness(full(), "round_robin").await;
        seed_provider(&h, "gem", "https://gemini.example", "gemini").await;
        h.script(vec![gemini_reply()]);
        let (status, body) = generate(&h, "gem", &url).await;
        assert_ne!(status, StatusCode::OK, "{body}");
        assert_eq!(*hits.lock().unwrap(), 0);
        assert!(h.client.seen.lines().is_empty());
    })
    .await
    .unwrap();
}
