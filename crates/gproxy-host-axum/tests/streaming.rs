#![cfg(not(target_arch = "wasm32"))]
//! The lease-lifetime rule, observed from outside.
//!
//! [`CallOutcome::admitted`](gproxy_app::CallOutcome) carries the request's
//! rate-limit charges, and a **concurrency** permit measures requests in
//! flight. A streamed response is in flight long after `App::call` returned,
//! so the host has to hold the decision until the last byte — this crate does
//! it by moving the whole thing into the response body's stream.
//!
//! There is exactly one way to assert that from outside: configure a
//! concurrency limit of one and watch a second request be refused **while the
//! first response is still open**, then accepted once it has ended. Anything
//! less (draining the body and then counting) would pass even if the lease
//! were dropped the instant the head was written, which is the bug this is
//! here to catch.

mod support;

use futures_util::StreamExt as _;
use gproxy_protocol::connection::Bytes;
use gproxy_seaorm::FixedDecimal;
use gproxy_store::entity::{limits::rate_limit, usage::usage_record};
use http::StatusCode;
use sea_orm::{EntityTrait, Set};
use serde_json::json;
use support::{Host, Reply, keyed, post};

/// One provider, one permitted key, and at most one request in flight.
async fn instance() -> Host {
    let host = Host::new().await;
    let handle = host.handle();
    support::person(&handle, "alice", "user").await;
    support::api_key(&handle, "k-alice", "alice", None, None).await;
    support::provider(&handle, "p1", &["m1"]).await;
    support::allow(&handle, "perm", "alice", None).await;
    support::credential(&handle, "c1", "p1", None, None, None).await;
    handle
        .store()
        .rate_limits()
        .create_many(vec![rate_limit::ActiveModel {
            id: Set("one-at-a-time".into()),
            user_id: Set(Some("alice".into())),
            api_key_id: Set(None),
            metric: Set("concurrency".into()),
            limit_value: Set(FixedDecimal::from_atoms(FixedDecimal::FACTOR)),
            period_seconds: Set(60),
            model_pattern: Set(Some("*".into())),
            enabled: Set(true),
        }])
        .await
        .unwrap();
    host.publish().await;
    host
}

