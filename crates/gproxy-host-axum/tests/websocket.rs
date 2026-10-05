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

#[path = "websocket/responses_retry.rs"]
mod responses_retry;
mod support;

use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use gproxy_protocol::connection::{Bytes, WsClose, WsFrame};
use gproxy_seaorm::FixedDecimal;
use gproxy_store::entity::{
    limits::rate_limit,
    usage::{downstream_event as capture_event, downstream_record as capture_record, usage_record},
};
use http::StatusCode;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde_json::json;
use support::{Bound, Host, WsReply, get, keyed, with};

async fn responses_instance(dialect: &str, concurrency: Option<i64>) -> Host {
    use gproxy_store::entity::upstream::{provider, provider_model};
    let host = instance(concurrency).await;
    provider::Entity::update(provider::ActiveModel {
        id: Set("p1".into()),
        config: Set(json!({"test_dialect":dialect})),
        ..Default::default()
    })
    .exec(host.handle().store().connection())
    .await
    .unwrap();
    provider_model::Entity::update(provider_model::ActiveModel {
        id: Set("p1-m1".into()),
        metadata: Set(json!({"variants":["fast"]})),
        ..Default::default()
    })
    .exec(host.handle().store().connection())
    .await
    .unwrap();
    host.publish().await;
    host
}

async fn ws_json(socket: &mut Socket) -> serde_json::Value {
    match recv(socket).await {
        Some(Message::Text(text)) => serde_json::from_str(&text).unwrap(),
        message => panic!("expected JSON event, received {message:?}"),
    }
}

async fn ws_send(socket: &mut Socket, value: serde_json::Value) {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .unwrap();
}

fn sse(value: serde_json::Value) -> Bytes {
    Bytes::from(format!("data: {value}\n\n"))
}

fn response_item(id: &str, text: &str) -> serde_json::Value {
    json!({"id":id,"type":"message","role":"assistant","status":"completed",
        "content":[{"type":"output_text","text":text,"annotations":[],"logprobs":[]}]})
}

fn finish_http_segment(
    tx: tokio::sync::mpsc::UnboundedSender<Bytes>,
    item: serde_json::Value,
    input: u64,
    output: u64,
) {
    tx.send(sse(
        json!({"type":"response.output_item.done","output_index":0,"item":item}),
    ))
    .unwrap();
    tx.send(sse(json!({"type":"response.completed","response":{"id":"native","status":"completed","output":[item],
        "usage":{"input_tokens":input,"output_tokens":output,"total_tokens":input+output}}}))).unwrap();
}

async fn until_type(socket: &mut Socket, kind: &str) -> serde_json::Value {
    for _ in 0..30 {
        let event = ws_json(socket).await;
        assert_ne!(event["type"], "error", "{event}");
        assert_ne!(event["type"], "response.failed", "{event}");
        if event["type"] == kind {
            return event;
        }
    }
    panic!("missing {kind}")
}

