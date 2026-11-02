#![cfg(all(feature = "remote", feature = "openai", not(target_arch = "wasm32")))]
mod support;
use futures_util::StreamExt;
use gproxy_client::OutboundClient;
use gproxy_client_sdk::*;
use gproxy_protocol::{
    HttpBody, WireResponse,
    capability::{CapabilityError, CapabilityFuture},
    connection::{ByteStream, Bytes},
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

async fn server(
    status: u16,
    headers: &str,
    body: Vec<u8>,
    fragment: bool,
) -> (String, tokio::task::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let headers = headers.to_owned();
    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let header_end = loop {
            let mut chunk = [0u8; 4096];
            let read = socket.read(&mut chunk).await.unwrap();
            assert_ne!(read, 0);
            request.extend_from_slice(&chunk[..read]);
            if let Some(pos) = request.windows(4).position(|v| v == b"\r\n\r\n") {
                break pos + 4;
            }
        };
        let header_text = String::from_utf8_lossy(&request[..header_end]).to_lowercase();
        let length: usize = header_text
            .lines()
            .find_map(|l| l.strip_prefix("content-length: "))
            .unwrap_or("0")
            .parse()
            .unwrap();
        while request.len() < header_end + length {
            let mut chunk = [0u8; 4096];
            let read = socket.read(&mut chunk).await.unwrap();
            assert_ne!(read, 0);
            request.extend_from_slice(&chunk[..read]);
        }
        socket.write_all(format!("HTTP/1.1 {status} test\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n",body.len()).as_bytes()).await.unwrap();
        if fragment {
            for byte in body {
                socket.write_all(&[byte]).await.unwrap();
                tokio::task::yield_now().await;
            }
        } else {
            socket.write_all(&body).await.unwrap();
        }
        request
    });
    (endpoint, handle)
}
fn request() -> GenerateRequest {
    support::request(
        Dialect::Openai,
        vec![Message {
            role: Role::User,
            content: vec![Block::text("hello")],
        }],
    )
}
fn captured_json(bytes: &[u8]) -> Value {
    let end = bytes.windows(4).position(|v| v == b"\r\n\r\n").unwrap() + 4;
    serde_json::from_slice(&bytes[end..]).unwrap()
}

#[tokio::test]
async fn remote_authenticates_renders_native_and_parses_buffered_response() {
    let (endpoint, server) = server(
        200,
        "Content-Type: application/json\r\n",
        support::openai().to_string().into_bytes(),
        false,
    )
    .await;
    let client = Client::new(
        backend::Remote::new(endpoint, "gateway-test-key")
            .await
            .unwrap(),
    );
    let result = client.generate(&request()).await.unwrap();
    assert_eq!(result.usage.cache_write_tokens, Some(4));
    let captured = server.await.unwrap();
    let headers = String::from_utf8_lossy(&captured).to_lowercase();
    assert!(headers.starts_with("post /v1/responses http/1.1\r\n"));
    assert!(headers.contains("authorization: bearer gateway-test-key\r\n"));
    let body = captured_json(&captured);
    assert_eq!(body["model"], "provider/model alias");
    assert_eq!(
        body["input"][0]["content"][0],
        json!({"type":"input_text","text":"hello"})
    );
}

#[tokio::test]
async fn remote_sse_handles_arbitrary_byte_boundaries_and_utf8() {
    let native = support::openai();
    let body = support::openai_events(&native)
        .into_iter()
        .map(|event| format!("data: {event}\r\n\r\n"))
        .collect::<String>();
    let (endpoint, server) = server(
        200,
        "Content-Type: text/event-stream\r\n",
        body.into_bytes(),
        true,
    )
    .await;
    let client = Client::new(backend::Remote::new(endpoint, "gateway").await.unwrap());
    let result = client
        .stream(&request())
        .await
        .unwrap()
        .aggregate()
        .await
        .unwrap();
    assert_eq!(
        result,
        dialects::openai::parse(serde_json::from_value(native).unwrap()).unwrap()
    );
    assert_eq!(captured_json(&server.await.unwrap())["stream"], true);
}

#[tokio::test]
async fn remote_maps_errors_and_preserves_status_body_and_retry_after_without_retries() {
    for (status, body) in [
        (401, "bad key"),
        (429, "slow down"),
        (400, "{\"error\":{\"code\":\"context_length_exceeded\"}}"),
        (400, "{\"error\":{\"code\":\"content_policy_violation\"}}"),
        (503, "original upstream body"),
    ] {
        let (endpoint, server) = server(
            status,
            "Retry-After: 7\r\n",
            body.as_bytes().to_vec(),
            false,
        )
        .await;
        let client = Client::new(backend::Remote::new(endpoint, "gateway").await.unwrap());
        let error = client.generate(&request()).await.unwrap_err();
        match (status, error) {
            (
                401,
                Error::Auth {
                    status: 401,
                    body: actual,
                },
            ) => assert_eq!(actual, body),
            (
                429,
                Error::RateLimit {
                    retry_after,
                    body: actual,
                },
            ) => {
                assert_eq!(retry_after.as_deref(), Some("7"));
                assert_eq!(actual, body);
            }
            (
                400,
                Error::ContextTooLong { body: actual } | Error::ContentBlocked { body: actual },
            ) => assert_eq!(actual, body),
            (
                503,
                Error::Upstream {
                    status: 503,
                    body: actual,
                },
            ) => assert_eq!(actual, body),
            (_, other) => panic!("unexpected error {other:?}"),
        }
        server.await.unwrap();
    }
}

