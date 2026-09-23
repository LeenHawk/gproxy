#![cfg(not(target_arch = "wasm32"))]

mod support;
use support::*;

use futures_util::{SinkExt, StreamExt};
use gproxy_core::{
    BlockSource, CapturePolicy, CoreError, ExecutionTarget, ObservationPolicy, RequestContext,
    UsageState, keys,
};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest,
    capability::UpstreamConnection,
    connection::{Bytes, WebSocket, WsFrame, WsReceiver},
};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::json;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn rotates_credentials_rewrites_request_uses_endpoint_and_settles_usage() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![
        json_reply(
            StatusCode::OK,
            json!({"ok": 1, "usage": {"input_tokens": 3, "output_tokens": 5}}),
        ),
        json_reply(StatusCode::OK, json!({"ok": 2})),
    ]);
    let execution = h
        .core
        .stream_generate_content(h.context("r1", 3, None), request("{\"q\":1}"))
        .await
        .unwrap();
    assert_eq!(execution.response().status, StatusCode::OK);
    let (response, completion) = execution.into_parts();
    assert_eq!(
        read(response.body).await,
        "{\"ok\":1,\"usage\":{\"input_tokens\":3,\"output_tokens\":5}}"
    );
    let report = completion.await.unwrap();
    assert_eq!(report.state, UsageState::Completed);
    assert_eq!(report.exchanges.len(), 1);
    assert_eq!(report.exchanges[0].credential_id, "a");
    assert_eq!(report.exchanges[0].usage.tokens.input_tokens, Some(3));
    assert_eq!(
        h.observer.reports.lock().unwrap().len(),
        1,
        "funnel called usage once"
    );

    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].starts_with(
            "POST https://alt.example/v1/responses auth=Bearer ka tag=new body={\"q\":1}"
        ),
        "{}",
        seen[0]
    );
    let log = h.observer.log.lines();
    assert!(
        log.iter()
            .any(|l| l.starts_with("r1-1 0 req POST https://alt.example/v1/responses tag=new")),
        "{log:?}"
    );
    assert!(log.iter().any(|l| l.contains("req-chunk {\"q\":1}")));
    assert!(log.iter().any(|l| l.contains("resp 200 OK")));
    assert!(log.iter().any(|l| l.contains("resp-chunk {\"ok\":1")));
    assert!(log.iter().any(|l| l == "r1-1 finish Complete"), "{log:?}");
    assert!(log.iter().any(|l| l.starts_with("trace r1-1 a Succeeded")));

    let second = h
        .core
        .stream_generate_content(h.context("r2", 3, None), request("{}"))
        .await
        .unwrap();
    let (response, completion) = second.into_parts();
    read(response.body).await;
    assert_eq!(completion.await.unwrap().state, UsageState::Completed);
    assert!(
        h.client.seen.lines()[1].contains("auth=Bearer kb"),
        "round robin advances"
    );
}

#[tokio::test]
async fn rate_limit_blocks_the_credential_persistently_and_fails_over() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![
        (
            StatusCode::TOO_MANY_REQUESTS,
            vec![("retry-after", "120")],
            vec![Bytes::from_static(b"slow")],
        ),
        json_reply(StatusCode::OK, json!({"ok": true})),
        json_reply(StatusCode::OK, json!({"ok": true})),
    ]);
    let execution = h
        .core
        .stream_generate_content(h.context("r1", 3, None), request("{}"))
        .await
        .unwrap();
    let (response, completion) = execution.into_parts();
    assert_eq!(response.status, StatusCode::OK);
    read(response.body).await;
    let report = completion.await.unwrap();
    assert_eq!(
        report.exchanges.len(),
        1,
        "a served answer without reported usage is estimated locally"
    );
    let estimated = &report.exchanges[0].usage;
    assert_eq!(
        estimated.dimensions.get("estimated").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        estimated.completeness,
        gproxy_channel::channel::UsageCompleteness::Partial
    );
    assert_eq!(
        estimated.tokens.input_tokens, None,
        "`{{}}` carries no prompt text: nothing to count"
    );
    assert_eq!(
        estimated.tokens.output_tokens,
        Some(6),
        "half the 11 bytes of `{{\"ok\":true}}`, rounded up"
    );
    let seen = h.client.seen.lines();
    assert!(seen[0].contains("auth=Bearer ka"));
    assert!(seen[1].contains("auth=Bearer kb"));

    let rows = h
        .core
        .store()
        .load_control_data()
        .await
        .unwrap()
        .credential_blocks;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].credential_id, "a");
    assert_eq!(rows[0].source["kind"], "rate_limited");
    let cached: gproxy_core::CredentialBlocks = serde_json::from_slice(
        &h.core
            .cache()
            .get(&keys::credential_blocks("p", "a"))
            .await
            .unwrap()
            .unwrap()
            .value,
    )
    .unwrap();
    assert!(matches!(cached.blocks[0].source, BlockSource::RateLimited));
    assert!(cached.blocks[0].until_ms - cached.blocks[0].observed_at_ms == 120_000);
    let log = h.observer.log.lines();
    assert!(
        log.iter().any(|l| l == "r1-1 finish Complete"),
        "superseded 429 body is drained and captured: {log:?}"
    );

    let third = h
        .core
        .stream_generate_content(h.context("r2", 3, None), request("{}"))
        .await
        .unwrap();
    read(third.into_parts().0.body).await;
    assert!(
        h.client.seen.lines()[2].contains("auth=Bearer kb"),
        "blocked credential is skipped"
    );
}

