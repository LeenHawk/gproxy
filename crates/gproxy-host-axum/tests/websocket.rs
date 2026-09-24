#![cfg(not(target_arch = "wasm32"))]
//! Upgrades, the duplex pump and what a socket holds open.
//!
//! Two kinds of test, for two reasons:
//!
//! - **Refusals** are HTTP answers, so they go through `tower::ServiceExt::
//!   oneshot` like the rest of the suite. That is also the assertion: a
//!   refusal that reaches the client as an HTTP status is a refusal that never
//!   upgraded anything.
//! - **A round trip** needs a real connection. `oneshot` never puts hyper's
//!   `OnUpgrade` extension on the request — an upgrade is made of it — so the
//!   socket tests bind a loopback port and speak the protocol with
//!   `tokio-tungstenite`.
//!
//! The upstream is scripted either way: `ScriptClient::connect` answers a
//! queued [`WsReply`], and a connected one hands the test both ends of the
//! socket the gateway is pumping.

mod support;

use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use gproxy_protocol::connection::{Bytes, WsClose, WsFrame};
use gproxy_seaorm::FixedDecimal;
use gproxy_store::entity::{
    limits::rate_limit,
    usage::{capture_event, capture_record, usage_record},
};
use http::StatusCode;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde_json::json;
use support::{Bound, Host, WsReply, get, keyed, with};
use tokio_tungstenite::tungstenite::{
    Message,
    client::IntoClientRequest,
    protocol::{CloseFrame, frame::coding::CloseCode},
};

/// One provider on the scripted channel, one permitted key, and — in the
/// tests that ask for it — at most one request in flight.
async fn instance(concurrency: Option<i64>) -> Host {
    let host = Host::new().await;
    let handle = host.handle();
    support::person(&handle, "alice", "user").await;
    support::api_key(&handle, "k-alice", "alice", None, None).await;
    support::provider(&handle, "p1", &["m1"]).await;
    support::allow(&handle, "perm", "alice", None).await;
    support::credential(&handle, "c1", "p1", None, None, None).await;
    if let Some(limit) = concurrency {
        handle
            .store()
            .rate_limits()
            .create_many(vec![rate_limit::ActiveModel {
                id: Set("one-at-a-time".into()),
                user_id: Set(Some("alice".into())),
                api_key_id: Set(None),
                metric: Set("concurrency".into()),
                limit_value: Set(FixedDecimal::from_atoms(limit * FixedDecimal::FACTOR)),
                period_seconds: Set(60),
                model_pattern: Set(Some("*".into())),
                enabled: Set(true),
            }])
            .await
            .unwrap();
    }
    host.publish().await;
    host
}

