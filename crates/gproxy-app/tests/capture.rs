#![cfg(not(target_arch = "wasm32"))]
//! The downstream half of the request log: the row this crate writes, and the
//! direct associations from upstream attempts to their downstream request.
//!
//! Every assertion here is about a switch being read *before* work is done, so
//! "nothing was captured" is a row count and a byte search rather than an
//! absence of evidence.

mod support;

use gproxy_app::{
    App, CallOutcome, Caller, CallerKind, DataPlaneRequest,
    capture::{CaptureDirection, CaptureOutcome, CapturedFrame, DownstreamCapture},
};
use gproxy_core::UsageAttribution;
use gproxy_store::entity::{config::setting, usage::capture_record};
use http::StatusCode;
use sea_orm::{DatabaseConnection, EntityTrait, Set};
use serde_json::{Value, json};
use support::Reply;

type TestApp = App<DatabaseConnection>;

/// A body big enough that its presence anywhere in the database is
/// unmistakable, and longer than the former 64 KiB cutoff.
const SENTINEL: &str = "sentinel-payload-";

fn oversized() -> String {
    SENTINEL.repeat(128 * 1024 / SENTINEL.len() + 64)
}

/// One provider `p1` serving `m1`, one shared credential, `alice` holding a
/// key that may reach it. Both exchange logs are switched on, since the
/// schema leaves them off; the bodies stay at the default, not recorded.
async fn one_provider() -> (TestApp, std::sync::Arc<support::ScriptClient>) {
    let (app, client) = support::app().await;
    let handle = app.gproxy().clone();
    support::person(&handle, "alice", "user").await;
    support::api_key(&handle, "k-alice", "alice", None, None).await;
    support::allow(&handle, "p-alice", "alice", None).await;
    support::provider(&handle, "p1", &["m1"]).await;
    support::credential(&handle, "c-shared", "p1", None, None, None).await;
    log_exchanges(&app).await;
    support::publish(&app).await;
    (app, client)
}

/// The same with a second provider, so a 5xx from the first can be retried on
/// the second and the request ends up with two upstream attempts.
async fn two_providers() -> (TestApp, std::sync::Arc<support::ScriptClient>) {
    let (app, client) = support::app().await;
    let handle = app.gproxy().clone();
    support::person(&handle, "alice", "user").await;
    support::api_key(&handle, "k-alice", "alice", None, None).await;
    support::allow(&handle, "p-alice", "alice", None).await;
    support::provider(&handle, "p1", &["m1"]).await;
    support::provider(&handle, "p2", &["m1"]).await;
    support::credential(&handle, "c-1", "p1", None, None, None).await;
    support::credential(&handle, "c-2", "p2", None, None, None).await;
    log_exchanges(&app).await;
    support::publish(&app).await;
    (app, client)
}

/// Record both halves of every exchange, which an instance does not do until
/// it is asked to.
async fn log_exchanges(app: &TestApp) {
    app.gproxy()
        .store()
        .settings()
        .update(setting::ActiveModel {
            enable_downstream_log: Set(true),
            enable_upstream_log: Set(true),
            ..Default::default()
        })
        .await
        .unwrap();
}

/// Move the logging switches and republish both snapshots, the way a
/// management write would.
async fn logging(app: &TestApp, log: bool, body: bool, redact: bool) {
    app.gproxy()
        .store()
        .settings()
        .update(setting::ActiveModel {
            enable_downstream_log: Set(log),
            enable_downstream_log_body: Set(body),
            disable_log_redaction: Set(!redact),
            ..Default::default()
        })
        .await
        .unwrap();
    support::publish(app).await;
    let switches = app.data().observation;
    assert_eq!(switches.downstream_log, log);
    assert_eq!(switches.downstream_log_body, body);
    assert_eq!(switches.redact, redact);
}

/// A POST to `/v1/responses?stream=true&api_key=sk-query` carrying `body`,
/// with an `authorization` header the gateway itself consumed.
fn request(id: &str, body: Value) -> DataPlaneRequest {
    let http_request = http::Request::builder()
        .method(http::Method::POST)
        .uri("/v1/responses?stream=true&api_key=sk-query")
        .header("authorization", "Bearer sk-alice")
        .header("x-trace", "keep-me")
        .body(())
        .unwrap();
    let (parts, ()) = http_request.into_parts();
    let mut request = DataPlaneRequest::new(
        id,
        support::generate(),
        parts,
        bytes::Bytes::from(serde_json::to_vec(&body).unwrap()),
    );
    request.client_ip = Some("203.0.113.7".into());
    request
}