#[tokio::test]
async fn repeated_server_errors_trip_a_failure_block_and_the_last_answer_is_returned() {
    let h = harness(full(), "sticky").await;
    h.script(vec![
        json_reply(StatusCode::BAD_GATEWAY, json!({"e": 1})),
        json_reply(StatusCode::BAD_GATEWAY, json!({"e": 2})),
        json_reply(StatusCode::BAD_GATEWAY, json!({"e": 3})),
        json_reply(StatusCode::BAD_GATEWAY, json!({"e": 4})),
    ]);
    let ctx = h.context("r1", 4, Some("s1"));
    let one_credential = Arc::new(RequestContext {
        target: ExecutionTarget {
            requested_model: None,
            provider: ctx.target.provider.clone(),
            upstream_model: ctx.target.upstream_model.clone(),
            credentials: vec![ctx.target.credentials[0].clone()],
        },
        request_id: ctx.request_id.clone(),
        attribution: ctx.attribution.clone(),
        snapshot: ctx.snapshot.clone(),
        scope: ctx.scope.clone(),
        session: ctx.session.clone(),
        operation: ctx.operation,
        budgets: Vec::new(),
        max_attempts: ctx.max_attempts,
        started_at_ms: 0,
        deadline: None,
        cancellation: CancellationToken::new(),
    });
    let execution = h
        .core
        .stream_generate_content(one_credential.clone(), request("{}"))
        .await
        .unwrap();
    let (response, completion) = execution.into_parts();
    assert_eq!(response.status, StatusCode::BAD_GATEWAY);
    assert_eq!(
        read(response.body).await,
        "{\"e\":3}",
        "three attempts, then the third failure is credential-blocked"
    );
    assert_eq!(completion.await.unwrap().state, UsageState::Failed);
    let cached: gproxy_core::CredentialBlocks = serde_json::from_slice(
        &h.core
            .cache()
            .get(&keys::credential_blocks("p", "a"))
            .await
            .unwrap()
            .unwrap()
            .value,
    )
    .unwrap();
    assert!(
        matches!(
            cached.blocks[0].source,
            BlockSource::Failures { consecutive: 3 }
        ),
        "{:?}",
        cached.blocks
    );
    assert_eq!(
        cached.blocks[0].scope,
        gproxy_channel::channel::QuotaScope::Models(vec!["gpt-x".into()])
    );

    let error = h
        .core
        .stream_generate_content(one_credential, request("{}"))
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::NoUsableCredential), "{error}");
    let report = h.observer.reports.lock().unwrap().pop().unwrap();
    assert_eq!(
        report.state,
        UsageState::Failed,
        "a failed request still reaches the funnel"
    );
}

#[tokio::test]
async fn policy_off_skips_capture_and_usage_and_dead_credentials_are_never_picked() {
    let h = harness(
        ObservationPolicy {
            usage: false,
            capture: CapturePolicy::Off,
            trace: false,
        },
        "round_robin",
    )
    .await;
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"usage": {"input_tokens": 1}}),
    )]);
    let execution = h
        .core
        .stream_generate_content(h.context("r1", 1, None), request("{}"))
        .await
        .unwrap();
    let (response, completion) = execution.into_parts();
    read(response.body).await;
    let report = completion.await.unwrap();
    assert_eq!(report.state, UsageState::Skipped);
    assert!(report.exchanges.is_empty());
    assert!(
        h.observer.reports.lock().unwrap().is_empty(),
        "usage funnel not called when disabled"
    );
    assert!(h.observer.log.lines().is_empty(), "no capture, no trace");

    h.core
        .store()
        .credentials()
        .set_status_many(vec![
            gproxy_store::operations::credentials::CredentialStatusUpdate {
                id: "a".into(),
                expected_version: 0,
                status: gproxy_core::CredentialStatus::Dead,
                reason: Some("invalid_grant".into()),
            },
            gproxy_store::operations::credentials::CredentialStatusUpdate {
                id: "b".into(),
                expected_version: 0,
                status: gproxy_core::CredentialStatus::Dead,
                reason: Some("revoked".into()),
            },
        ])
        .await
        .unwrap();
    h.core
        .reload_credentials(&["a".into(), "b".into()])
        .await
        .unwrap();
    let error = h
        .core
        .stream_generate_content(h.context("r2", 2, None), request("{}"))
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::CredentialDead { .. }), "{error}");
}