#[tokio::test(flavor = "multi_thread")]
async fn responses_injection_keeps_one_response_and_accounts_for_both_http_segments() {
    let host = responses_instance("openai", Some(1)).await;
    let (first, rx) = tokio::sync::mpsc::unbounded_channel();
    let (second, rx2) = tokio::sync::mpsc::unbounded_channel();
    host.client.script(vec![
        support::Reply::Sse(StatusCode::OK, rx),
        support::Reply::Sse(StatusCode::OK, rx2),
    ]);
    let bound = host.bind().await;
    let mut client = dial(&bound, "/v1/responses", Some("k-alice"))
        .await
        .unwrap();
    ws_send(
        &mut client,
        json!({"type":"response.create","model":"p1/m1","input":"start","store":false}),
    )
    .await;
    let created = ws_json(&mut client).await;
    let id = created["response"]["id"].clone();
    first.send(sse(json!({"type":"response.output_item.added","output_index":0,"item":response_item("a", "first")}))).unwrap();
    until_type(&mut client, "response.output_item.added").await;
    ws_send(&mut client, json!({"type":"response.inject","response_id":id,"input":[{"role":"user","content":"extra"}]})).await;
    assert_eq!(
        ws_json(&mut client).await["type"],
        "response.inject.created"
    );
    finish_http_segment(first, response_item("a", "first"), 3, 2);
    second.send(sse(json!({"type":"response.output_item.added","output_index":0,"item":response_item("b", "second")}))).unwrap();
    let item = until_type(&mut client, "response.output_item.added").await;
    assert_eq!(item["output_index"], 1);
    finish_http_segment(second, response_item("b", "second"), 4, 1);
    let end = until_type(&mut client, "response.completed").await;
    assert_eq!(end["response"]["id"], id);
    assert_eq!(end["response"]["output"].as_array().unwrap().len(), 2);
    assert_eq!(end["response"]["usage"]["input_tokens"], 7);
    let bodies = host.client.bodies();
    assert_eq!(bodies.len(), 2);
    assert_eq!(bodies[1]["input"].as_array().unwrap().len(), 3);
    assert_eq!(bodies[1]["input"][2]["content"], "extra");
    assert_eq!(usage_rows(&host).await.len(), 2);
    ws_send(&mut client, json!({"type":"response.inject","response_id":id,"input":[{"role":"user","content":"too late"}]})).await;
    let refused = ws_json(&mut client).await;
    assert_eq!(refused["type"], "response.inject.failed");
    assert_eq!(refused["error"]["code"], "response_already_completed");
    client.close(None).await.unwrap();
    let _ = recv(&mut client).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn responses_lanes_are_independent_and_an_unknown_model_does_not_close_the_socket() {
    let host = responses_instance("openai_responses_websocket", Some(2)).await;
    let upstream_a = host.client.accept_socket();
    let upstream_b = host.client.accept_socket();
    let bound = host.bind().await;
    let mut client = dial(&bound, "/v1/responses", Some("k-alice"))
        .await
        .unwrap();
    ws_send(
        &mut client,
        json!({"type":"response.create","model":"missing-model","input":"no"}),
    )
    .await;
    assert_eq!(ws_json(&mut client).await["type"], "error");
    assert!(host.client.urls().is_empty());
    ws_send(
        &mut client,
        json!({"type":"response.create","stream_id":"a","model":"p1/m1","input":"slow"}),
    )
    .await;
    assert!(upstream_a.next().await.is_some());
    ws_send(
        &mut client,
        json!({"type":"response.create","stream_id":"b","model":"p1/m1","input":"fast"}),
    )
    .await;
    assert!(
        tokio::time::timeout(Duration::from_secs(5), upstream_b.next())
            .await
            .unwrap()
            .is_some()
    );
    upstream_b.send.send(Ok(WsFrame::Text(json!({"type":"response.completed","stream_id":"b",
        "response":{"id":"b-response","status":"completed","usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}}}).to_string()))).unwrap();
    let response = ws_json(&mut client).await;
    assert_eq!(response["stream_id"], "b", "lane a must not block lane b");
    client.close(None).await.unwrap();
    let _ = recv(&mut client).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn responses_steering_waits_for_real_tool_results_before_its_successor() {
    let cases = [
        (
            json!({"id":"fc","type":"function_call","call_id":"call","name":"run","arguments":"{}","status":"completed"}),
            json!({"type":"function_call_output","call_id":"call","output":"42"}),
        ),
        (
            json!({"id":"custom","type":"custom_tool_call","call_id":"call","name":"run","input":"hi","status":"completed"}),
            json!({"type":"custom_tool_call_output","call_id":"call","output":"42"}),
        ),
        (
            json!({"id":"computer","type":"computer_call","call_id":"call","action":{"type":"screenshot"},"pending_safety_checks":[],"status":"completed"}),
            json!({"type":"computer_call_output","call_id":"call","output":{"type":"computer_screenshot","image_url":"data:image/png;base64,AA=="}}),
        ),
        (
            json!({"id":"shell","type":"shell_call","call_id":"call","action":{"commands":["pwd"],"max_output_length":null,"timeout_ms":null},"environment":{"type":"local"},"status":"completed"}),
            json!({"type":"shell_call_output","call_id":"call","output":[{"stdout":"/","stderr":"","outcome":{"type":"exit","exit_code":0}}]}),
        ),
        (
            json!({"id":"patch","type":"apply_patch_call","call_id":"call","operation":{"type":"delete_file","path":"a"},"status":"completed"}),
            json!({"type":"apply_patch_call_output","call_id":"call","status":"completed"}),
        ),
        (
            json!({"id":"search","type":"tool_search_call","call_id":"call","arguments":{},"execution":"client","status":"completed"}),
            json!({"type":"tool_search_output","call_id":"call","execution":"client","tools":[]}),
        ),
        (
            json!({"id":"call","type":"mcp_approval_request","name":"run","arguments":"{}","server_label":"local"}),
            json!({"type":"mcp_approval_response","approval_request_id":"call","approve":true}),
        ),
    ];
    for (call, result) in cases {
        let host = responses_instance("openai", Some(1)).await;
        let (first, rx) = tokio::sync::mpsc::unbounded_channel();
        let (second, rx2) = tokio::sync::mpsc::unbounded_channel();
        host.client.script(vec![
            support::Reply::Sse(StatusCode::OK, rx),
            support::Reply::Sse(StatusCode::OK, rx2),
        ]);
        let bound = host.bind().await;
        let mut client = dial(&bound, "/v1/responses", Some("k-alice"))
            .await
            .unwrap();
        ws_send(&mut client, json!({"type":"response.create","model":"p1/m1","input":"start","store":false,
        "tools":[{"type":"function","name":"run","parameters":{"type":"object","properties":{}},"strict":false,"async":true}]})).await;
        let response = ws_json(&mut client).await;
        assert_eq!(response["type"], "response.created", "{response}");
        let id = response["response"]["id"].clone();
        first
            .send(sse(
                json!({"type":"response.output_item.added","output_index":0,"item":call}),
            ))
            .unwrap();
        first
            .send(sse(
                json!({"type":"response.output_item.done","output_index":0,"item":call}),
            ))
            .unwrap();
        let emitted = until_type(&mut client, "response.output_item.done").await;
        if call["type"] == "function_call" {
            assert_eq!(emitted["item"]["async"], true);
        }
        ws_send(
            &mut client,
            json!({"type":"response.steer","previous_response_id":id,"input":"explain the result"}),
        )
        .await;
        let pending = until_type(&mut client, "response.steer.pending").await;
        let required = &pending["required_input"][0];
        assert_eq!(required["type"], result["type"]);
        if call["type"] == "mcp_approval_request" {
            assert_eq!(required["approval_request_id"], "call");
            ws_send(&mut client, json!({"type":"response.inject","response_id":id,"input":[{"type":"mcp_approval_response","approval_request_id":"wrong","approve":true}]})).await;
            assert_eq!(ws_json(&mut client).await["type"], "error");
        } else {
            assert_eq!(required["call_id"], "call");
        }
        assert_eq!(host.client.urls().len(), 1, "no tool result is invented");
        ws_send(
            &mut client,
            json!({"type":"response.inject","response_id":id,
        "input":[result]}),
        )
        .await;
        until_type(&mut client, "response.incomplete").await;
        let next = until_type(&mut client, "response.created").await;
        assert_ne!(next["response"]["id"], id);
        finish_http_segment(second, response_item("answer", "42"), 5, 2);
        until_type(&mut client, "response.completed").await;
        let requests = host.client.bodies();
        let input = requests[1]["input"].as_array().unwrap();
        assert_eq!(input.len(), 4);
        assert_eq!(input[1]["type"], call["type"]);
        assert_eq!(input[2]["type"], result["type"]);
        assert_eq!(input[3]["content"], "explain the result");
        client.close(None).await.unwrap();
        let _ = recv(&mut client).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn responses_checks_model_permission_after_upgrade_and_can_retry_after_policy_changes() {
    let host = responses_instance("openai_responses_websocket", None).await;
    support::person(&host.handle(), "bob", "user").await;
    support::api_key(&host.handle(), "k-bob", "bob", None, None).await;
    host.publish().await;
    let bound = host.bind().await;
    let mut client = dial(&bound, "/v1/responses", Some("k-bob")).await.unwrap();
    ws_send(
        &mut client,
        json!({"type":"response.create","model":"p1/m1","input":"denied"}),
    )
    .await;
    let denied = ws_json(&mut client).await;
    assert_eq!(denied["status"], 403);
    assert!(host.client.urls().is_empty());
    support::allow(&host.handle(), "bob-permission", "bob", None).await;
    host.publish().await;
    let upstream = host.client.accept_socket();
    ws_send(
        &mut client,
        json!({"type":"response.create","model":"p1/m1","input":"allowed"}),
    )
    .await;
    assert!(
        tokio::time::timeout(Duration::from_secs(5), upstream.next())
            .await
            .unwrap()
            .is_some()
    );
    client.close(None).await.unwrap();
    let _ = recv(&mut client).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn responses_native_steering_rejection_releases_its_reservation_and_successor_is_metered() {
    let host = responses_instance("openai_responses_websocket", Some(1)).await;
    let upstream = host.client.accept_socket();
    let bound = host.bind().await;
    let mut client = dial(&bound, "/v1/responses", Some("k-alice"))
        .await
        .unwrap();
    ws_send(
        &mut client,
        json!({"type":"response.create","model":"p1/m1","input":"start"}),
    )
    .await;
    upstream.next().await.unwrap();
    upstream
        .send
        .send(Ok(WsFrame::Text(
            json!({"type":"response.created","response":{"id":"r1","status":"in_progress"}})
                .to_string(),
        )))
        .unwrap();
    until_type(&mut client, "response.created").await;
    for attempt in 0..2 {
        ws_send(&mut client, json!({"type":"response.steer","previous_response_id":"r1","input":"new direction","extension":true})).await;
        let frame = tokio::time::timeout(Duration::from_secs(5), upstream.next())
            .await
            .unwrap()
            .unwrap();
        let WsFrame::Text(frame) = frame else {
            panic!("text")
        };
        let frame: serde_json::Value = serde_json::from_str(&frame).unwrap();
        assert_eq!(frame["type"], "response.steer");
        assert_eq!(frame["extension"], true);
        if attempt == 0 {
            upstream
                .send
                .send(Ok(WsFrame::Text(
                    json!({"type":"error","status":400,
                "error":{"type":"invalid_request_error","message":"steering refused"}})
                    .to_string(),
                )))
                .unwrap();
            assert_eq!(ws_json(&mut client).await["type"], "error");
        }
    }
    for event in [
        json!({"type":"response.steer.accepted","sequence_number":1,"steer":{"id":"s","previous_response_id":"r1"}}),
        json!({"type":"response.incomplete","response":{"id":"r1","status":"incomplete","incomplete_details":{"reason":"steered"},
            "usage":{"input_tokens":2,"output_tokens":1,"total_tokens":3}}}),
        json!({"type":"response.created","response":{"id":"r2","status":"in_progress"}}),
        json!({"type":"response.completed","response":{"id":"r2","status":"completed",
            "usage":{"input_tokens":3,"output_tokens":2,"total_tokens":5}}}),
    ] {
        upstream
            .send
            .send(Ok(WsFrame::Text(event.to_string())))
            .unwrap();
    }
    until_type(&mut client, "response.incomplete").await;
    assert_eq!(
        until_type(&mut client, "response.created").await["response"]["id"],
        "r2"
    );
    until_type(&mut client, "response.completed").await;
    assert_eq!(usage_rows(&host).await.len(), 2);
    client.close(None).await.unwrap();
    let _ = recv(&mut client).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn responses_independent_turns_can_select_different_providers_on_one_client_connection() {
    use gproxy_store::entity::upstream::provider;
    let host = responses_instance("openai_responses_websocket", Some(1)).await;
    support::provider(&host.handle(), "p2", &["m2"]).await;
    support::credential(&host.handle(), "c2", "p2", None, None, None).await;
    provider::Entity::update(provider::ActiveModel {
        id: Set("p2".into()),
        config: Set(json!({"test_dialect":"openai_responses_websocket"})),
        ..Default::default()
    })
    .exec(host.handle().store().connection())
    .await
    .unwrap();
    host.publish().await;
    let first = host.client.accept_socket();
    let second = host.client.accept_socket();
    let bound = host.bind().await;
    let mut client = dial(&bound, "/v1/responses?key=k-alice", None)
        .await
        .unwrap();
    for (upstream, provider, model) in [(&first, "p1", "m1"), (&second, "p2", "m2")] {
        ws_send(
            &mut client,
            json!({"type":"response.create","model":format!("{provider}/{model}"),"input":"hello"}),
        )
        .await;
        let WsFrame::Text(text) = tokio::time::timeout(Duration::from_secs(5), upstream.next())
            .await
            .unwrap()
            .unwrap()
        else {
            panic!("text")
        };
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&text).unwrap()["model"],
            model
        );
        upstream
            .send
            .send(Ok(WsFrame::Text(
                json!({"type":"response.completed","response":{"id":format!("{provider}-response"),
            "status":"completed","usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}}})
                .to_string(),
            )))
            .unwrap();
        until_type(&mut client, "response.completed").await;
    }
    assert_eq!(
        host.client.urls(),
        vec![
            "https://p1.example/v1/responses",
            "https://p2.example/v1/responses"
        ]
    );
    client.close(None).await.unwrap();
    let _ = recv(&mut client).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn responses_upstream_handshake_refusal_is_a_structured_error_after_client_upgrade() {
    let host = responses_instance("openai_responses_websocket", None).await;
    host.client.push_socket(WsReply::Rejected(StatusCode::TOO_MANY_REQUESTS, json!({
        "error":{"type":"rate_limit_error","code":"insufficient_quota","message":"quota exhausted"}
    })));
    let bound = host.bind().await;
    let mut client = dial(&bound, "/v1/responses", Some("k-alice"))
        .await
        .unwrap();
    ws_send(
        &mut client,
        json!({"type":"response.create","stream_id":"quota","model":"p1/m1","input":"hi"}),
    )
    .await;
    let error = ws_json(&mut client).await;
    assert_eq!(error["status"], 429);
    assert_eq!(error["stream_id"], "quota");
    assert_eq!(error["error"]["code"], "insufficient_quota");
    assert_eq!(error["error"]["message"], "quota exhausted");
    assert!(usage_rows(&host).await.is_empty());
    client.close(None).await.unwrap();
    let _ = recv(&mut client).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn responses_selects_from_first_frame_reuses_native_socket_and_settles_each_turn() {
    let host = responses_instance("openai_responses_websocket", Some(1)).await;
    settings(
        &host,
        gproxy_sdk::dto::SettingsPatch {
            instance: Some(gproxy_sdk::dto::InstanceSettingsPatch {
                stream_idle_timeout_ms: Some(500),
                ..Default::default()
            }),
            logging: Some(gproxy_sdk::dto::LoggingSettingsPatch {
                enable_downstream_log_body: Some(true),
                ..Default::default()
            }),
        },
    )
    .await;
    let upstream = host.client.accept_socket();
    let bound = host.bind().await;
    let mut client = dial(&bound, "/v1/responses", Some("k-alice"))
        .await
        .unwrap();
    assert!(
        host.client.urls().is_empty(),
        "upgrade does not select an upstream"
    );
    for (id, previous) in [("r1", None), ("r2", Some("r1"))] {
        ws_send(
            &mut client,
            json!({"type":"response.create","model":"p1/fast","input":"hello",
            "previous_response_id":previous,"opaque_extension":{"keep":true}}),
        )
        .await;
        let frame = tokio::time::timeout(Duration::from_secs(5), upstream.next())
            .await
            .unwrap()
            .unwrap();
        let WsFrame::Text(text) = frame else {
            panic!("text")
        };
        let sent: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(sent["model"], "m1");
        assert_eq!(sent["opaque_extension"]["keep"], true);
        upstream
            .send
            .send(Ok(WsFrame::Text(
                json!({"type":"response.created","response":{"id":id,"status":"in_progress"}})
                    .to_string(),
            )))
            .unwrap();
        upstream
            .send
            .send(Ok(WsFrame::Text(
                json!({"type":"response.completed","response":{"id":id,"status":"completed",
            "usage":{"input_tokens":3,"output_tokens":2,"total_tokens":5}}})
                .to_string(),
            )))
            .unwrap();
        assert_eq!(ws_json(&mut client).await["type"], "response.created");
        assert_eq!(ws_json(&mut client).await["type"], "response.completed");
    }
    assert_eq!(
        host.client.urls().len(),
        1,
        "the continuation reuses the physical connection"
    );
    assert_eq!(
        usage_rows(&host).await.len(),
        2,
        "usage is settled before socket closure"
    );
    assert!(matches!(recv(&mut client).await, Some(Message::Close(_))));
    assert!(matches!(upstream.next().await, Some(WsFrame::Close(_))));
    settled(&host).await;
    assert_eq!(usage_rows(&host).await.len(), 2);
    let captures = records(&host).await;
    assert_eq!(
        captures
            .iter()
            .filter(|r| r.kind == capture_record::CaptureKind::WsConnection)
            .count(),
        1
    );
    assert_eq!(
        captures
            .iter()
            .filter(|r| r.kind == capture_record::CaptureKind::WsTurn)
            .count(),
        2
    );
    let connection = captures
        .iter()
        .find(|row| row.kind == capture_record::CaptureKind::WsConnection)
        .unwrap();
    let events = events(&host, &connection.id).await;
    let requests: Vec<_> = events
        .iter()
        .filter(|event| {
            event.direction == capture_event::CaptureDirection::Request
                && event.kind == capture_event::CaptureEventKind::WsText
        })
        .collect();
    assert_eq!(requests.len(), 2);
    assert_ne!(requests[0].turn_id, requests[1].turn_id);
    for request in requests {
        let turn = request.turn_id.as_ref().unwrap();
        assert!(
            captures
                .iter()
                .any(|row| &row.id == turn && row.session_id.as_ref() == Some(&connection.id))
        );
        assert!(events.iter().any(|event| event.direction
            == capture_event::CaptureDirection::Response
            && event.turn_id.as_ref() == Some(turn)));
    }
}

/// A chain's upstream connection is chosen with its first turn in hand, so a
/// turn referencing an uploaded file opens on the credential holding it
/// (`gproxy_core::owned`), whichever the pool would have picked otherwise.
#[tokio::test(flavor = "multi_thread")]
async fn responses_a_new_chain_opens_on_the_credential_holding_its_files() {
    use gproxy_store::entity::resource::resource_binding;
    let host = responses_instance("openai_responses_websocket", None).await;
    let handle = host.handle();
    support::credential(&handle, "c2", "p1", None, None, None).await;
    handle
        .store()
        .resource_bindings()
        .create_many(vec![resource_binding::ActiveModel {
            id: Set("own-file-x".into()),
            scope: Set("user:alice\u{1f}p1".into()),
            kind: Set(gproxy_core::owned::FILE_KIND.into()),
            public_id: Set("file-x".into()),
            generation: Set(0),
            provider_id: Set("p1".into()),
            upstream_id: Set(Some("file-x".into())),
            credential_id: Set("c2".into()),
            summary: Set(json!({})),
            created_at_ms: Set(0),
            updated_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    host.publish().await;

    let upstream = host.client.accept_socket();
    let bound = host.bind().await;
    let mut client = dial(&bound, "/v1/responses", Some("k-alice"))
        .await
        .unwrap();
    ws_send(
        &mut client,
        json!({"type":"response.create","model":"p1/m1","input":[{"role":"user","content":[
            {"type":"input_file","file_id":"file-x"}
        ]}]}),
    )
    .await;
    let frame = tokio::time::timeout(Duration::from_secs(5), upstream.next())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(frame, WsFrame::Text(_)));
    assert_eq!(host.client.handshake_auth(), ["Bearer k-c2"]);

    // Another caller's file is refused on a new chain, before any upstream.
    ws_send(
        &mut client,
        json!({"type":"response.create","model":"p1/m1","stream_id":"other","input":[{"role":"user","content":[
            {"type":"input_file","file_id":"file-theirs"}
        ]}]}),
    )
    .await;
    let mut refused = false;
    for _ in 0..5 {
        let event = ws_json(&mut client).await;
        if event["type"] == "error" {
            assert!(event.to_string().contains("file-theirs"), "{event}");
            refused = true;
            break;
        }
    }
    assert!(refused, "the foreign file must be refused");
    assert_eq!(host.client.handshake_auth().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn responses_bridge_warmup_has_no_generation_and_store_false_continues_then_interrupts() {
    let host = instance(Some(1)).await;
    let bound = host.bind().await;
    let mut client = dial(&bound, "/p1/v1/responses", Some("k-alice"))
        .await
        .unwrap();
    ws_send(&mut client, json!({"type":"response.create","model":"m1","input":"warm context","store":false,"generate":false})).await;
    let created = ws_json(&mut client).await;
    assert_eq!(created["type"], "response.created", "{created}");
    let warm = ws_json(&mut client).await;
    assert_eq!(warm["type"], "response.completed");
    assert!(host.client.urls().is_empty());
    assert!(warm["response"]["usage"].is_null());
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    host.client.push(support::Reply::Sse(StatusCode::OK, rx));
    ws_send(
        &mut client,
        json!({"type":"response.create","model":"m1","input":"continue","store":false,
        "previous_response_id":warm["response"]["id"]}),
    )
    .await;
    let created = ws_json(&mut client).await;
    assert_eq!(created["type"], "response.created", "{created}");
    tx.send(sse(
        json!({"type":"response.output_item.added","output_index":0,"item":{
        "id":"partial","type":"message","role":"assistant","status":"in_progress","content":[]}}),
    ))
    .unwrap();
    tx.send(sse(json!({"type":"response.output_text.delta","item_id":"partial","output_index":0,"content_index":0,"delta":"unfinished"}))).unwrap();
    assert_eq!(
        ws_json(&mut client).await["type"],
        "response.output_item.added"
    );
    assert_eq!(
        ws_json(&mut client).await["type"],
        "response.output_text.delta"
    );
    let bodies = host.client.bodies();
    assert_eq!(bodies.len(), 1);
    assert_eq!(bodies[0]["model"], "m1");
    assert_eq!(bodies[0]["input"][0]["content"], "warm context");
    assert_eq!(bodies[0]["input"][1]["content"], "continue");
    ws_send(&mut client, json!({"type":"response.interrupt","response_id":created["response"]["id"],"mode":"discard_partial_items"})).await;
    assert_eq!(
        ws_json(&mut client).await["type"],
        "response.interrupt.accepted"
    );
    assert_eq!(
        ws_json(&mut client).await["type"],
        "response.output_item.interrupted"
    );
    let end = ws_json(&mut client).await;
    assert_eq!(end["type"], "response.incomplete", "{end}");
    assert_eq!(
        end["response"]["incomplete_details"]["reason"],
        "interrupted"
    );
    assert_eq!(end["response"]["output"], json!([]));
    client.close(None).await.unwrap();
    let _ = recv(&mut client).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn responses_stored_history_and_target_survive_reconnection_but_store_false_does_not() {
    let host = responses_instance("openai", None).await;
    settings(
        &host,
        gproxy_sdk::dto::SettingsPatch {
            instance: Some(gproxy_sdk::dto::InstanceSettingsPatch {
                stream_idle_timeout_ms: Some(500),
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .await;
    let bound = host.bind().await;
    for stored in [true, false] {
        let mut client = dial(&bound, "/v1/responses", Some("k-alice"))
            .await
            .unwrap();
        ws_send(&mut client, json!({"type":"response.create","model":"p1/m1","input":"saved","generate":false,"store":stored})).await;
        until_type(&mut client, "response.created").await;
        let previous =
            until_type(&mut client, "response.completed").await["response"]["id"].clone();
        // Idle lanes and their upstream connections are released; stored
        // history remains resumable on the next connection.
        assert!(matches!(recv(&mut client).await, Some(Message::Close(_))));
        let mut client = dial(&bound, "/v1/responses", Some("k-alice"))
            .await
            .unwrap();
        ws_send(
            &mut client,
            json!({"type":"response.create","model":"p1/m1","input":"next",
            "previous_response_id":previous,"generate":false,"store":stored}),
        )
        .await;
        let event = ws_json(&mut client).await;
        if stored {
            assert_eq!(event["type"], "response.created", "{event}");
            until_type(&mut client, "response.completed").await;
        } else {
            assert_eq!(event["type"], "error", "{event}");
            assert_eq!(event["status"], 400);
        }
        client.close(None).await.unwrap();
        let _ = recv(&mut client).await;
    }
    assert!(host.client.urls().is_empty());
    // History writes can succeed while the separate target binding fails.
    // The client must not receive a completed response it cannot resume.
    use sea_orm::ConnectionTrait;
    host.handle().store().connection().execute_unprepared(
        "CREATE TRIGGER fail_response_binding BEFORE INSERT ON protocol_states WHEN NEW.scope LIKE 'responses-target:%' BEGIN SELECT RAISE(FAIL, 'binding unavailable'); END"
    ).await.unwrap();
    let mut client = dial(&bound, "/v1/responses", Some("k-alice"))
        .await
        .unwrap();
    ws_send(
        &mut client,
        json!({"type":"response.create","model":"p1/m1","input":"save fails","generate":false}),
    )
    .await;
    assert_eq!(ws_json(&mut client).await["type"], "response.created");
    let failed = ws_json(&mut client).await;
    assert_eq!(failed["type"], "error");
    assert_eq!(failed["error"]["code"], "continuation_unavailable");
    assert_eq!(failed["status"], 500);
    client.close(None).await.unwrap();
    let _ = recv(&mut client).await;
}
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
    // Sockets are what these tests record, and an instance logs nothing
    // until it is asked to.
    settings(
        &host,
        gproxy_sdk::dto::SettingsPatch {
            logging: Some(gproxy_sdk::dto::LoggingSettingsPatch {
                enable_downstream_log: Some(true),
                enable_upstream_log: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .await;
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

    // A refused handshake without metered usage remains a log, not a fabricated usage row.
    assert!(usage_rows(&host).await.is_empty());
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

#[tokio::test(flavor = "multi_thread")]
async fn an_oversized_frame_is_refused_with_the_code_the_rfc_reserves_for_it() {
    let host = instance(None).await;
    // A limit small enough to exceed without allocating anything silly.
    settings(
        &host,
        gproxy_sdk::dto::SettingsPatch {
            instance: Some(gproxy_sdk::dto::InstanceSettingsPatch {
                max_ws_frame_bytes: Some(64),
                ..Default::default()
            }),
            logging: None,
        },
    )
    .await;

    let upstream = host.client.accept_socket();
    let bound = host.bind().await;
    let mut client = dial(&bound, "/v1/realtime", Some("k-alice")).await.unwrap();

    // Exactly at the limit is fine.
    client
        .send(Message::Binary(Bytes::from(vec![7_u8; 64])))
        .await
        .unwrap();
    assert_eq!(
        upstream.next().await,
        Some(WsFrame::Binary(Bytes::from(vec![7_u8; 64])))
    );

    // One byte over it is not.
    client
        .send(Message::Binary(Bytes::from(vec![7_u8; 65])))
        .await
        .unwrap();
    let Some(Message::Close(Some(frame))) = recv(&mut client).await else {
        panic!("an oversized frame closes the socket rather than being forwarded");
    };
    assert_eq!(frame.code, CloseCode::Size, "1009 Message Too Big");
    assert_eq!(
        upstream.next().await,
        Some(WsFrame::Close(Some(WsClose {
            code: 1009,
            reason: "client frame exceeds the frame limit".into()
        }))),
        "the upstream is closed too, rather than left paying for a dead session"
    );
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
/// task; downstream capture is written after completion, including unmetered sockets.
async fn settled(host: &Host) {
    for _ in 0..100 {
        if records(host)
            .await
            .iter()
            .any(|row| row.ended_at_ms.is_some())
        {
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
        .downstream_records()
        .query(capture_record::Entity::find())
        .await
        .unwrap()
}

async fn downstream_record(host: &Host) -> capture_record::Model {
    let mut rows = records(host).await;
    assert_eq!(rows.len(), 1, "one socket is one downstream record");
    rows.remove(0)
}

async fn events(
    host: &Host,
    capture_id: &str,
) -> Vec<gproxy_store::entity::usage::capture_event::Model> {
    let rows = host
        .app
        .gproxy()
        .store()
        .downstream_events()
        .query(
            capture_event::Entity::find()
                .filter(capture_event::Column::CaptureId.eq(capture_id))
                .order_by_asc(capture_event::Column::Sequence),
        )
        .await
        .unwrap();
    host.app
        .gproxy()
        .store()
        .hydrate_capture_events(rows.into_iter().map(Into::into).collect())
        .await
        .unwrap()
}