fn call_body() -> Value {
    json!({"model": "test/m1", "input": "hello"})
}

/// Drain the response into the capture and settle it, which is what writes the
/// record and its events. Answers the response bytes.
async fn settle(app: &TestApp, outcome: CallOutcome, end: CaptureOutcome) -> Vec<u8> {
    let CallOutcome {
        execution,
        admitted,
        capture,
    } = outcome;
    let (response, usage) = execution.into_parts();
    let bytes = support::read_bytes(response.body).await;
    match capture {
        Some(mut capture) => {
            for chunk in bytes.chunks(997) {
                capture.record_response_chunk(chunk);
            }
            capture
                .settle(app.gproxy().store(), end, usage)
                .await
                .unwrap();
        }
        None => {
            usage.await.unwrap();
        }
    }
    drop(admitted);
    bytes
}

async fn records(app: &TestApp) -> Vec<capture_record::Model> {
    app.gproxy()
        .store()
        .capture_records()
        .query(capture_record::Entity::find())
        .await
        .unwrap()
}

async fn downstream(app: &TestApp) -> Vec<capture_record::Model> {
    records(app)
        .await
        .into_iter()
        .filter(|row| row.side == capture_record::CaptureSide::Downstream)
        .collect()
}

/// The one downstream row, which most tests expect to be alone.
async fn only_downstream(app: &TestApp) -> capture_record::Model {
    let mut rows = downstream(app).await;
    assert_eq!(rows.len(), 1, "one request, one downstream record");
    rows.pop().unwrap()
}

async fn associated(app: &TestApp) -> Vec<capture_record::Model> {
    let mut rows: Vec<_> = records(app)
        .await
        .into_iter()
        .filter(|r| {
            r.side == capture_record::CaptureSide::Upstream && r.initiator_request_id.is_some()
        })
        .collect();
    rows.sort_by(|a, b| {
        (a.started_at_ms, a.attempt_ordinal, &a.id).cmp(&(
            b.started_at_ms,
            b.attempt_ordinal,
            &b.id,
        ))
    });
    rows
}

fn header(row: &capture_record::Model, name: &str) -> Option<String> {
    row.request_headers
        .as_ref()?
        .as_array()?
        .iter()
        .find(|pair| pair[0] == json!(name))
        .map(|pair| pair[1].as_str().unwrap_or_default().to_owned())
}

/// Whether the sentinel body reached the database at all, in any row and any
/// column.
async fn sentinel_stored(app: &TestApp) -> bool {
    records(app).await.iter().any(|row| {
        let body = |bytes: &Option<Vec<u8>>| {
            bytes
                .as_ref()
                .is_some_and(|bytes| String::from_utf8_lossy(bytes).contains(SENTINEL))
        };
        body(&row.request_body) || body(&row.response_body)
    })
}

// ------------------------------------------------------------ the switches --

#[tokio::test]
async fn the_switch_being_off_means_no_capture_at_all() {
    let (app, client) = one_provider().await;
    logging(&app, false, true, true).await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let caller = support::caller_for(&app, "k-alice").await;

    let body = json!({"model": "test/m1", "input": oversized()});
    let outcome = app.call(&caller, request("req-off", body)).await.unwrap();
    assert!(
        outcome.capture.is_none(),
        "with the switch off there is nothing to clone into"
    );
    settle(&app, outcome, CaptureOutcome::Complete).await;

    assert!(downstream(&app).await.is_empty(), "no downstream record");
    assert_eq!(
        associated(&app).await[0].initiator_request_id.as_deref(),
        Some("req-off"),
        "association survives disabled downstream logging"
    );
    assert!(
        !sentinel_stored(&app).await,
        "the body reached no row on either side"
    );
    // The upstream half is core's and is governed by its own switch, which
    // this one does not touch.
    assert_eq!(records(&app).await.len(), 1, "the upstream record stands");
}