#[tokio::test]
async fn streaming_request_bodies_are_buffered_for_replay_and_sse_responses_are_rewritten_per_event()
 {
    let h = harness(full(), "round_robin").await;
    h.script(vec![
        json_reply(StatusCode::INTERNAL_SERVER_ERROR, json!({})),
        (
            StatusCode::OK,
            vec![("content-type", "text/event-stream")],
            vec![
                Bytes::from_static(b"event: message\ndata: {\"delta\":\"my sec"),
                Bytes::from_static(b"ret\"}\n\ndata: [DONE]\n\n"),
            ],
        ),
    ]);
    let body: Vec<Result<Bytes, gproxy_protocol::connection::TransportError>> = vec![
        Ok(Bytes::from_static(b"{\"pa")),
        Ok(Bytes::from_static(b"rt\":2}")),
    ];
    let wire = WireRequest {
        method: Method::POST,
        path: "/v1/responses".into(),
        query: None,
        headers: HeaderMap::new(),
        body: HttpBody::Stream(Box::pin(futures_util::stream::iter(body))),
    };
    let execution = h
        .core
        .stream_generate_content(h.context("r1", 2, None), wire)
        .await
        .unwrap();
    let (response, completion) = execution.into_parts();
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        read(response.body).await,
        "event: message\ndata: {\"delta\":\"my ***\"}\n\ndata: [DONE]\n\n"
    );
    assert_eq!(completion.await.unwrap().state, UsageState::Completed);
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 2);
    assert!(seen[0].ends_with("body={\"part\":2}"), "{}", seen[0]);
    assert!(
        seen[1].ends_with("body={\"part\":2}"),
        "buffered body replayed: {}",
        seen[1]
    );
    assert!(seen[0].contains("auth=Bearer ka") && seen[1].contains("auth=Bearer kb"));
}

#[tokio::test]
async fn cancellation_before_dispatch_settles_as_cancelled() {
    let h = harness(full(), "round_robin").await;
    let ctx = h.context("r1", 2, None);
    ctx.cancellation.cancel();
    let error = h
        .core
        .stream_generate_content(ctx, request("{}"))
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::Cancelled));
    let report = h.observer.reports.lock().unwrap().pop().unwrap();
    assert_eq!(report.state, UsageState::Cancelled);
    assert!(h.client.seen.lines().is_empty());
}

#[tokio::test]
async fn websocket_handshake_fails_over_and_the_socket_is_observed_in_both_directions() {
    let h = harness(full(), "round_robin").await;
    h.script_ws(vec![
        WsReply::Rejected(StatusCode::TOO_MANY_REQUESTS, "busy"),
        WsReply::Connected(vec![
            WsFrame::Text("{\"type\":\"delta\",\"delta\":\"a secret\"}".into()),
            WsFrame::Binary(Bytes::from_static(b"\x01")),
        ]),
    ]);
    let ctx = Arc::new(RequestContext {
        operation: OperationKey {
            operation: Operation::ConnectRealtime,
            dialect: Dialect::OpenAi,
        },
        ..Arc::try_unwrap(h.context("r1", 3, None)).ok().unwrap()
    });
    let mut headers = HeaderMap::new();
    headers.insert("x-client-tag", HeaderValue::from_static("old"));
    let execution = h
        .core
        .connect_realtime(
            ctx,
            WireRequest {
                method: Method::GET,
                path: "/v1/realtime".into(),
                query: None,
                headers,
                body: (),
            },
        )
        .await
        .unwrap();
    let (connection, completion) = execution.into_parts();
    let UpstreamConnection::Connected { handshake, socket } = connection else {
        panic!("second credential connects");
    };
    assert_eq!(handshake.status, StatusCode::SWITCHING_PROTOCOLS);
    let seen = h.client.seen.lines();
    assert!(seen[0].contains("auth=Bearer ka tag=new"), "{}", seen[0]);
    assert!(seen[1].contains("auth=Bearer kb tag=new"), "{}", seen[1]);
    let WebSocket {
        mut incoming,
        mut outgoing,
    } = socket;
    assert_eq!(
        incoming.next().await.unwrap().unwrap(),
        WsFrame::Text("{\"type\":\"delta\",\"delta\":\"a ***\"}".into())
    );
    assert_eq!(
        incoming.next().await.unwrap().unwrap(),
        WsFrame::Binary(Bytes::from_static(b"\x01"))
    );
    outgoing
        .send(WsFrame::Text("{\"type\":\"client\"}".into()))
        .await
        .unwrap();
    assert_eq!(h.client.sent_frames.lock().unwrap().len(), 1);
    assert!(incoming.next().await.is_none());
    let report = completion.await.unwrap();
    assert_eq!(report.state, UsageState::Completed);
    // The superseded handshake rejection has been drained and captured.
    tokio::task::yield_now().await;
    let log = h.observer.log.lines();
    assert_eq!(
        log.iter().filter(|l| l.contains(" frame")).count(),
        3,
        "{log:?}"
    );
    assert!(log.iter().any(|l| l == "r1-1 finish Complete"), "{log:?}");
    assert!(log.iter().any(|l| l == "r1-2 finish Complete"), "{log:?}");
    let rows = h
        .core
        .store()
        .load_control_data()
        .await
        .unwrap()
        .credential_blocks;
    assert_eq!(rows[0].credential_id, "a");
}

