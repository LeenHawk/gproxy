#![cfg(not(target_arch = "wasm32"))]
//! What a client that goes away costs, observed from outside.
//!
//! A client that hangs up has not asked for anything: no header says so, no
//! callback fires. The gateway learns it by having something dropped out from
//! under it — the handler future before a head was written, the response body
//! mid-stream, the client half of a socket — and the whole of this crate's fix
//! is to turn that drop into the request's cancellation token, which core
//! already honours.
//!
//! There is no way to fake that. `tower::ServiceExt::oneshot` never involves a
//! socket, and every HTTP client in the tree drains a body before it hands it
//! over, so these tests bind a loopback port (the harness the websocket suite
//! already uses) and type the request out over a raw `TcpStream`. Closing the
//! connection is then `drop`, which is exactly what a client does.
//!
//! Each test asserts the same three things, because a cancellation that is only
//! two of them is still a bug:
//!
//! 1. **The upstream was not read any further.** A counter in the scripted
//!    upstream; the tokens nobody will read are the money.
//! 2. **The physical capture is honest.** Its state is `cancelled`, not
//!    `completed` — core decides that by reading the token, which is why a token
//!    fired after a *finished* response would be just as wrong.
//! 3. **The concurrency lease came back.** Asserted the way P9 and P10 assert
//!    it: with one slot configured, the next request is admitted.
//!
//! # What each test is worth
//!
//! Measured against the code before the token existed, only
//! [`a_socket_whose_client_vanishes_cancels_the_session`] fails, and that is a
//! structural fact rather than an accident: an upgraded socket is pumped by a
//! **task hyper spawned**, so the handler future is long gone and there is
//! nothing left for a disconnect to drop. The session used to settle as
//! `completed` — a realtime hour nobody was listening to, metered as delivered.
//!
//! The two HTTP tests passed before the fix as well, because in one process
//! every layer under the handler is a future the handler owns, so hyper dropping
//! it *is* a cancellation and core's drop guards settle it. They are kept
//! because that is a property worth pinning rather than inferring, and because
//! the token is what makes the HTTP path take core's designed cancellation route
//! instead of its drop fallback — the one that has to spawn and that no host
//! can order. [`a_response_read_to_its_end_is_not_cancelled`] is the guard for
//! the other half of the rule.

mod support;

use std::{sync::Arc, time::Duration};

use futures_util::SinkExt as _;
use gproxy_protocol::connection::{Bytes, WsFrame};
use gproxy_seaorm::FixedDecimal;
use gproxy_store::entity::{limits::rate_limit, usage::upstream_record as capture_record};
use http::StatusCode;
use sea_orm::{ActiveEnum, EntityTrait, Set};
use serde_json::json;
use support::{Bound, Host, Probe, Reply, keyed, post};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpStream,
};
use tokio_tungstenite::tungstenite::{
    Message, client::IntoClientRequest, protocol::CloseFrame, protocol::frame::coding::CloseCode,
};