#[tokio::test]
async fn the_body_switch_off_records_the_exchange_but_not_what_was_in_it() {
    let (app, client) = one_provider().await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let caller = support::caller_for(&app, "k-alice").await;

    let body = json!({"model": "test/m1", "input": oversized()});
    let outcome = app.call(&caller, request("req-1", body)).await.unwrap();
    settle(&app, outcome, CaptureOutcome::Complete).await;

    let row = only_downstream(&app).await;
    assert_eq!(
        row.id, "req-1",
        "a downstream record's id is the request id"
    );
    assert_eq!(row.kind, capture_record::CaptureKind::Http);
    assert_eq!(row.state, capture_record::CaptureState::Completed);
    assert_eq!(row.error, None);

    // Attribution is the admission decision, not anything from the wire.
    assert_eq!(row.user_id.as_deref(), Some("alice"));
    assert_eq!(row.api_key_id.as_deref(), Some("k-alice"));
    assert_eq!(
        row.model.as_deref(),
        Some("test/m1"),
        "the name the client asked for"
    );
    assert_eq!(row.operation.as_deref(), Some("generate_content"));
    assert_eq!(row.client_ip.as_deref(), Some("203.0.113.7"));
    assert_eq!(row.provider_id, None, "a retried request reached several");
    assert_eq!(row.credential_id, None);
    assert_eq!(row.metrics, None, "billed usage is the usage row's");

    // The exchange itself is there in full.
    assert_eq!(row.request_method.as_deref(), Some("POST"));
    assert_eq!(row.request_url.as_deref(), Some("/v1/responses"));
    assert_eq!(header(&row, "x-trace").as_deref(), Some("keep-me"));
    assert_eq!(row.response_status, Some(200));
    assert!(row.response_headers.is_some());
    assert!(row.first_response_at_ms.is_some());
    assert!(row.ended_at_ms.unwrap() >= row.started_at_ms);

    // The bodies are not.
    assert_eq!(row.request_body, None);
    assert_eq!(row.response_body, None);
    assert_eq!(
        row.request_body_state,
        capture_record::CaptureBodyState::NotCaptured
    );
    assert_eq!(
        row.response_body_state,
        capture_record::CaptureBodyState::NotCaptured
    );
    assert!(!sentinel_stored(&app).await);
}

#[tokio::test]
async fn the_body_switch_on_stores_both_directions() {
    let (app, client) = one_provider().await;
    logging(&app, true, true, true).await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let caller = support::caller_for(&app, "k-alice").await;

    let outcome = app
        .call(&caller, request("req-1", call_body()))
        .await
        .unwrap();
    let answer = settle(&app, outcome, CaptureOutcome::Complete).await;

    let row = only_downstream(&app).await;
    assert_eq!(
        serde_json::from_slice::<Value>(&row.request_body.unwrap()).unwrap(),
        call_body()
    );
    assert_eq!(row.response_body.as_deref(), Some(&answer[..]));
    assert_eq!(
        row.request_body_state,
        capture_record::CaptureBodyState::Complete
    );
    assert_eq!(
        row.response_body_state,
        capture_record::CaptureBodyState::Complete
    );
    assert_eq!(
        row.request_framing,
        capture_record::BodyFraming::Buffered,
        "the inline column, which is what Buffered means"
    );
}

#[tokio::test]
async fn large_request_and_chunked_response_are_retained_in_full() {
    let (app, client) = one_provider().await;
    logging(&app, true, true, true).await;
    client.script(vec![Reply::Http(
        StatusCode::OK,
        json!({"answer": oversized()}),
    )]);
    let caller = support::caller_for(&app, "k-alice").await;

    let body = json!({"model": "test/m1", "input": oversized()});
    let outcome = app
        .call(&caller, request("req-1", body.clone()))
        .await
        .unwrap();
    let answer = settle(&app, outcome, CaptureOutcome::Complete).await;
    assert!(answer.len() > 64 * 1024, "the answer was big");

    let row = only_downstream(&app).await;
    assert_eq!(
        serde_json::from_slice::<Value>(&row.request_body.unwrap()).unwrap(),
        body
    );
    assert_eq!(row.response_body.unwrap(), answer);
    assert_eq!(
        row.request_body_state,
        capture_record::CaptureBodyState::Complete,
        "all bytes are retained"
    );
    assert_eq!(
        row.response_body_state,
        capture_record::CaptureBodyState::Complete
    );
}

// ------------------------------------------------------------- redaction --