#[tokio::test]
async fn dropping_a_body_early_settles_as_cancelled() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![(
        StatusCode::OK,
        vec![("content-type", "text/event-stream")],
        vec![
            Bytes::from_static(b"data: 1\n\n"),
            Bytes::from_static(b"data: 2\n\n"),
        ],
    )]);
    let execution = h
        .core
        .stream_generate_content(h.context("r1", 1, None), request("{}"))
        .await
        .unwrap();
    let (response, completion) = execution.into_parts();
    let HttpBody::Stream(mut body) = response.body else {
        panic!()
    };
    assert_eq!(body.next().await.unwrap().unwrap().as_ref(), b"data: 1\n\n");
    drop(body);
    let report = tokio::time::timeout(std::time::Duration::from_secs(2), completion)
        .await
        .expect("drop settles")
        .unwrap();
    assert_eq!(report.state, UsageState::Cancelled);
    assert!(
        h.observer
            .log
            .lines()
            .iter()
            .any(|l| l == "r1-1 finish Cancelled")
    );
}

#[tokio::test]
async fn chat_client_is_converted_to_a_claude_upstream_with_failover_inside_the_conversion() {
    let h = harness(full(), "round_robin").await;
    let claude_message = json!({
        "id": "msg_native", "type": "message", "role": "assistant", "model": "gpt-x",
        "content": [{"type": "text", "text": "answer"}], "stop_reason": "end_turn",
        "usage": {"input_tokens": 4, "output_tokens": 2}
    });
    h.script(vec![
        (
            StatusCode::TOO_MANY_REQUESTS,
            vec![("retry-after", "5")],
            vec![Bytes::from_static(b"{\"error\":\"busy\"}")],
        ),
        json_reply(StatusCode::OK, claude_message),
    ]);
    let ctx = h.context_for(
        "claude",
        OperationKey {
            operation: Operation::GenerateContent,
            dialect: Dialect::OpenAiChat,
        },
        "r1",
        3,
        Some("session-1"),
    );
    let mut headers = HeaderMap::new();
    headers.insert("x-client-tag", HeaderValue::from_static("old"));
    let wire = WireRequest {
        method: Method::POST,
        path: "/v1/chat/completions".into(),
        query: None,
        headers,
        body: HttpBody::Bytes(Bytes::from_static(
            b"{\"model\":\"alias\",\"max_completion_tokens\":64,\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}]}",
        )),
    };
    let execution = h.core.generate_content(ctx, wire).await.unwrap();
    let (response, completion) = execution.into_parts();
    assert_eq!(response.status, StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&read(response.body).await).unwrap();
    assert_eq!(body["object"], "chat.completion");
    assert_eq!(body["choices"][0]["message"]["content"], "answer");
    assert_eq!(body["usage"]["prompt_tokens"], 4);
    let report = completion.await.unwrap();
    assert_eq!(report.state, UsageState::Completed);
    assert_eq!(
        report.exchanges.len(),
        1,
        "the successful native exchange reports usage"
    );
    assert_eq!(report.exchanges[0].credential_id, "cl2");
    assert_eq!(report.exchanges[0].usage.tokens.input_tokens, Some(4));

    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 2);
    assert!(
        seen[0].starts_with("POST https://claude.example/v1/messages auth=Bearer k1"),
        "{}",
        seen[0]
    );
    assert!(
        seen[1].starts_with("POST https://claude.example/v1/messages auth=Bearer k2"),
        "{}",
        seen[1]
    );
    assert!(
        seen[1].contains("\"messages\":[{\"role\":\"user\"")
            && seen[1].contains("\"max_tokens\":64"),
        "native Claude body: {}",
        seen[1]
    );
    let rows = h
        .core
        .store()
        .load_control_data()
        .await
        .unwrap()
        .credential_blocks;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].credential_id, "cl1");
    let states = h.core.store().load_control_data().await.unwrap();
    let _ = states;
    let log = h.observer.log.lines();
    assert!(
        log.iter()
            .any(|l| l.starts_with("trace r1-1 cl1 Rejected { status: 429")),
        "{log:?}"
    );
    assert!(
        log.iter()
            .any(|l| l.starts_with("trace r1-2 cl2 Succeeded")),
        "{log:?}"
    );
}

