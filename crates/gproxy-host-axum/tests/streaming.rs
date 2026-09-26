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
        post("/v1/messages", json!({"model": "test/m1", "stream": true})),
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
        .script(vec![Reply::Stream(StatusCode::OK, receiver)]);

    let response = host.raw(call()).await;
    let mut body = response.into_body().into_data_stream();
    chunks.send(Bytes::from_static(b"partial")).unwrap();
    assert_eq!(body.next().await.unwrap().unwrap(), "partial");

    assert!(
        rows(&host).await.is_empty(),
        "a request that is still streaming has not been settled"
    );

    drop(chunks);
    while body.next().await.is_some() {}

    let rows = rows(&host).await;
    assert_eq!(rows.len(), 1, "the settlement ran when the stream ended");
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