#[tokio::test]
async fn redaction_hides_the_credential_in_a_header_a_query_and_a_body() {
    let (app, client) = one_provider().await;
    logging(&app, true, true, true).await;
    client.script(vec![Reply::Http(
        StatusCode::OK,
        json!({"ok": true, "refresh_token": "rt-upstream"}),
    )]);
    let caller = support::caller_for(&app, "k-alice").await;

    let body = json!({"model": "test/m1", "api_key": "sk-in-the-body"});
    let outcome = app.call(&caller, request("req-1", body)).await.unwrap();
    settle(&app, outcome, CaptureOutcome::Complete).await;

    let row = only_downstream(&app).await;
    assert_eq!(header(&row, "authorization").as_deref(), Some("[redacted]"));
    assert_eq!(
        header(&row, "x-trace").as_deref(),
        Some("keep-me"),
        "only the names on the list"
    );
    assert_eq!(
        row.request_query.as_deref(),
        Some("stream=true&api_key=[redacted]")
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&row.request_body.unwrap()).unwrap(),
        json!({"model": "test/m1", "api_key": "[redacted]"})
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&row.response_body.unwrap()).unwrap(),
        json!({"ok": true, "refresh_token": "[redacted]"}),
        "what came back is redacted on the same list"
    );
}

#[tokio::test]
async fn disable_log_redaction_stores_the_exchange_as_it_was() {
    let (app, client) = one_provider().await;
    logging(&app, true, true, false).await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let caller = support::caller_for(&app, "k-alice").await;

    let body = json!({"model": "test/m1", "api_key": "sk-in-the-body"});
    let outcome = app.call(&caller, request("req-1", body)).await.unwrap();
    settle(&app, outcome, CaptureOutcome::Complete).await;

    let row = only_downstream(&app).await;
    assert_eq!(
        header(&row, "authorization").as_deref(),
        Some("Bearer sk-alice")
    );
    assert_eq!(
        row.request_query.as_deref(),
        Some("stream=true&api_key=sk-query")
    );
    assert!(
        String::from_utf8_lossy(&row.request_body.unwrap()).contains("sk-in-the-body"),
        "the operator asked for the raw exchange"
    );
}

// ----------------------------------------------------------------- edges --

#[tokio::test]
async fn a_settled_call_is_linked_to_the_upstream_it_reached() {
    let (app, client) = one_provider().await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let caller = support::caller_for(&app, "k-alice").await;

    let outcome = app
        .call(&caller, request("req-1", call_body()))
        .await
        .unwrap();
    settle(&app, outcome, CaptureOutcome::Complete).await;

    let upstream: Vec<capture_record::Model> = records(&app)
        .await
        .into_iter()
        .filter(|row| row.side == capture_record::CaptureSide::Upstream)
        .collect();
    assert_eq!(upstream.len(), 1);
    assert_eq!(upstream[0].provider_id.as_deref(), Some("p1"));

    let links = associated(&app).await;
    assert_eq!(links.len(), 1, "one exchange, one associated record");
    assert_eq!(links[0].initiator_request_id.as_deref(), Some("req-1"));
    assert_eq!(links[0].id, upstream[0].id);
}

#[tokio::test]
async fn a_retried_call_is_linked_to_every_provider_it_tried() {
    let (app, client) = two_providers().await;
    // The first target refuses with a 5xx, which is retried elsewhere; both
    // sends are real exchanges and both leave an upstream record.
    client.script(vec![
        Reply::Http(StatusCode::SERVICE_UNAVAILABLE, json!({"error": "busy"})),
        Reply::Http(StatusCode::OK, json!({"ok": true})),
    ]);
    let caller = support::caller_for(&app, "k-alice").await;

    let outcome = app
        .call(&caller, request("req-1", call_body()))
        .await
        .unwrap();
    assert_eq!(outcome.execution.response().status, StatusCode::OK);
    settle(&app, outcome, CaptureOutcome::Complete).await;
    assert_eq!(client.urls().len(), 2, "two providers were reached");

    let upstream: Vec<capture_record::Model> = records(&app)
        .await
        .into_iter()
        .filter(|row| row.side == capture_record::CaptureSide::Upstream)
        .collect();
    assert_eq!(upstream.len(), 2);

    let links = associated(&app).await;
    assert_eq!(
        links.len(),
        2,
        "the abandoned attempt is part of what this request did"
    );
    assert!(
        links
            .iter()
            .all(|link| link.initiator_request_id.as_deref() == Some("req-1"))
    );
    let mut linked: Vec<&str> = links.iter().map(|link| link.id.as_str()).collect();
    linked.sort_unstable();
    let mut written: Vec<&str> = upstream.iter().map(|row| row.id.as_str()).collect();
    written.sort_unstable();
    assert_eq!(
        linked, written,
        "every upstream record retains its downstream ID"
    );

    // The engine restarts its attempt counter at the next provider, so the
    // sequence cannot be the upstream's own ordinal and still order a retry.
    assert_eq!(
        upstream
            .iter()
            .map(|row| row.attempt_ordinal.unwrap())
            .collect::<Vec<_>>(),
        [1, 1]
    );
    let started = |provider: &str| {
        upstream
            .iter()
            .find(|row| row.provider_id.as_deref() == Some(provider))
            .unwrap()
            .started_at_ms
    };
    assert!(started("p1") <= started("p2"));
}