#[tokio::test]
async fn chat_client_stream_is_converted_from_a_claude_sse_upstream() {
    let h = harness(full(), "round_robin").await;
    let events = [
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"gpt-x\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":4,\"output_tokens\":0}}}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hel\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":2}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    ];
    h.script(vec![(
        StatusCode::OK,
        vec![("content-type", "text/event-stream")],
        events
            .iter()
            .map(|e| Bytes::from_static(e.as_bytes()))
            .collect(),
    )]);
    let ctx = h.context_for(
        "claude",
        OperationKey {
            operation: Operation::StreamGenerateContent,
            dialect: Dialect::OpenAiChat,
        },
        "r1",
        2,
        None,
    );
    let wire = WireRequest {
        method: Method::POST,
        path: "/v1/chat/completions".into(),
        query: None,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from_static(
            b"{\"model\":\"alias\",\"stream\":true,\"max_completion_tokens\":32,\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}]}",
        )),
    };
    let execution = h.core.stream_generate_content(ctx, wire).await.unwrap();
    let (response, completion) = execution.into_parts();
    assert_eq!(response.status, StatusCode::OK);
    let text = read(response.body).await;
    let frames: Vec<&str> = text.split("\n\n").filter(|f| !f.is_empty()).collect();
    let mut content = String::new();
    let mut finish = None;
    for frame in &frames {
        let data = frame
            .strip_prefix("data: ")
            .unwrap_or_else(|| panic!("frame {frame:?}"));
        if data == "[DONE]" {
            continue;
        }
        let chunk: serde_json::Value = serde_json::from_str(data).unwrap();
        assert_eq!(chunk["object"], "chat.completion.chunk", "{data}");
        if let Some(piece) = chunk["choices"][0]["delta"]["content"].as_str() {
            content.push_str(piece);
        }
        if let Some(reason) = chunk["choices"][0]["finish_reason"].as_str() {
            finish = Some(reason.to_owned());
        }
    }
    assert_eq!(content, "hello");
    assert_eq!(finish.as_deref(), Some("stop"));
    assert_eq!(frames.last().copied(), Some("data: [DONE]"));

    let report = completion.await.unwrap();
    assert_eq!(report.state, UsageState::Completed);
    assert_eq!(
        report.exchanges.len(),
        1,
        "the native body released after its terminal event still reports usage"
    );
    assert_eq!(report.exchanges[0].usage.tokens.input_tokens, Some(4));
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].starts_with("POST https://claude.example/v1/messages auth=Bearer k1")
            && seen[0].contains("\"stream\":true"),
        "{}",
        seen[0]
    );
}

fn chat_stream_request(body: &'static str) -> WireRequest<HttpBody> {
    WireRequest {
        method: Method::POST,
        path: "/v1/chat/completions".into(),
        query: None,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from_static(body.as_bytes())),
    }
}

/// Parse a Chat SSE body into (per-choice content, finish reasons, usage chunk).
fn parse_chat_sse(text: &str) -> (Vec<String>, Vec<Option<String>>, Option<serde_json::Value>) {
    let mut content: Vec<String> = Vec::new();
    let mut finish: Vec<Option<String>> = Vec::new();
    let mut usage = None;
    let frames: Vec<&str> = text.split("\n\n").filter(|f| !f.is_empty()).collect();
    assert_eq!(frames.last().copied(), Some("data: [DONE]"), "{text}");
    for frame in &frames {
        let data = frame
            .strip_prefix("data: ")
            .unwrap_or_else(|| panic!("frame {frame:?}"));
        if data == "[DONE]" {
            continue;
        }
        let chunk: serde_json::Value = serde_json::from_str(data).unwrap();
        assert_eq!(chunk["object"], "chat.completion.chunk", "{data}");
        if !chunk["usage"].is_null() {
            usage = Some(chunk["usage"].clone());
        }
        for choice in chunk["choices"].as_array().into_iter().flatten() {
            let index = choice["index"].as_u64().unwrap() as usize;
            while content.len() <= index {
                content.push(String::new());
                finish.push(None);
            }
            if let Some(piece) = choice["delta"]["content"].as_str() {
                content[index].push_str(piece);
            }
            if let Some(reason) = choice["finish_reason"].as_str() {
                finish[index] = Some(reason.to_owned());
            }
        }
    }
    (content, finish, usage)
}