/// The client half of a handshake, with the key a caller presents.
async fn dial(
    bound: &Bound,
    path: &str,
    key: Option<&str>,
) -> tokio_tungstenite::tungstenite::Result<Socket> {
    let mut request = bound.ws(path).into_client_request().unwrap();
    if let Some(key) = key {
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {key}").parse().unwrap());
    }
    let (socket, _) = tokio_tungstenite::connect_async(request).await?;
    Ok(socket)
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// The next message the client receives, or `None` if nothing arrives.
async fn recv(socket: &mut Socket) -> Option<Message> {
    tokio::time::timeout(Duration::from_secs(5), socket.next())
        .await
        .ok()
        .flatten()
        .map(|message| message.expect("client socket"))
}

// ------------------------------------------------------- refused over HTTP --

#[tokio::test(flavor = "multi_thread")]
async fn an_unauthenticated_upgrade_is_refused_before_it_upgrades_anything() {
    let host = instance(None).await;
    // A complete, well-formed handshake. The only thing missing is the key.
    let request = with(
        with(
            with(get("/v1/realtime"), "connection", "upgrade"),
            "upgrade",
            "websocket",
        ),
        "sec-websocket-version",
        "13",
    );
    let request = with(request, "sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==");

    let refused = host.send(request).await;
    assert_eq!(
        refused.status,
        StatusCode::UNAUTHORIZED,
        "an anonymous caller is turned away over HTTP, where a status can be read: {}",
        refused.text()
    );
    assert_eq!(refused.json()["error"]["code"], "unauthorized");
    assert!(
        host.client.urls().is_empty(),
        "nothing was sent upstream for a handshake that was never admitted"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_plain_get_to_a_handshake_surface_is_told_to_upgrade() {
    let host = instance(None).await;
    let refused = host.send(keyed(get("/v1/realtime"), "k-alice")).await;
    assert_eq!(refused.status, StatusCode::UPGRADE_REQUIRED);
    assert_eq!(
        refused.header("upgrade"),
        Some("websocket"),
        "the refusal says what to upgrade to"
    );
    assert!(host.client.urls().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_caller_with_no_permission_is_refused_over_http_too() {
    let host = Host::new().await;
    let handle = host.handle();
    support::person(&handle, "bob", "user").await;
    support::api_key(&handle, "k-bob", "bob", None, None).await;
    support::provider(&handle, "p1", &["m1"]).await;
    support::credential(&handle, "c1", "p1", None, None, None).await;
    host.publish().await;
    let bound = host.bind().await;

    // No `allow` rule, so admission refuses before the engine is handed
    // anything — and the client sees it as a failed handshake carrying a 403,
    // not as a socket that opened and closed.
    let error = dial(&bound, "/v1/realtime", Some("k-bob"))
        .await
        .unwrap_err();
    assert!(
        matches!(&error, tokio_tungstenite::tungstenite::Error::Http(response)
            if response.status() == StatusCode::FORBIDDEN),
        "{error:?}"
    );
    assert!(host.client.urls().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_upstream_handshake_arrives_as_the_upstreams_own_response() {
    let host = instance(None).await;
    host.client.push_socket(WsReply::Rejected(
        StatusCode::TOO_MANY_REQUESTS,
        json!({"error": {"code": "insufficient_quota", "message": "buy more"}}),
    ));
    let bound = host.bind().await;

    let error = dial(&bound, "/v1/realtime", Some("k-alice"))
        .await
        .unwrap_err();
    let tokio_tungstenite::tungstenite::Error::Http(response) = error else {
        panic!("a refused handshake is an HTTP response, not {error:?}");
    };
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/json",
        "the upstream's own headers, not a generic envelope"
    );
    let body = response.body().as_ref().expect("the vendor's body is kept");
    let body: serde_json::Value = serde_json::from_slice(body).unwrap();
    assert_eq!(
        body["error"]["code"], "insufficient_quota",
        "the vendor's own code survives; a 502 would have thrown it away"
    );

    // A refusal is still a request: it settles and is metered.
    assert_eq!(usage_rows(&host).await.len(), 1);
}

// -------------------------------------------------------------- round trip --

#[tokio::test(flavor = "multi_thread")]
async fn an_accepted_upgrade_pumps_a_frame_each_way() {
    let host = instance(None).await;
    let upstream = host.client.accept_socket();
    let bound = host.bind().await;
    let mut client = dial(&bound, "/v1/realtime", Some("k-alice")).await.unwrap();

    // Client -> upstream.
    client
        .send(Message::Text(r#"{"type":"session.update"}"#.into()))
        .await
        .unwrap();
    assert_eq!(
        upstream.next().await,
        Some(WsFrame::Text(r#"{"type":"session.update"}"#.into()))
    );

    // Upstream -> client, text and binary both.
    upstream
        .send
        .send(Ok(WsFrame::Text(r#"{"type":"session.created"}"#.into())))
        .unwrap();
    assert_eq!(
        recv(&mut client).await,
        Some(Message::Text(r#"{"type":"session.created"}"#.into()))
    );
    upstream
        .send
        .send(Ok(WsFrame::Binary(Bytes::from_static(&[1, 2, 3]))))
        .unwrap();
    assert_eq!(
        recv(&mut client).await,
        Some(Message::Binary(Bytes::from_static(&[1, 2, 3])))
    );

    client
        .send(Message::Binary(Bytes::from_static(&[9])))
        .await
        .unwrap();
    assert_eq!(
        upstream.next().await,
        Some(WsFrame::Binary(Bytes::from_static(&[9])))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_close_from_the_client_closes_the_upstream_and_settles() {
    let host = instance(None).await;
    let upstream = host.client.accept_socket();
    let bound = host.bind().await;
    let mut client = dial(&bound, "/v1/realtime", Some("k-alice")).await.unwrap();

    client.send(Message::Text("hello".into())).await.unwrap();
    assert_eq!(upstream.next().await, Some(WsFrame::Text("hello".into())));

    client
        .send(Message::Close(Some(CloseFrame {
            code: CloseCode::Normal,
            reason: "done".into(),
        })))
        .await
        .unwrap();

    assert_eq!(
        upstream.next().await,
        Some(WsFrame::Close(Some(WsClose {
            code: 1000,
            reason: "done".into()
        }))),
        "the client's close, with its code and reason, reaches the upstream"
    );
    settled(&host).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_close_from_the_upstream_closes_the_client() {
    let host = instance(None).await;
    let upstream = host.client.accept_socket();
    let bound = host.bind().await;
    let mut client = dial(&bound, "/v1/realtime", Some("k-alice")).await.unwrap();

    upstream
        .send
        .send(Ok(WsFrame::Close(Some(WsClose {
            code: 1001,
            reason: "going away".into(),
        }))))
        .unwrap();

    let Some(Message::Close(Some(frame))) = recv(&mut client).await else {
        panic!("the upstream's close reaches the client");
    };
    assert_eq!(frame.code, CloseCode::Away);
    assert_eq!(frame.reason.as_str(), "going away");
    settled(&host).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_upstream_that_vanishes_closes_the_client_too() {
    let host = instance(None).await;
    let upstream = host.client.accept_socket();
    let bound = host.bind().await;
    let mut client = dial(&bound, "/v1/realtime", Some("k-alice")).await.unwrap();

    // No close frame at all: the upstream stream simply ends.
    drop(upstream.send);

    let Some(Message::Close(Some(frame))) = recv(&mut client).await else {
        panic!("a client is told the socket is over even when the upstream said nothing");
    };
    assert_eq!(frame.code, CloseCode::Normal);
    settled(&host).await;
}

// ------------------------------------------------------------- the leases --

#[tokio::test(flavor = "multi_thread")]
async fn a_socket_holds_its_concurrency_permit_until_it_closes() {
    // The same assertion style as the streamed-body test: with one slot, a
    // second request has to be refused *while the socket is open* and
    // admitted once it is not. Anything weaker would pass even if the lease
    // were dropped the instant the `101` was written.
    let host = instance(Some(1)).await;
    let upstream = host.client.accept_socket();
    let bound = host.bind().await;
    let mut client = dial(&bound, "/v1/realtime", Some("k-alice")).await.unwrap();

    // Open, and doing nothing in particular — which is what a realtime
    // session does most of the time.
    host.client
        .push(support::Reply::Http(StatusCode::OK, json!({"ok": true})));
    let refused = host
        .send(keyed(
            support::post("/v1/messages", json!({"model": "test/m1"})),
            "k-alice",
        ))
        .await;
    assert_eq!(
        refused.status,
        StatusCode::TOO_MANY_REQUESTS,
        "the socket's permit was released before it closed: {}",
        refused.text()
    );

    client.close(None).await.unwrap();
    assert!(matches!(
        upstream.next().await,
        Some(WsFrame::Close(_)) | None
    ));
    settled(&host).await;

    // Freed. The scripted HTTP reply from above is still queued.
    let after = host
        .send(keyed(
            support::post("/v1/messages", json!({"model": "test/m1"})),
            "k-alice",
        ))
        .await;
    assert_eq!(after.status, StatusCode::OK, "{}", after.text());
}

// ---------------------------------------------------------------- capture --

#[tokio::test(flavor = "multi_thread")]
async fn a_socket_is_captured_as_one_connection_with_its_frames() {
    let host = instance(None).await;
    // Bodies are opt-in on both sides; frames are the socket's body.
    settings(
        &host,
        gproxy_sdk::dto::SettingsPatch {
            instance: None,
            logging: Some(gproxy_sdk::dto::LoggingSettingsPatch {
                enable_downstream_log_body: Some(true),
                ..Default::default()
            }),
        },
    )
    .await;

    let upstream = host.client.accept_socket();
    let bound = host.bind().await;
    let mut client = dial(&bound, "/v1/realtime", Some("k-alice")).await.unwrap();

    client.send(Message::Text("up".into())).await.unwrap();
    assert_eq!(upstream.next().await, Some(WsFrame::Text("up".into())));
    upstream
        .send
        .send(Ok(WsFrame::Text("down".into())))
        .unwrap();
    assert_eq!(recv(&mut client).await, Some(Message::Text("down".into())));
    client
        .send(Message::Close(Some(CloseFrame {
            code: CloseCode::Normal,
            reason: "bye".into(),
        })))
        .await
        .unwrap();
    settled(&host).await;

    let record = downstream_record(&host).await;
    assert_eq!(
        record.kind,
        capture_record::CaptureKind::WsConnection,
        "a `101` makes the record a connection, not an HTTP exchange"
    );
    assert_eq!(record.response_status, Some(101));
    assert_eq!(
        record.request_framing,
        capture_record::BodyFraming::WebSocket
    );
    assert_eq!(
        record.response_framing,
        capture_record::BodyFraming::WebSocket
    );

    let events = events(&host, &record.id).await;
    let seen: Vec<_> = events
        .iter()
        .map(|event| {
            (
                event.direction,
                event.kind,
                String::from_utf8_lossy(&event.payload).into_owned(),
            )
        })
        .collect();
    use capture_event::{CaptureDirection as D, CaptureEventKind as K};
    assert_eq!(
        seen,
        vec![
            (D::Request, K::WsText, "up".to_owned()),
            (D::Response, K::WsText, "down".to_owned()),
            (
                D::Request,
                K::WsClose,
                r#"{"code":1000,"reason":"bye"}"#.to_owned()
            ),
        ],
        "one ordered list across both directions, the close frame included"
    );
    assert!(
        events.iter().all(|event| event.turn_id.is_none()),
        "no turn is invented: the host forwards realtime frames as opaque passthrough"
    );
    // No `WsTurn` record either, for the same reason.
    assert!(
        records(&host)
            .await
            .iter()
            .all(|row| row.kind != capture_record::CaptureKind::WsTurn)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_relayed_close_is_recorded_once_in_the_direction_it_arrived() {
    // The close the upstream sent is forwarded to the client and echoed back
    // upstream, so three sends happen for one logical close. It must appear in
    // the log once, as a response — two rows would read as two closes.
    let host = instance(None).await;
    settings(
        &host,
        gproxy_sdk::dto::SettingsPatch {
            instance: None,
            logging: Some(gproxy_sdk::dto::LoggingSettingsPatch {
                enable_downstream_log_body: Some(true),
                ..Default::default()
            }),
        },
    )
    .await;

    let upstream = host.client.accept_socket();
    let bound = host.bind().await;
    let mut client = dial(&bound, "/v1/realtime", Some("k-alice")).await.unwrap();
    upstream
        .send
        .send(Ok(WsFrame::Close(Some(WsClose {
            code: 1001,
            reason: "going away".into(),
        }))))
        .unwrap();
    assert!(matches!(
        recv(&mut client).await,
        Some(Message::Close(Some(_)))
    ));
    settled(&host).await;

    let record = downstream_record(&host).await;
    let events = events(&host, &record.id).await;
    assert_eq!(
        events
            .iter()
            .map(|event| (
                event.direction,
                event.kind,
                String::from_utf8_lossy(&event.payload).into_owned()
            ))
            .collect::<Vec<_>>(),
        vec![(
            capture_event::CaptureDirection::Response,
            capture_event::CaptureEventKind::WsClose,
            r#"{"code":1001,"reason":"going away"}"#.to_owned()
        )]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_socket_records_no_frames_with_the_body_switch_off() {
    // The default. The connection is still recorded — the operator wants to
    // know a socket happened — but nothing the caller said is copied.
    let host = instance(None).await;
    let upstream = host.client.accept_socket();
    let bound = host.bind().await;
    let mut client = dial(&bound, "/v1/realtime", Some("k-alice")).await.unwrap();
    client.send(Message::Text("secret".into())).await.unwrap();
    assert_eq!(upstream.next().await, Some(WsFrame::Text("secret".into())));
    client.close(None).await.unwrap();
    settled(&host).await;

    let record = downstream_record(&host).await;
    assert_eq!(record.kind, capture_record::CaptureKind::WsConnection);
    assert!(events(&host, &record.id).await.is_empty());
    assert_eq!(
        record.request_framing,
        capture_record::BodyFraming::Buffered,
        "nothing was event-backed, so the framing stays what an unrecorded body is"
    );
}

// --------------------------------------------------------- service sockets --

#[tokio::test(flavor = "multi_thread")]
async fn a_vendor_service_socket_upgrades_under_a_credential_view() {
    let host = instance(None).await;
    // Only an administrator of the credential may name it, and the scripted
    // channel — like Codex's remote control — refuses anything else.
    support::person(&host.handle(), "root", "admin").await;
    support::api_key(&host.handle(), "k-root", "root", None, None).await;
    host.publish().await;

    let upstream = host.client.accept_socket();
    let bound = host.bind().await;

    let mut request = bound
        .ws("/p1/backend-api/wham/remote/control/server")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("authorization", "Bearer k-root".parse().unwrap());
    request
        .headers_mut()
        .insert("x-gproxy-view", "credential:c1".parse().unwrap());
    let (mut client, _) = tokio_tungstenite::connect_async(request).await.unwrap();

    client.send(Message::Text("ping".into())).await.unwrap();
    assert_eq!(upstream.next().await, Some(WsFrame::Text("ping".into())));
    upstream
        .send
        .send(Ok(WsFrame::Text("pong".into())))
        .unwrap();
    assert_eq!(recv(&mut client).await, Some(Message::Text("pong".into())));

    client.close(None).await.unwrap();
    // A service takes no lease and settles nothing, which is `gproxy-app`'s
    // rule: services run outside the observation funnel.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(usage_rows(&host).await.is_empty());
    let captured = records(&host).await;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].side, capture_record::CaptureSide::Downstream);
    assert_eq!(captured[0].operation.as_deref(), Some("service"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_synthesized_view_on_a_service_socket_is_refused_by_the_channel() {
    let host = instance(None).await;
    let bound = host.bind().await;

    // The default view is `caller`, which is synthesized.
    let error = dial(
        &bound,
        "/p1/backend-api/wham/remote/control/server",
        Some("k-alice"),
    )
    .await
    .unwrap_err();
    let tokio_tungstenite::tungstenite::Error::Http(response) = error else {
        panic!("{error:?}");
    };
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = response.body().as_ref().expect("the channel's own body");
    assert!(
        String::from_utf8_lossy(body).contains("credential view"),
        "the channel's refusal is relayed, not replaced"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_service_socket_reached_without_a_handshake_is_told_to_upgrade() {
    let host = instance(None).await;
    let refused = host
        .send(keyed(
            get("/p1/backend-api/wham/remote/control/server"),
            "k-alice",
        ))
        .await;
    assert_eq!(refused.status, StatusCode::UPGRADE_REQUIRED);
    assert_eq!(refused.header("upgrade"), Some("websocket"));
}

// ------------------------------------------------------------------ helpers --

/// Write one settings patch and republish, so the request that follows is
/// decided against it.
async fn settings(host: &Host, patch: gproxy_sdk::dto::SettingsPatch) {
    host.handle()
        .manage()
        .settings()
        .update(patch)
        .await
        .unwrap();
    host.publish().await;
}

/// Wait for the settlement the socket's end runs. It happens in the pump's own
/// task, so there is nothing to await from here but the row it writes.
async fn settled(host: &Host) {
    for _ in 0..100 {
        if !usage_rows(host).await.is_empty() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the socket ended without settling");
}

async fn usage_rows(host: &Host) -> Vec<usage_record::Model> {
    host.app
        .gproxy()
        .store()
        .usage_records()
        .query(usage_record::Entity::find())
        .await
        .unwrap()
}

async fn records(host: &Host) -> Vec<capture_record::Model> {
    host.app
        .gproxy()
        .store()
        .capture_records()
        .query(
            capture_record::Entity::find()
                .filter(capture_record::Column::Side.eq(capture_record::CaptureSide::Downstream)),
        )
        .await
        .unwrap()
}

async fn downstream_record(host: &Host) -> capture_record::Model {
    let mut rows = records(host).await;
    assert_eq!(rows.len(), 1, "one socket is one downstream record");
    rows.remove(0)
}

async fn events(host: &Host, capture_id: &str) -> Vec<capture_event::Model> {
    host.app
        .gproxy()
        .store()
        .capture_events()
        .query(
            capture_event::Entity::find()
                .filter(capture_event::Column::CaptureId.eq(capture_id))
                .order_by_asc(capture_event::Column::Sequence),
        )
        .await
        .unwrap()
}