// ------------------------------------------------------------- endings --

#[tokio::test]
async fn a_cancelled_stream_is_recorded_as_cancelled() {
    let (app, client) = one_provider().await;
    logging(&app, true, true, true).await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let caller = support::caller_for(&app, "k-alice").await;

    let outcome = app
        .call(&caller, request("req-1", call_body()))
        .await
        .unwrap();
    settle(&app, outcome, CaptureOutcome::Cancelled).await;

    let row = only_downstream(&app).await;
    assert_eq!(row.state, capture_record::CaptureState::Cancelled);
    assert!(row.error.unwrap().contains("client disconnected"));
    assert_eq!(
        row.response_body_state,
        capture_record::CaptureBodyState::Partial,
        "a body that stopped early is not the whole body, cut or not"
    );
    assert_eq!(
        row.request_body_state,
        capture_record::CaptureBodyState::Complete,
        "what the client sent arrived whole regardless"
    );
}

#[tokio::test]
async fn an_interrupted_stream_is_a_failure_and_a_captured_error_is_not() {
    let (app, client) = one_provider().await;
    client.script(vec![
        Reply::Http(StatusCode::OK, json!({"ok": true})),
        Reply::Http(StatusCode::BAD_REQUEST, json!({"error": "nope"})),
    ]);
    let caller = support::caller_for(&app, "k-alice").await;

    let outcome = app
        .call(&caller, request("req-1", call_body()))
        .await
        .unwrap();
    settle(&app, outcome, CaptureOutcome::Interrupted).await;
    let row = only_downstream(&app).await;
    assert_eq!(row.state, capture_record::CaptureState::Failed);
    assert!(row.error.unwrap().contains("interrupted"));

    // A refusal that reached the client whole is a complete capture of a
    // failed exchange: the state follows the status, and there is no error.
    let outcome = app
        .call(&caller, request("req-2", call_body()))
        .await
        .unwrap();
    settle(&app, outcome, CaptureOutcome::Complete).await;
    let row = downstream(&app)
        .await
        .into_iter()
        .find(|row| row.id == "req-2")
        .unwrap();
    assert_eq!(row.response_status, Some(400));
    assert_eq!(row.state, capture_record::CaptureState::Failed);
    assert_eq!(row.error, None);
}

#[tokio::test]
async fn a_call_that_never_reached_an_upstream_still_leaves_a_record() {
    let (app, _client) = one_provider().await;
    let caller = support::caller_for(&app, "k-alice").await;

    // Neither a channel nor a provider is called `nowhere`, so resolution
    // fails after admission has already decided who this is.
    let error = app
        .call(&caller, request("req-1", json!({"model": "nowhere/m1"})))
        .await
        .unwrap_err();
    assert_ne!(error.status_code(), 500);

    let row = only_downstream(&app).await;
    assert_eq!(row.state, capture_record::CaptureState::Failed);
    assert!(row.error.is_some(), "the engine's reason is kept");
    assert_eq!(row.response_status, None, "nothing was answered");
    assert!(associated(&app).await.is_empty());
}

// --------------------------------------------------- persistence failures --