#[cfg(feature = "gemini")]
#[tokio::test]
async fn gemini_model_alias_is_one_encoded_path_segment_and_sse_is_requested() {
    let body = format!("data: {}\n\n", support::gemini());
    let (endpoint, server) = server(
        200,
        "Content-Type: text/event-stream\r\n",
        body.into_bytes(),
        false,
    )
    .await;
    let client = Client::new(
        backend::Remote::new(format!("{endpoint}/mount/"), "gateway")
            .await
            .unwrap(),
    );
    let request = support::request(
        Dialect::Gemini,
        vec![Message {
            role: Role::User,
            content: vec![Block::text("hi")],
        }],
    );
    client
        .stream(&request)
        .await
        .unwrap()
        .aggregate()
        .await
        .unwrap();
    let captured = server.await.unwrap();
    assert!(String::from_utf8_lossy(&captured).starts_with("POST /mount/v1beta/models/provider%2Fmodel%20alias:streamGenerateContent?alt=sse HTTP/1.1\r\n"));
}

struct PendingTransport {
    dropped: Arc<AtomicBool>,
}
struct Guard(Arc<AtomicBool>);
impl Drop for Guard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}
impl OutboundClient for PendingTransport {
    fn send<'a>(
        &'a self,
        _request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, std::result::Result<WireResponse<HttpBody>, CapabilityError>> {
        let guard = Guard(self.dropped.clone());
        Box::pin(async move {
            let first = Bytes::from_static(b"data: {\"type\":\"response.output_text.delta\",\"item_id\":\"m\",\"output_index\":0,\"content_index\":0,\"delta\":\"hello\",\"sequence_number\":1,\"logprobs\":[]}\n\n");
            let bytes: ByteStream = Box::pin(futures_util::stream::unfold(
                (Some(first), guard),
                |(first, guard)| async {
                    if let Some(first) = first {
                        Some((Ok(first), (None, guard)))
                    } else {
                        std::future::pending().await
                    }
                },
            ));
            Ok(WireResponse {
                status: http::StatusCode::OK,
                headers: http::HeaderMap::new(),
                body: HttpBody::Stream(bytes),
            })
        })
    }
}
#[tokio::test]
async fn cancel_releases_the_live_response_without_buffering_or_spawning_a_task() {
    let dropped = Arc::new(AtomicBool::new(false));
    let remote = backend::Remote::with_transport(
        "http://example.test",
        "gateway",
        Arc::new(PendingTransport {
            dropped: dropped.clone(),
        }),
    )
    .unwrap();
    let client = Client::new(remote);
    let mut stream = client.stream(&request()).await.unwrap();
    assert!(matches!(stream.next().await, Some(Event::TextDelta { .. })));
    assert!(!dropped.load(Ordering::SeqCst));
    stream.cancel();
    assert!(dropped.load(Ordering::SeqCst));
}

#[cfg(all(feature = "claude", feature = "count-tokens"))]
#[tokio::test]
async fn count_tokens_uses_its_native_typed_parameters() {
    use protocol::claude::count_tokens::{
        CountTokensContextManagementResponse, CountTokensResponseBody,
    };
    for body in [
        CountTokensResponseBody::builder(42).build(),
        CountTokensResponseBody::builder(42)
            .context_management(CountTokensContextManagementResponse::builder(50).build())
            .build(),
    ] {
        let expected = serde_json::to_value(&body).unwrap();
        let (endpoint, server) = server(
            200,
            "Content-Type: application/json\r\n",
            serde_json::to_vec(&body).unwrap(),
            false,
        )
        .await;
        let client = Client::new(backend::Remote::new(endpoint, "gateway").await.unwrap());
        let native = serde_json::from_value(
            json!({"model":"claude","messages":[{"role":"user","content":"hi"}]}),
        )
        .unwrap();
        let params =
            operations::CountTokensParams::Claude(operations::Params::new(native, Vendor::Claude));
        let count = client.count_tokens(&params).await.unwrap();
        assert_eq!(count.input_tokens, Some(42));
        assert_eq!(count.provider_metadata[&Vendor::Claude]["native"], expected);
        let captured = server.await.unwrap();
        assert!(String::from_utf8_lossy(&captured).starts_with("POST /v1/messages/count_tokens "));
        assert!(String::from_utf8_lossy(&captured).contains("anthropic-version: 2023-06-01"));
    }
}
