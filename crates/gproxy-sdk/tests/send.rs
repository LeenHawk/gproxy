#![cfg(not(target_arch = "wasm32"))]

//! Every future the sdk hands a host must be `Send`.
//!
//! `CallBuilder::send` and `ConnectBuilder::send` wrap the engine's own
//! execution futures, so they inherit whatever bound those have. A host spawns
//! one task per request — an axum handler's future is spawned onto a
//! multi-threaded runtime and therefore has to be `Send` — while the rest of
//! this crate's tests run under `#[tokio::test]`'s current-thread runtime,
//! which imposes no such bound and so cannot notice when it is lost.
//!
//! The bound is easy to lose by accident: `ByteStream` is `Send` but
//! deliberately not `Sync`, so a shared reference to a `WireRequest<HttpBody>`
//! held across a single await makes the whole future non-`Send`.
//!
//! wasm32 puts no `Send` bound on `ByteStream` at all, so this file is
//! native-only rather than weakened to suit it.

mod support;

use std::sync::Arc;

use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest,
    connection::{Bytes, HeaderMap, Method},
};
use http::StatusCode;
use serde_json::json;
use support::seed::{self, Handle, Reply, SeedClient, WsReply};

/// Fails to compile when `T` is not `Send`. Applied to a future before it is
/// awaited, this is exactly the bound `tokio::spawn` would ask for.
fn assert_send<T: Send>(_: &T) {}

fn generate() -> OperationKey {
    OperationKey {
        operation: Operation::GenerateContent,
        dialect: Dialect::OpenAi,
    }
}

fn request() -> WireRequest<HttpBody> {
    WireRequest {
        method: Method::POST,
        path: "/v1/responses".into(),
        query: None,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from_static(br#"{"model":"solo"}"#)),
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

/// One provider serving `m1` behind a route exposed as `solo`.
async fn solo(channel: &str) -> (Handle, Arc<SeedClient>) {
    let (gproxy, client, _) = seed::handle().await;
    seed::provider(&gproxy, "p1", channel, &["m1"]).await;
    seed::credential(&gproxy, "c-p1", "p1").await;
    seed::route(
        &gproxy,
        "r",
        "solo",
        gproxy_store::entity::routing::route::RouteStrategy::Failover,
        4,
        &[("m-p1", "p1", "m1", 0, 100)],
    )
    .await;
    seed::publish(&gproxy).await;
    (gproxy, client)
}

#[tokio::test]
async fn the_call_builder_hands_out_a_send_future() {
    let (gproxy, client) = solo("test").await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);

    let future = gproxy.call(generate(), request()).scope("user:u1").send();
    assert_send(&future);
    let execution = future.await.unwrap();
    assert_eq!(execution.response().status, StatusCode::OK);
    support::read(execution.into_parts().0.body).await;
}

#[tokio::test]
async fn the_connect_builder_hands_out_a_send_future() {
    let (gproxy, client) = solo("alt").await;
    client.script_ws(vec![WsReply::Connected]);

    let future = gproxy
        .connect(
            OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: Dialect::OpenAiResponsesWebSocket,
            },
            socket_request(),
        )
        .scope("user:u1")
        .model("solo")
        .send();
    assert_send(&future);
    let _ = future.await;
}

/// The shape a real host uses: one spawned task per request on a
/// multi-threaded runtime. `tokio::spawn` demands `Send + 'static`, which is
/// what makes this the assertion a regression cannot slip past.
#[tokio::test(flavor = "multi_thread")]
async fn a_host_can_spawn_a_call() {
    let (gproxy, client) = solo("test").await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let gproxy = Arc::new(gproxy);

    let engine = gproxy.clone();
    let execution = tokio::spawn(async move {
        engine
            .call(generate(), request())
            .scope("user:u1")
            .send()
            .await
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(execution.response().status, StatusCode::OK);
    support::read(execution.into_parts().0.body).await;
}