/// A capture built by hand, so the failure paths can be driven without an
/// engine behind them. Everything it carries is what admission would have
/// decided.
fn handmade(app: &TestApp, id: &str) -> DownstreamCapture {
    let caller = Caller {
        user_id: "alice".into(),
        user_role: "user".into(),
        api_key_id: Some("k-alice".into()),
        organization_id: None,
        team_id: None,
        grant: None,
        kind: CallerKind::ApiKey,
        management: false,
    };
    let admitted = gproxy_app::Admitted {
        scope: "user:alice".into(),
        attribution: UsageAttribution {
            user_id: Some("alice".into()),
            api_key_id: Some("k-alice".into()),
            model: Some("test/m1".into()),
        },
        budgets: Vec::new(),
        providers: Default::default(),
        credentials: Default::default(),
        session: None,
        rate_limit_leases: Vec::new(),
    };
    let switches = app.data().observation;
    DownstreamCapture::open(&switches, &request(id, call_body()), &caller, &admitted).unwrap()
}

#[tokio::test]
async fn a_downstream_log_does_not_require_an_upstream_record() {
    let (app, _client) = one_provider().await;
    let id = handmade(&app, "req-1")
        .finish(app.gproxy().store(), CaptureOutcome::Complete)
        .await
        .unwrap();
    assert_eq!(id, "req-1");
    assert_eq!(only_downstream(&app).await.id, "req-1");
    assert!(associated(&app).await.is_empty());
}

#[tokio::test]
async fn a_write_that_cannot_land_is_reported_and_not_panicked_on() {
    let (app, _client) = one_provider().await;
    handmade(&app, "req-1")
        .finish(app.gproxy().store(), CaptureOutcome::Complete)
        .await
        .unwrap();
    // The id is the request id, so a second record under it collides. Nothing
    // above this line can be undone by that, which is the whole point.
    let error = handmade(&app, "req-1")
        .finish(app.gproxy().store(), CaptureOutcome::Complete)
        .await
        .unwrap_err();
    assert_eq!(
        error.status_code(),
        500,
        "the operator's problem, not a caller's"
    );
    assert_eq!(downstream(&app).await.len(), 1);
}

#[tokio::test]
async fn a_capture_failure_does_not_change_what_the_caller_is_told() {
    let (app, _client) = one_provider().await;
    // A record already exists under the id the failing call will use, so the
    // capture `App::call` writes on its error path cannot land.
    handmade(&app, "req-1")
        .finish(app.gproxy().store(), CaptureOutcome::Complete)
        .await
        .unwrap();

    let caller = support::caller_for(&app, "k-alice").await;
    let error = app
        .call(&caller, request("req-1", json!({"model": "nowhere/m1"})))
        .await
        .unwrap_err();
    assert_ne!(
        error.status_code(),
        500,
        "the caller is told why the call failed, not that logging did"
    );
    assert_eq!(downstream(&app).await.len(), 1, "nothing was overwritten");
}

// ----------------------------------------------------------- the switches --

#[tokio::test]
async fn a_websocket_handshake_is_captured_as_a_connection() {
    let (app, _client) = one_provider().await;
    let mut capture = handmade(&app, "req-ws");
    capture.record_response_head(StatusCode::SWITCHING_PROTOCOLS, &http::HeaderMap::new());
    capture
        .finish(app.gproxy().store(), CaptureOutcome::Complete)
        .await
        .unwrap();

    let row = only_downstream(&app).await;
    assert_eq!(row.kind, capture_record::CaptureKind::WsConnection);
    assert_eq!(row.response_status, Some(101));
    assert_eq!(
        row.state,
        capture_record::CaptureState::Completed,
        "101 is not a failure"
    );
}

#[tokio::test]
async fn websocket_frames_after_64_kib_are_retained_in_full() {
    let (app, _) = one_provider().await;
    logging(&app, true, true, true).await;
    let mut capture = handmade(&app, "large-ws");
    capture.record_response_head(StatusCode::SWITCHING_PROTOCOLS, &http::HeaderMap::new());
    let first = "文".repeat(32 * 1024);
    let last = "last frame after the former cutoff";
    capture.record_frame(CaptureDirection::Response, CapturedFrame::Text(&first));
    capture.record_frame(CaptureDirection::Request, CapturedFrame::Text(last));
    capture
        .finish(app.gproxy().store(), CaptureOutcome::Complete)
        .await
        .unwrap();
    let detail = app
        .gproxy()
        .query()
        .logs()
        .detail("large-ws")
        .await
        .unwrap();
    assert_eq!(detail.events.len(), 2);
    assert_eq!(detail.events[0].payload.content, first);
    assert_eq!(detail.events[1].payload.content, last);
    assert_eq!(detail.downstream.response_body.state, "complete");
}