fn claude_message(text: &str) -> serde_json::Value {
    json!({
        "id": "msg_native", "type": "message", "role": "assistant", "model": "gpt-x",
        "content": [{"type": "text", "text": text}], "stop_reason": "end_turn",
        "usage": {"input_tokens": 4, "output_tokens": 2}
    })
}

#[tokio::test]
async fn chat_stream_is_synthesized_when_the_upstream_only_generates_buffered() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![json_reply(StatusCode::OK, claude_message("answer"))]);
    let ctx = h.context_for(
        "claude-buffered",
        OperationKey {
            operation: Operation::StreamGenerateContent,
            dialect: Dialect::OpenAiChat,
        },
        "r1",
        1,
        None,
    );
    let wire = chat_stream_request(
        "{\"model\":\"alias\",\"stream\":true,\"stream_options\":{\"include_usage\":true},\"max_completion_tokens\":32,\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}]}",
    );
    let execution = h.core.stream_generate_content(ctx, wire).await.unwrap();
    let (response, completion) = execution.into_parts();
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        response.headers.get("content-type").unwrap(),
        "text/event-stream"
    );
    let (content, finish, usage) = parse_chat_sse(&read(response.body).await);
    assert_eq!(content, vec!["answer".to_owned()]);
    assert_eq!(finish, vec![Some("stop".to_owned())]);
    assert_eq!(usage.unwrap()["prompt_tokens"], 4);
    let report = completion.await.unwrap();
    assert_eq!(report.state, UsageState::Completed);
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].starts_with("POST https://buffered.example/v1/messages auth=Bearer kb1")
            && seen[0].contains("\"stream\":false"),
        "the upstream is asked for a buffered result: {}",
        seen[0]
    );
}

#[tokio::test]
async fn chat_two_candidates_fan_out_into_two_buffered_claude_calls() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![
        json_reply(StatusCode::OK, claude_message("one")),
        json_reply(StatusCode::OK, claude_message("two")),
    ]);
    let ctx = h.context_for(
        "claude",
        OperationKey {
            operation: Operation::GenerateContent,
            dialect: Dialect::OpenAiChat,
        },
        "r1",
        1,
        None,
    );
    let wire = chat_stream_request(
        "{\"model\":\"alias\",\"n\":2,\"max_completion_tokens\":32,\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}]}",
    );
    let execution = h.core.generate_content(ctx, wire).await.unwrap();
    let (response, completion) = execution.into_parts();
    assert_eq!(response.status, StatusCode::OK);
    let body: serde_json::Value = serde_json::from_str(&read(response.body).await).unwrap();
    assert_eq!(body["object"], "chat.completion");
    let choices = body["choices"].as_array().unwrap();
    assert_eq!(choices.len(), 2);
    assert_eq!(choices[0]["index"], 0);
    assert_eq!(choices[0]["message"]["content"], "one");
    assert_eq!(choices[1]["index"], 1);
    assert_eq!(choices[1]["message"]["content"], "two");
    assert_eq!(
        body["usage"]["prompt_tokens"], 8,
        "per-call charges are summed"
    );
    let report = completion.await.unwrap();
    assert_eq!(report.state, UsageState::Completed);
    assert_eq!(report.exchanges.len(), 2, "both child calls report usage");
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 2);
    assert!(
        seen.iter().all(
            |line| line.starts_with("POST https://claude.example/v1/messages")
                && !line.contains("\"n\":")
        ),
        "{seen:?}"
    );
}

#[tokio::test]
async fn chat_two_candidates_stream_through_two_claude_sse_calls() {
    let h = harness(full(), "round_robin").await;
    let sse = |text: &str| -> Vec<Bytes> {
        vec![
            Bytes::from_static(b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"gpt-x\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":4,\"output_tokens\":0}}}\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n"),
            Bytes::from(format!("event: content_block_delta\ndata: {{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"text_delta\",\"text\":\"{text}\"}}}}\n\n")),
            Bytes::from_static(b"event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":2}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"),
        ]
    };
    h.script(vec![
        (
            StatusCode::OK,
            vec![("content-type", "text/event-stream")],
            sse("one"),
        ),
        (
            StatusCode::OK,
            vec![("content-type", "text/event-stream")],
            sse("two"),
        ),
    ]);
    let ctx = h.context_for(
        "claude",
        OperationKey {
            operation: Operation::StreamGenerateContent,
            dialect: Dialect::OpenAiChat,
        },
        "r1",
        1,
        None,
    );
    let wire = chat_stream_request(
        "{\"model\":\"alias\",\"n\":2,\"stream\":true,\"max_completion_tokens\":32,\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}]}",
    );
    let execution = h.core.stream_generate_content(ctx, wire).await.unwrap();
    let (response, completion) = execution.into_parts();
    assert_eq!(response.status, StatusCode::OK);
    let (content, finish, _) = parse_chat_sse(&read(response.body).await);
    assert_eq!(content, vec!["one".to_owned(), "two".to_owned()]);
    assert_eq!(
        finish,
        vec![Some("stop".to_owned()), Some("stop".to_owned())]
    );
    let report = completion.await.unwrap();
    assert_eq!(report.state, UsageState::Completed);
    assert_eq!(report.exchanges.len(), 2);
    assert_eq!(h.client.seen.lines().len(), 2);
}

