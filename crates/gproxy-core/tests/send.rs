#![cfg(not(target_arch = "wasm32"))]

//! Every future the engine hands a host must be `Send`.
//!
//! A host spawns one task per request: an axum handler's future is spawned
//! onto a multi-threaded runtime and therefore has to be `Send`. The rest of
//! this crate's tests cannot notice when that stops being true, because
//! `#[tokio::test]` builds a current-thread runtime, which imposes no `Send`
//! bound on the future it drives.
//!
//! The bound is easy to lose by accident. `ByteStream` is `Send` but
//! deliberately not `Sync`, so `HttpBody` is not `Sync`, so a shared reference
//! to a `WireRequest<HttpBody>` (or to anything containing one) is not `Send`.
//! Holding such a reference across a single await makes the whole future
//! non-`Send`, and the only symptom is that a host can no longer spawn it.
//!
//! On wasm32 `ByteStream` carries no `Send` bound at all and the host is
//! single-threaded, so the whole file is native-only rather than weakened.

mod support;

use gproxy_channel::channel::CallerRole;
use gproxy_core::{ServiceRequest, ServiceView};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, connection::Bytes};
use http::{HeaderMap, Method, StatusCode};
use serde_json::json;
use std::sync::{Arc, atomic::Ordering};
use support::*;

/// Fails to compile when `T` is not `Send`. Applied to a future before it is
/// awaited, this is exactly the bound `tokio::spawn` would ask for.
fn assert_send<T: Send>(_: &T) {}

fn key(operation: Operation) -> OperationKey {
    OperationKey {
        operation,
        dialect: Dialect::OpenAi,
    }
}

fn socket_request() -> WireRequest<()> {
    WireRequest {
        method: Method::GET,
        path: "/v1/realtime".into(),
        query: None,
        headers: HeaderMap::new(),
        body: (),
    }
}

fn service_request<B>(h: &Harness, body: B) -> ServiceRequest<B> {
    ServiceRequest {
        scope: "tenant".into(),
        user_id: None,
        caller: CallerRole::Admin,
        view: ServiceView::Caller,
        target: h.target("p"),
        budgets: Vec::new(),
        request: WireRequest {
            method: Method::GET,
            path: "/api/oauth/profile".into(),
            query: None,
            headers: HeaderMap::new(),
            body,
        },
    }
}

#[tokio::test]
async fn the_generic_entry_points_hand_out_send_futures() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![json_reply(StatusCode::OK, json!({"ok": 1}))]);

    let future = h.core.send(h.context("send", 1, None), request("{}"));
    assert_send(&future);
    let execution = future.await.unwrap();
    read(execution.into_parts().0.body).await;

    h.script_ws(vec![WsReply::Connected(Vec::new())]);
    let context = h.context_for("p", key(Operation::ConnectRealtime), "connect", 1, None);
    let future = h.core.connect(context, socket_request());
    assert_send(&future);
    let _ = future.await;
}

#[tokio::test]
async fn the_named_operation_methods_hand_out_send_futures() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![json_reply(StatusCode::OK, json!({"ok": 1}))]);

    let future = h
        .core
        .stream_generate_content(h.context("stream", 1, None), request("{}"));
    assert_send(&future);
    read(future.await.unwrap().into_parts().0.body).await;

    h.script(vec![json_reply(StatusCode::OK, json!({"data": []}))]);
    let context = h.context_for("p", key(Operation::ListModels), "models", 1, None);
    let future = h.core.list_models(context, request("{}"));
    assert_send(&future);
    let _ = future.await;

    h.script_ws(vec![WsReply::Connected(Vec::new())]);
    let context = h.context_for("p", key(Operation::ConnectRealtime), "realtime", 1, None);
    let future = h.core.connect_realtime(context, socket_request());
    assert_send(&future);
    let _ = future.await;
}

#[tokio::test]
async fn the_service_entry_points_hand_out_send_futures() {
    let h = harness(full(), "round_robin").await;
    h.channel.expose_services.store(true, Ordering::Relaxed);
    h.script(vec![json_reply(StatusCode::OK, json!({"account": {}}))]);

    let call = service_request(&h, HttpBody::Bytes(Bytes::new()));
    let future = h.core.call_service(call);
    assert_send(&future);
    let _ = future.await;

    let connect = service_request(&h, ());
    let future = h.core.connect_service(connect);
    assert_send(&future);
    let _ = future.await;
}

/// The shape a real host uses: one spawned task per request on a
/// multi-threaded runtime. This is the assertion that would have caught the
/// regression, because `tokio::spawn` demands `Send + 'static`.
#[tokio::test(flavor = "multi_thread")]
async fn a_host_can_spawn_a_call() {
    let h = Arc::new(harness(full(), "round_robin").await);
    h.script(vec![json_reply(StatusCode::OK, json!({"ok": 1}))]);
    let engine = h.clone();
    let execution = tokio::spawn(async move {
        engine
            .core
            .send(engine.context("spawned", 1, None), request("{}"))
            .await
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(execution.response().status, StatusCode::OK);
    read(execution.into_parts().0.body).await;
}