/// One provider, one permitted key, and at most one request in flight.
///
/// The concurrency limit of one is not decoration: it is the only way to see a
/// lease from outside, so it is how assertion 3 is made in every test here.
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
    handle
        .manage()
        .settings()
        .update(gproxy_sdk::dto::SettingsPatch {
            logging: Some(gproxy_sdk::dto::LoggingSettingsPatch {
                enable_upstream_log: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    host.publish().await;
    host
}

// ----------------------------------------------------------------- the http --

#[tokio::test(flavor = "multi_thread")]
async fn a_client_that_hangs_up_mid_stream_cancels_the_upstream_call() {
    let host = instance().await;
    let (chunks, receiver) = tokio::sync::mpsc::unbounded_channel();
    let probe = Arc::new(Probe::default());
    host.client
        .script(vec![Reply::Probed(StatusCode::OK, receiver, probe.clone())]);
    let bound = host.bind().await;

    // One chunk written, the upstream still thinking: the state every long
    // answer is in for most of its life.
    let mut client = call(&bound).await;
    chunks.send(Bytes::from_static(b"event: one\n\n")).unwrap();
    read_until(&mut client, "event: one")
        .await
        .expect("the first chunk reaches the client");
    assert_eq!(probe.delivered(), 1);

    // **The disconnect.** Nothing is said and nothing is closed politely; the
    // socket goes away, which is all a client ever does.
    drop(client);

    let row = settled(&host).await;
    assert_eq!(
        state(&row),
        "cancelled",
        "an abandoned response is metered as abandoned, not as delivered"
    );
    assert_eq!(
        probe.delivered(),
        1,
        "the upstream was read no further than the one chunk the client read"
    );
    assert!(
        probe.dropped(),
        "the upstream body was let go rather than drained to its end"
    );
    // The upstream still has more to say, and there is no longer anything on
    // the other end of it to say it to.
    assert!(
        chunks.send(Bytes::from_static(b"event: two\n\n")).is_err(),
        "the gateway is still holding the upstream body open"
    );
    assert_admitted_again(&host).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_that_hangs_up_before_the_head_cancels_the_upstream_call() {
    let host = instance().await;
    let probe = Arc::new(Probe::default());
    host.client.script(vec![Reply::Stalls(probe.clone())]);
    let bound = host.bind().await;

    // In flight, and staying that way: the upstream has the request and is
    // answering nothing, so there is no head and no body to drop — only the
    // handler future.
    let client = call(&bound).await;
    for _ in 0..100 {
        if !host.client.urls().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        host.client.urls().len(),
        1,
        "the call reached the upstream before the client left"
    );
    assert!(!probe.dropped());

    drop(client);

    let row = settled(&host).await;
    assert_eq!(state(&row), "cancelled");
    assert!(
        probe.dropped(),
        "the gateway gave the upstream call up rather than waiting out an \
         answer nobody would read"
    );
    assert_admitted_again(&host).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_response_read_to_its_end_is_not_cancelled() {
    // The regression guard for the disarm. The token is owned by the value the
    // client's departure drops, and a response that ends normally drops that
    // value too — so if the disarm were missing, *every* finished request would
    // settle as cancelled and core would be told to abort the settlement the
    // body is waiting on.
    let host = instance().await;
    let (chunks, receiver) = tokio::sync::mpsc::unbounded_channel();
    let probe = Arc::new(Probe::default());
    host.client
        .script(vec![Reply::Probed(StatusCode::OK, receiver, probe.clone())]);
    let bound = host.bind().await;

    let mut client = call(&bound).await;
    chunks.send(Bytes::from_static(b"event: one\n\n")).unwrap();
    read_until(&mut client, "event: one").await.unwrap();
    chunks.send(Bytes::from_static(b"event: two\n\n")).unwrap();
    // The upstream is done, so the body ends on its own terms.
    drop(chunks);
    read_until(&mut client, "event: two")
        .await
        .expect("the last chunk reaches the client");
    // `0\r\n\r\n`: the terminal chunk, which is hyper saying the body is over.
    read_until(&mut client, "0\r\n\r\n")
        .await
        .expect("the body ends");

    let row = settled(&host).await;
    assert_eq!(
        state(&row),
        "completed",
        "a response that ran out is finished, not abandoned"
    );
    assert_eq!(probe.delivered(), 2);

    // And it stays finished when the client then goes away, which is what every
    // client does the moment it has what it asked for.
    drop(client);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(state(&settled(&host).await), "completed");
    assert_admitted_again(&host).await;
}

// ------------------------------------------------------------ the websocket --

#[tokio::test(flavor = "multi_thread")]
async fn a_socket_whose_client_vanishes_cancels_the_session() {
    let host = instance().await;
    let upstream = host.client.accept_socket();
    let bound = host.bind().await;
    let mut client = dial(&bound).await;

    client.send(Message::Text("hello".into())).await.unwrap();
    assert_eq!(upstream.next().await, Some(WsFrame::Text("hello".into())));

    // No close frame: the connection is dropped, which reaches the pump as a
    // failed client socket rather than as an end of stream.
    drop(client);

    assert!(
        matches!(upstream.next().await, Some(WsFrame::Close(_))),
        "the upstream is told the session is over"
    );
    let row = settled(&host).await;
    assert_eq!(
        state(&row),
        "cancelled",
        "a realtime session nobody is listening to is not a completed one"
    );
    assert_admitted_again(&host).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_socket_closed_cleanly_still_settles_as_completed() {
    // The other half of the rule, and the thing the cancel must not break: two
    // peers that finished a session finished it.
    let host = instance().await;
    let upstream = host.client.accept_socket();
    let bound = host.bind().await;
    let mut client = dial(&bound).await;

    client.send(Message::Text("hello".into())).await.unwrap();
    assert_eq!(upstream.next().await, Some(WsFrame::Text("hello".into())));
    client
        .send(Message::Close(Some(CloseFrame {
            code: CloseCode::Normal,
            reason: "done".into(),
        })))
        .await
        .unwrap();

    assert!(matches!(upstream.next().await, Some(WsFrame::Close(_))));
    let row = settled(&host).await;
    assert_eq!(state(&row), "completed");
    assert_admitted_again(&host).await;
}

// ----------------------------------------------------------------- the wire --

/// One `POST /v1/messages`, typed out over a raw connection.
///
/// By hand, because the disconnect *is* the test: a client that buffers the
/// response has already read everything before the test can hang up, and
/// `oneshot` never opens a socket to hang up on.
async fn call(bound: &Bound) -> TcpStream {
    let body = json!({"model": "test/m1", "stream": true}).to_string();
    let request = format!(
        "POST /v1/messages HTTP/1.1\r\n\
         host: {host}\r\n\
         authorization: Bearer k-alice\r\n\
         content-type: application/json\r\n\
         content-length: {length}\r\n\
         \r\n\
         {body}",
        host = support::HOST,
        length = body.len(),
    );
    let mut stream = TcpStream::connect(bound.address).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();
    stream
}

/// Read from the connection until `needle` has arrived. `None` is a failure of
/// the test's premise, not a state a caller should tolerate.
async fn read_until(stream: &mut TcpStream, needle: &str) -> Option<String> {
    let mut seen = String::new();
    let mut buffer = [0_u8; 4096];
    loop {
        let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buffer))
            .await
            .ok()?
            .ok()?;
        if read == 0 {
            return None;
        }
        seen.push_str(&String::from_utf8_lossy(&buffer[..read]));
        if seen.contains(needle) {
            return Some(seen);
        }
    }
}

/// The client half of one realtime handshake.
async fn dial(bound: &Bound) -> Socket {
    let mut request = bound.ws("/v1/realtime").into_client_request().unwrap();
    request
        .headers_mut()
        .insert("authorization", "Bearer k-alice".parse().unwrap());
    let (socket, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    socket
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

// ------------------------------------------------------------- assertions --

/// The physical capture, once the exchange has closed. It happens in a task of its
/// own — the body's tail, or the pump's — so there is nothing to await from here
/// but the row it writes.
async fn settled(host: &Host) -> capture_record::Model {
    for _ in 0..150 {
        let mut rows = host
            .app
            .gproxy()
            .store()
            .upstream_records()
            .query(capture_record::Entity::find())
            .await
            .unwrap();
        if rows.first().is_some_and(|row| row.ended_at_ms.is_some()) {
            assert_eq!(rows.len(), 1, "one physical call is one capture row");
            return rows.remove(0);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the request was never settled");
}

/// How core closed the physical exchange, including calls with no usage.
fn state(row: &capture_record::Model) -> String {
    row.state.to_value()
}

/// The lease is back. With one slot configured, the only proof is that another
/// request gets in — the same assertion P9 and P10 make.
async fn assert_admitted_again(host: &Host) {
    host.client
        .push(Reply::Http(StatusCode::OK, json!({"ok": true})));
    for attempt in 0..100 {
        let answer = host
            .send(keyed(
                post("/v1/messages", json!({"model": "test/m1"})),
                "k-alice",
            ))
            .await;
        if answer.status == StatusCode::OK {
            return;
        }
        assert_eq!(
            answer.status,
            StatusCode::TOO_MANY_REQUESTS,
            "{}",
            answer.text()
        );
        // The permit is handed back by a release the settlement awaits, so a
        // moment of contention is expected; never getting in is the bug.
        assert!(attempt < 99, "the concurrency permit was never released");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