fn claude_sse(text: &str) -> Vec<Bytes> {
    vec![
        // Native message ids are unique per upstream response, as they are live.
        Bytes::from(format!("event: message_start\ndata: {{\"type\":\"message_start\",\"message\":{{\"id\":\"msg_{text}\",\"type\":\"message\",\"role\":\"assistant\",\"model\":\"gpt-x\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{{\"input_tokens\":4,\"output_tokens\":0,\"cache_creation_input_tokens\":0,\"cache_read_input_tokens\":0}}}}}}\n\nevent: content_block_start\ndata: {{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{{\"type\":\"text\",\"text\":\"\"}}}}\n\n")),
        Bytes::from(format!("event: content_block_delta\ndata: {{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"text_delta\",\"text\":\"{text}\"}}}}\n\n")),
        Bytes::from_static(b"event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":2}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"),
    ]
}

#[tokio::test]
async fn responses_websocket_client_is_served_turn_by_turn_over_a_claude_sse_upstream() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![
        (
            StatusCode::OK,
            vec![("content-type", "text/event-stream")],
            claude_sse("hello"),
        ),
        (
            StatusCode::OK,
            vec![("content-type", "text/event-stream")],
            claude_sse("again"),
        ),
    ]);
    let ctx = h.context_for(
        "claude",
        OperationKey {
            operation: Operation::StreamGenerateContent,
            dialect: Dialect::OpenAiResponsesWebSocket,
        },
        "r1",
        1,
        Some("session-ws"),
    );
    let execution = h
        .core
        .stream_generate_content_websocket(
            ctx,
            WireRequest {
                method: Method::GET,
                path: "/v1/responses".into(),
                query: None,
                headers: HeaderMap::new(),
                body: (),
            },
        )
        .await
        .unwrap();
    let (connection, completion) = execution.into_parts();
    let UpstreamConnection::Connected { handshake, socket } = connection else {
        panic!("the fabricated handshake is accepted");
    };
    assert_eq!(handshake.status, StatusCode::SWITCHING_PROTOCOLS);
    let WebSocket {
        mut incoming,
        mut outgoing,
    } = socket;

    let collect_turn = async |incoming: &mut WsReceiver, lane: Option<&str>| {
        let mut text = String::new();
        loop {
            let frame = incoming.next().await.unwrap().unwrap();
            let WsFrame::Text(payload) = frame else {
                panic!("server messages are text frames: {frame:?}");
            };
            let event: serde_json::Value = serde_json::from_str(&payload).unwrap();
            assert_ne!(event["type"], "error", "{payload}");
            assert_eq!(
                event["stream_id"].as_str(),
                lane,
                "every event carries the request lane: {payload}"
            );
            if event["type"] == "response.output_text.delta" {
                text.push_str(event["delta"].as_str().unwrap());
            }
            if event["type"] == "response.completed" {
                assert_eq!(event["response"]["status"], "completed");
                return text;
            }
        }
    };

    outgoing
        .send(WsFrame::Text(
            "{\"type\":\"response.create\",\"model\":\"alias\",\"max_output_tokens\":32,\"input\":\"hi\"}".into(),
        ))
        .await
        .unwrap();
    assert_eq!(collect_turn(&mut incoming, None).await, "hello");
    outgoing
        .send(WsFrame::Text(
            "{\"type\":\"response.create\",\"stream_id\":\"main\",\"model\":\"alias\",\"max_output_tokens\":32,\"input\":\"more\"}".into(),
        ))
        .await
        .unwrap();
    assert_eq!(collect_turn(&mut incoming, Some("main")).await, "again");

    // A malformed message is answered on the socket, not by closing it.
    outgoing
        .send(WsFrame::Text("{\"type\":\"nope\"}".into()))
        .await
        .unwrap();
    let WsFrame::Text(error) = incoming.next().await.unwrap().unwrap() else {
        panic!("error message");
    };
    let error: serde_json::Value = serde_json::from_str(&error).unwrap();
    assert_eq!(error["type"], "error");
    assert_eq!(error["status"], 400);

    outgoing.send(WsFrame::Close(None)).await.unwrap();
    assert_eq!(
        incoming.next().await.unwrap().unwrap(),
        WsFrame::Close(None)
    );
    assert!(incoming.next().await.is_none());
    let report = completion.await.unwrap();
    assert_eq!(report.state, UsageState::Completed);
    assert_eq!(report.exchanges.len(), 2, "one native exchange per turn");
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 2);
    assert!(
        seen.iter().all(
            |line| line.starts_with("POST https://claude.example/v1/messages")
                && line.contains("\"stream\":true")
        ),
        "{seen:?}"
    );
}