fn call() -> http::Request<axum::body::Body> {
    keyed(
        post("/v1/messages", json!({"model": "p1/m1", "stream": true})),
        "k-alice",
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn a_streamed_response_holds_its_lease_until_the_last_byte() {
    let host = instance().await;
    let (chunks, receiver) = tokio::sync::mpsc::unbounded_channel();
    host.client
        .script(vec![Reply::Stream(StatusCode::OK, receiver)]);

    // The head is answered as soon as the upstream sends it; the body is still
    // open.
    let response = host.raw(call()).await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body().into_data_stream();

    chunks.send(Bytes::from_static(b"event: one\n\n")).unwrap();
    let first = body.next().await.unwrap().unwrap();
    assert_eq!(first, Bytes::from_static(b"event: one\n\n"));

    // **Mid-stream.** The permit is still charged, so the only other slot this
    // caller has is taken.
    let refused = host.send(call()).await;
    assert_eq!(
        refused.status,
        StatusCode::TOO_MANY_REQUESTS,
        "the lease was released before the response ended: {}",
        refused.text()
    );
    assert_eq!(refused.json()["error"]["code"], "rate_limited");
    assert!(
        refused.header("retry-after").is_some(),
        "a caller told to back off is told how long for"
    );

    // End the upstream body and drain what is left, which is what runs the
    // settlement and returns the permit.
    chunks.send(Bytes::from_static(b"event: two\n\n")).unwrap();
    drop(chunks);
    let mut rest = Vec::new();
    while let Some(chunk) = body.next().await {
        rest.extend_from_slice(&chunk.unwrap());
    }
    assert_eq!(rest, b"event: two\n\n");
    gproxy_host_axum::response::settled().await;

    // Freed. The next request is admitted.
    host.client
        .push(Reply::Http(StatusCode::OK, json!({"ok": true})));
    let after = host.send(call()).await;
    assert_eq!(after.status, StatusCode::OK, "{}", after.text());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_streamed_response_is_settled_when_it_ends_not_when_it_starts() {
    let host = instance().await;
    let (chunks, receiver) = tokio::sync::mpsc::unbounded_channel();
    host.client
        .script(vec![Reply::Sse(StatusCode::OK, receiver)]);

    let response = host.raw(call()).await;
    let mut body = response.into_body().into_data_stream();
    let delta = Bytes::from_static(b"data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hello\"}}\n\n");
    chunks.send(delta.clone()).unwrap();
    assert_eq!(body.next().await.unwrap().unwrap(), delta);

    assert!(
        rows(&host).await.is_empty(),
        "a request that is still streaming has not been settled"
    );

    drop(chunks);
    while body.next().await.is_some() {}

    let rows = rows(&host).await;
    assert_eq!(rows.len(), 1, "the settlement ran when the stream ended");
    assert_eq!(rows[0].output_tokens, Some(3));
    assert_eq!(rows[0].user_id.as_deref(), Some("alice"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_body_reaches_the_client_unbuffered() {
    let host = instance().await;
    let (chunks, receiver) = tokio::sync::mpsc::unbounded_channel();
    host.client
        .script(vec![Reply::Stream(StatusCode::OK, receiver)]);

    let response = host.raw(call()).await;
    let mut body = response.into_body().into_data_stream();

    // Each chunk is readable before the next one exists, which is the whole
    // property: a buffered implementation would block here forever.
    for index in 0..3_u8 {
        let payload = Bytes::from(format!("chunk-{index}"));
        chunks.send(payload.clone()).unwrap();
        assert_eq!(body.next().await.unwrap().unwrap(), payload);
    }
    drop(chunks);
    assert!(body.next().await.is_none());
}

async fn rows(host: &Host) -> Vec<usage_record::Model> {
    host.app
        .gproxy()
        .store()
        .usage_records()
        .query(usage_record::Entity::find())
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tool_error_does_not_close_the_response_stream() {
    let host = instance().await;
    let (chunks, receiver) = tokio::sync::mpsc::unbounded_channel();
    host.client.push(Reply::Sse(StatusCode::OK, receiver));
    let response = host
        .raw(keyed(
            post(
                "/v1/responses",
                json!({"model":"p1/m1","stream":true,"input":"hi"}),
            ),
            "k-alice",
        ))
        .await;
    let mut body = response.into_body().into_data_stream();
    let tool = Bytes::from_static(b"data: {\"type\":\"response.mcp_call.failed\",\"error\":{\"message\":\"tool unavailable\"}}\n\n");
    chunks.send(tool.clone()).unwrap();
    assert_eq!(body.next().await.unwrap().unwrap(), tool);
    assert!(!chunks.is_closed());
    let error = Bytes::from_static(b"data: {\"type\":\"error\",\"code\":\"server_error\",\"message\":\"upstream unavailable\",\"param\":null,\"sequence_number\":1}\n\n");
    chunks.send(error.clone()).unwrap();
    assert_eq!(body.next().await.unwrap().unwrap(), error);
    assert!(body.next().await.is_none());
    assert!(chunks.is_closed());
}

#[tokio::test(flavor = "multi_thread")]
async fn upstream_errors_reach_http_clients_and_close_without_upstream_eof() {
    use gproxy_store::entity::upstream::provider;
    use serde_json::Value;
    use std::time::Duration;

    for source in [
        "gemini",
        "claude",
        "openai_chat",
        "openai",
        "response.failed",
        "websocket",
        "websocket_sse",
    ] {
        let dialect = if source.starts_with("websocket") {
            "openai_responses_websocket"
        } else if source == "response.failed" {
            "openai"
        } else {
            source
        };
        for (client_dialect, path) in [
            ("openai_chat", "/v1/chat/completions"),
            ("openai", "/v1/responses"),
            ("claude", "/v1/messages"),
            (
                "gemini",
                "/v1beta/models/p1/m1:streamGenerateContent?alt=sse",
            ),
            ("gemini", "/v1beta/models/p1/m1:streamGenerateContent"),
        ] {
            for prefix in [false, true] {
                if prefix && source != "gemini" {
                    continue;
                }
                let host = instance().await;
                host.handle()
                    .store()
                    .providers()
                    .update_many(vec![provider::ActiveModel {
                        id: Set("p1".into()),
                        config: Set(json!({"test_dialect":dialect})),
                        ..Default::default()
                    }])
                    .await
                    .unwrap();
                host.publish().await;
                let bound = host.bind().await;
                let (chunks, receiver) = tokio::sync::mpsc::unbounded_channel();
                let socket = source
                    .starts_with("websocket")
                    .then(|| host.client.accept_socket());
                if socket.is_none() {
                    host.client.push(Reply::Sse(StatusCode::OK, receiver));
                }
                let send = |event: Value| {
                    if let Some(socket) = &socket {
                        socket
                            .send
                            .send(Ok(gproxy_protocol::connection::WsFrame::Text(
                                event.to_string(),
                            )))
                            .unwrap();
                    } else {
                        for part in format!("data: {event}\n\n").as_bytes().chunks(7) {
                            chunks.send(Bytes::copy_from_slice(part)).unwrap();
                        }
                    }
                };
                if prefix {
                    send(
                        json!({"candidates":[{"content":{"role":"model","parts":[{"text":"hello {\"error\":{\"message\":\"quoted content\"}}"}]}}]}),
                    );
                }
                let message = "服务器繁忙 (rate limited) | Upstream: {\"errors\":[]}";
                match source {
                    "gemini" => send(
                        json!({"error":{"code":429,"message":message,"status":"RESOURCE_EXHAUSTED"}}),
                    ),
                    "claude" => send(
                        json!({"type":"error","error":{"type":"rate_limit_error","message":message}}),
                    ),
                    "openai_chat" => send(
                        json!({"error":{"type":"rate_limit_error","code":"rate_limit_exceeded","message":message,"param":"model"}}),
                    ),
                    "openai" | "websocket_sse" => send(
                        json!({"type":"error","sequence_number":0,"code":"rate_limit_exceeded","message":message,"param":"model"}),
                    ),
                    "websocket" => send(json!({"type":"error","status":429,
                        "error":{"type":"rate_limit_error","code":"rate_limit_exceeded","message":message,"param":"model"}})),
                    _ => {
                        let mut response = json!({"id":"r1","created_at":1,"model":"m1","object":"response",
                            "status":"in_progress","error":null,"incomplete_details":null,"instructions":null,
                            "metadata":null,"output":[],"parallel_tool_calls":true,"temperature":null,
                            "top_p":null,"tools":[],"tool_choice":"auto"});
                        send(
                            json!({"type":"response.created","sequence_number":0,"response":response}),
                        );
                        response["status"] = json!("failed");
                        response["error"] = json!({"code":"rate_limit_exceeded","message":message});
                        send(
                            json!({"type":"response.failed","sequence_number":1,"response":response}),
                        );
                    }
                }
                // Keep the sender alive. The real HTTP client must receive clean
                // EOF before either its five-second deadline or the upstream idle timeout.
                let client = reqwest::Client::builder()
                    .no_proxy()
                    .timeout(Duration::from_secs(5))
                    .build()
                    .unwrap();
                let request = json!({"model":"p1/m1","stream":true,"max_tokens":64,"input":"hi",
                    "messages":[{"role":"user","content":"hi"}],"contents":[{"role":"user","parts":[{"text":"hi"}]}]});
                let response = client
                    .post(bound.http(path))
                    .bearer_auth("k-alice")
                    .header("content-type", "application/json")
                    .body(request.to_string())
                    .send()
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK, "{source} -> {path}");
                let array = response.headers()["content-type"] == "application/json";
                let text = response
                    .text()
                    .await
                    .unwrap_or_else(|error| panic!("{source} -> {path}: {error}"));
                assert!(
                    socket
                        .as_ref()
                        .map_or_else(|| chunks.is_closed(), |socket| socket.send.is_closed()),
                    "upstream retained: {source} -> {path}"
                );
                let events: Vec<Value> = if array {
                    serde_json::from_str(&text).unwrap()
                } else {
                    text.lines()
                        .filter_map(|line| line.strip_prefix("data: "))
                        .filter(|data| *data != "[DONE]")
                        .map(|data| serde_json::from_str(data).unwrap())
                        .collect()
                };
                let error = events.last().expect("downstream error event");
                let detail = error
                    .get("error")
                    .or_else(|| error.pointer("/response/error"))
                    .unwrap_or(error);
                assert_eq!(detail["message"], message, "{source} -> {path}: {text}");
                if client_dialect == "openai" && dialect != "openai" {
                    assert_eq!(error["type"], "error");
                    assert_eq!(error["status"], 429);
                    assert_eq!(error["sequence_number"], events.len() - 1);
                }
                if client_dialect == "openai_chat" && dialect != "openai_chat" {
                    assert_eq!(detail["type"], "rate_limit_error");
                    assert!(text.ends_with("data: [DONE]\n\n"));
                }
                if source == "gemini" && client_dialect == "openai_chat" {
                    assert_eq!(detail["code"], 429);
                }
                assert!(!text.contains("response.completed"));
                gproxy_host_axum::response::settled().await;
                let records = rows(&host).await;
                if socket.is_some() {
                    // A socket rejected before a metered response has no
                    // billable exchange, and must not create a success row.
                    assert!(records.is_empty(), "{source} -> {path}");
                    continue;
                }
                assert_eq!(records.len(), 1, "{source} -> {path}");
                assert_eq!(
                    records[0].state.as_deref(),
                    Some("failed"),
                    "{source} -> {path}"
                );
                // Switch back to a native request to check the concurrency lease.
                host.client
                    .push(Reply::Http(StatusCode::OK, json!({"ok":true})));
                let native_path = match dialect {
                    "gemini" => "/v1beta/models/p1/m1:generateContent",
                    "claude" => "/v1/messages",
                    "openai_chat" => "/v1/chat/completions",
                    _ => "/v1/responses",
                };
                let mut request = request;
                request["stream"] = json!(false);
                let next = host
                    .send(keyed(post(native_path, request), "k-alice"))
                    .await;
                assert_eq!(next.status, StatusCode::OK, "{}", next.text());
            }
        }
    }
}
