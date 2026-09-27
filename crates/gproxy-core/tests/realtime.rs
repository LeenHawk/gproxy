#![cfg(not(target_arch = "wasm32"))]
mod support;
use futures_util::StreamExt;
use gproxy_core::{CoreError, RequestContext};
use gproxy_protocol::{
    Dialect, Operation, OperationKey, WireRequest,
    capability::UpstreamConnection,
    connection::{Bytes, WsFrame},
};
use http::{HeaderMap, Method, StatusCode};
use std::sync::Arc;
use support::*;

fn key(operation: Operation) -> OperationKey {
    OperationKey {
        operation,
        dialect: Dialect::OpenAi,
    }
}
fn wire(path: &str, query: Option<&str>) -> WireRequest<()> {
    WireRequest {
        method: Method::GET,
        path: path.into(),
        query: query.map(str::to_owned),
        headers: HeaderMap::new(),
        body: (),
    }
}
async fn create(h: &Harness) {
    h.script(vec![(
        StatusCode::CREATED,
        vec![
            ("location", "/v1/realtime/calls/rtc_bound"),
            ("content-type", "application/sdp"),
        ],
        vec![Bytes::from_static(b"v=0\r\n")],
    )]);
    let context = h.context_for("p", key(Operation::CreateRealtimeCall), "create", 1, None);
    let execution = h
        .core
        .create_realtime_call(context, request("offer"))
        .await
        .unwrap();
    let (response, completion) = execution.into_parts();
    read(response.body).await;
    completion.await.unwrap();
}
#[tokio::test]
async fn calls_pin_credentials_across_round_robin_and_live_dispatch() {
    let h = harness(full(), "round_robin").await;
    create(&h).await;
    for (path, query) in [
        ("/v1/realtime", Some("call_id=rtc_bound")),
        ("/v1/live/rtc_bound", None),
    ] {
        h.script_ws(vec![WsReply::Connected(vec![WsFrame::Text(
            "{\"type\":\"session.created\"}".into(),
        )])]);
        let context = h.context_for("p", key(Operation::ConnectRealtime), "join", 2, None);
        let execution = h
            .core
            .connect_realtime_path(context, wire(path, query))
            .await
            .unwrap();
        let (connection, completion) = execution.into_parts();
        let UpstreamConnection::Connected { mut socket, .. } = connection else {
            panic!("connected")
        };
        while socket.incoming.next().await.is_some() {}
        drop(socket);
        completion.await.unwrap();
    }
    let sent = h.client.seen.lines();
    assert_eq!(sent.len(), 3);
    assert!(
        sent.iter().all(|line| line.contains("auth=Bearer ka")),
        "{sent:?}"
    );
}
#[tokio::test]
async fn call_binding_never_widens_scope_model_or_credential_access() {
    let h = harness(full(), "round_robin").await;
    create(&h).await;
    let base = h.context_for("p", key(Operation::ConnectRealtime), "join", 2, None);
    for context in [
        RequestContext {
            scope: "other".into(),
            ..(*base).clone()
        },
        {
            let mut c = (*base).clone();
            c.target.credentials.retain(|c| c.id != "a");
            c
        },
        {
            let mut c = (*base).clone();
            c.target.upstream_model = Some("different".into());
            c
        },
    ] {
        let error = h
            .core
            .connect_realtime(
                Arc::new(context),
                wire("/v1/realtime", Some("call_id=rtc_bound")),
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            CoreError::Forbidden(_) | CoreError::InvalidTarget(_)
        ));
    }
    for query in [
        "call_id=rtc_unknown",
        "call_id=rtc_bound&model=other",
        "call_id=rtc_bound&call_id=another",
    ] {
        assert!(
            h.core
                .connect_realtime(base.clone(), wire("/v1/realtime", Some(query)))
                .await
                .is_err()
        );
    }
    assert_eq!(
        h.client.seen.lines().len(),
        1,
        "denied joins never dispatch"
    );
}
#[tokio::test]
async fn failed_sideband_never_falls_back_to_another_credential() {
    let h = harness(full(), "round_robin").await;
    create(&h).await;
    h.script_ws(vec![WsReply::Rejected(
        StatusCode::TOO_MANY_REQUESTS,
        "busy",
    )]);
    let context = h.context_for("p", key(Operation::ConnectRealtime), "join", 3, None);
    let result = h
        .core
        .connect_realtime(context, wire("/v1/live/rtc_bound", None))
        .await;
    if let Ok(execution) = result {
        let (connection, completion) = execution.into_parts();
        drop(connection);
        let _ = completion.await;
    }
    let sent = h.client.seen.lines();
    assert_eq!(sent.len(), 2);
    assert!(sent[1].contains("auth=Bearer ka"));
}

/// A realtime `response.done` event reporting `tokens` of input.
fn done(id: &str, tokens: u64) -> WsFrame {
    WsFrame::Text(
        serde_json::json!({"type": "response.done", "response": {"id": id,
            "usage": {"input_tokens": tokens, "output_tokens": 0}}})
        .to_string(),
    )
}
async fn metered_join(h: &Harness, id: &str) -> gproxy_core::UsageReport {
    let context = h.context_for("p", key(Operation::ConnectRealtime), id, 1, None);
    let execution = h
        .core
        .connect_realtime(context, wire("/v1/realtime", Some("call_id=rtc_bound")))
        .await
        .unwrap();
    let (connection, completion) = execution.into_parts();
    let UpstreamConnection::Connected { mut socket, .. } = connection else {
        panic!("connected")
    };
    while socket.incoming.next().await.is_some() {}
    drop(socket);
    completion.await.unwrap()
}
#[tokio::test]
async fn replay_and_concurrent_connections_settle_each_response_once() {
    let h = harness(full(), "round_robin").await;
    create(&h).await;
    h.script_ws(vec![
        WsReply::Connected(vec![done("r1", 100)]),
        WsReply::Connected(vec![done("r1", 100), done("r2", 20)]),
        WsReply::Connected(vec![done("r1", 100), done("r2", 20)]),
    ]);
    let (a, b) = tokio::join!(metered_join(&h, "a"), metered_join(&h, "b"));
    let tokens: u64 = a
        .exchanges
        .iter()
        .chain(&b.exchanges)
        .filter_map(|e| e.usage.tokens.input_tokens)
        .sum();
    assert_eq!(tokens, 120);
    let replay = metered_join(&h, "replay").await;
    assert!(replay.exchanges.is_empty());
    assert!(replay.cost.is_none());
    let reports = h.observer.reports.lock().unwrap();
    let billed: u64 = reports
        .iter()
        .flat_map(|r| &r.exchanges)
        .filter_map(|e| e.usage.tokens.input_tokens)
        .sum();
    assert_eq!(
        billed, 120,
        "host observer receives only deduplicated usage"
    );
}