#[tokio::test]
async fn chat_stream_client_is_served_over_a_responses_websocket_upstream() {
    use gproxy_protocol::transform::{
        generate::stream::responses::synthesize_responses_stream,
        identity::{IdNamespace, IdentityFlow},
    };
    let h = harness(full(), "round_robin").await;
    let native: gproxy_protocol::wire::openai::responses::GenerateContentResponseBody =
        serde_json::from_value(json!({
            "id": "resp_1", "created_at": 123, "error": null, "incomplete_details": null,
            "instructions": null, "metadata": null, "model": "gpt-x", "object": "response",
            "output": [{"type": "message", "id": "msg_1", "role": "assistant", "status": "completed",
                "content": [{"type": "output_text", "text": "over ws", "annotations": [], "logprobs": []}]}],
            "parallel_tool_calls": true, "temperature": null, "tool_choice": "auto", "tools": [],
            "top_p": null, "status": "completed",
            "usage": {"input_tokens": 5, "output_tokens": 7, "total_tokens": 12,
                "input_tokens_details": {"cached_tokens": 0, "cache_write_tokens": 0},
                "output_tokens_details": {"reasoning_tokens": 0}}
        }))
        .unwrap();
    let frames = synthesize_responses_stream(
        native,
        &mut IdentityFlow::new(IdNamespace::with_bytes([7; 16])),
        Default::default(),
    )
    .unwrap()
    .value
    .into_iter()
    .map(|event| WsFrame::Text(serde_json::to_string(&event).unwrap()))
    .collect();
    h.script_ws(vec![WsReply::Connected(frames)]);
    let ctx = h.context_for(
        "ws",
        OperationKey {
            operation: Operation::StreamGenerateContent,
            dialect: Dialect::OpenAiChat,
        },
        "r1",
        1,
        None,
    );
    let wire = chat_stream_request(
        "{\"model\":\"alias\",\"stream\":true,\"max_completion_tokens\":32,\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}]}",
    );
    let execution = h.core.stream_generate_content(ctx, wire).await.unwrap();
    let (response, completion) = execution.into_parts();
    assert_eq!(response.status, StatusCode::OK);
    let (content, finish, _) = parse_chat_sse(&read(response.body).await);
    assert_eq!(content, vec!["over ws".to_owned()]);
    assert_eq!(finish, vec![Some("stop".to_owned())]);
    let report = completion.await.unwrap();
    assert_eq!(report.state, UsageState::Completed);
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].starts_with("WS https://ws.example/v1/responses auth=Bearer kws"),
        "{}",
        seen[0]
    );
    let sent = h.client.sent_frames.lock().unwrap();
    assert_eq!(sent.len(), 1, "one response.create per turn");
    let WsFrame::Text(text) = &sent[0] else {
        panic!("text");
    };
    let create: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(create["type"], "response.create");
    assert!(create.get("stream").is_none(), "{text}");
}

#[tokio::test]
async fn a_channel_keeps_state_across_requests_scoped_to_its_credential() {
    let h = harness(full(), "sticky").await;
    h.script(vec![
        json_reply(StatusCode::OK, json!({"ok": 1})),
        json_reply(StatusCode::OK, json!({"ok": 2})),
    ]);
    for id in ["r1", "r2"] {
        let ctx = h.context_for(
            "p",
            OperationKey {
                operation: Operation::CompactContent,
                dialect: Dialect::OpenAi,
            },
            id,
            1,
            Some("s-state"),
        );
        let execution = h
            .core
            .compact_content(
                ctx,
                WireRequest {
                    method: Method::POST,
                    path: "/v1/responses/compact".into(),
                    query: None,
                    headers: HeaderMap::new(),
                    body: HttpBody::Bytes(Bytes::from_static(b"{}")),
                },
            )
            .await
            .unwrap();
        read(execution.into_parts().0.body).await;
    }
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 2);
    let credential = if seen[0].contains("Bearer ka") {
        "a"
    } else {
        "b"
    };
    assert!(
        seen[1].contains(&format!("Bearer k{credential}")),
        "sticky: {seen:?}"
    );
    let row = h
        .core
        .store()
        .protocol_states()
        .get_many(&[(
            format!("channel\u{1f}p\u{1f}{credential}"),
            "turns".to_owned(),
        )])
        .await
        .unwrap()
        .remove(0)
        .expect("the channel's key lives under its credential scope");
    assert_eq!(row.payload, b"2", "two turns counted through CAS");
}
