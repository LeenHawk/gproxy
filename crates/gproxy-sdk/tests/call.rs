//! The call entry: what reaches core, what fails over, and what stops.

mod support;

use std::{collections::BTreeSet, sync::Arc};

use gproxy_core::{BudgetOwner, SessionSource, UsageAttribution};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest,
    connection::{Bytes, HeaderMap, HeaderValue, Method},
};
use gproxy_sdk::{GATEWAY_SESSION_HEADER, SdkError};
use gproxy_store::entity::usage::usage_record;
use http::StatusCode;
use sea_orm::EntityTrait;
use serde_json::json;
use support::seed::{self, Handle, Reply, SeedClient, SeedObserver, WsReply};

fn generate() -> OperationKey {
    OperationKey {
        operation: Operation::GenerateContent,
        dialect: Dialect::OpenAi,
    }
}

fn request(body: serde_json::Value) -> WireRequest<HttpBody> {
    WireRequest {
        method: Method::POST,
        path: "/v1/responses".into(),
        query: None,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&body).unwrap())),
    }
}

fn with_headers(body: serde_json::Value, pairs: &[(&'static str, &str)]) -> WireRequest<HttpBody> {
    let mut request = request(body);
    for (name, value) in pairs {
        request
            .headers
            .insert(*name, HeaderValue::from_str(value).unwrap());
    }
    request
}

/// Two providers of the `test` channel, both serving `m1`, behind one
/// failover route exposed as `pair`.
async fn pair() -> (Handle, Arc<SeedClient>, Arc<SeedObserver>) {
    let (gproxy, client, observer) = seed::handle().await;
    for id in ["p1", "p2"] {
        seed::provider(&gproxy, id, "test", &["m1"]).await;
        seed::credential(&gproxy, &format!("c-{id}"), id).await;
    }
    seed::route(
        &gproxy,
        "r",
        "pair",
        gproxy_store::entity::routing::route::RouteStrategy::Failover,
        4,
        &[("m-p1", "p1", "m1", 0, 100), ("m-p2", "p2", "m1", 0, 100)],
    )
    .await;
    seed::publish(&gproxy).await;
    (gproxy, client, observer)
}

/// Drain the response and its settlement, which is what makes the usage row
/// appear; a host that abandons the body gets the same settlement later.
async fn finish(execution: gproxy_core::HttpExecution) -> String {
    let (response, usage) = execution.into_parts();
    let body = support::read(response.body).await;
    usage.await.unwrap();
    body
}

async fn usage_rows(gproxy: &Handle) -> Vec<usage_record::Model> {
    gproxy
        .store()
        .usage_records()
        .query(usage_record::Entity::find())
        .await
        .unwrap()
}

#[tokio::test]
async fn a_scripted_answer_comes_back_and_is_metered() {
    let (gproxy, client, _) = pair().await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);

    let execution = gproxy
        .call(generate(), request(json!({"model": "pair"})))
        .scope("user:u1")
        .attribution(UsageAttribution {
            user_id: Some("u1".into()),
            ..Default::default()
        })
        .request_id("req-1")
        .send()
        .await
        .unwrap();
    assert_eq!(execution.response().status, StatusCode::OK);
    assert_eq!(finish(execution).await, r#"{"ok":true}"#);

    assert_eq!(client.urls(), ["https://p1.example/v1/responses"]);
    let rows = usage_rows(&gproxy).await;
    assert_eq!(rows.len(), 1, "one physical call, one usage row");
    assert!(
        gproxy
            .store()
            .capture_links()
            .get_many(&[("req-1".into(), rows[0].request_id.clone())])
            .await
            .unwrap()[0]
            .is_some()
    );
    assert_eq!(rows[0].user_id.as_deref(), Some("u1"));
    assert_eq!(
        rows[0].model, "m1",
        "usage records the actual upstream model"
    );
}

#[tokio::test]
async fn a_rejected_provider_hands_over_to_the_next_one() {
    let (gproxy, client, _) = pair().await;
    client.script(vec![
        Reply::Http(StatusCode::SERVICE_UNAVAILABLE, json!({"error": "down"})),
        Reply::Http(StatusCode::OK, json!({"from": "p2"})),
    ]);

    let execution = gproxy
        .call(generate(), request(json!({"model": "pair"})))
        .scope("user:u1")
        .send()
        .await
        .unwrap();
    assert_eq!(execution.response().status, StatusCode::OK);
    assert_eq!(finish(execution).await, r#"{"from":"p2"}"#);
    assert_eq!(
        client.urls(),
        [
            "https://p1.example/v1/responses",
            "https://p2.example/v1/responses"
        ],
        "both providers were tried, in plan order"
    );
}

#[tokio::test]
async fn the_last_provider_s_answer_is_returned_as_it_is() {
    let (gproxy, client, _) = pair().await;
    client.script(vec![
        Reply::Http(StatusCode::SERVICE_UNAVAILABLE, json!({"error": "a"})),
        Reply::Http(StatusCode::TOO_MANY_REQUESTS, json!({"error": "b"})),
    ]);

    let execution = gproxy
        .call(generate(), request(json!({"model": "pair"})))
        .scope("user:u1")
        .send()
        .await
        .unwrap();
    assert_eq!(
        execution.response().status,
        StatusCode::TOO_MANY_REQUESTS,
        "with nowhere left to go the caller gets the upstream's own answer"
    );
    assert_eq!(finish(execution).await, r#"{"error":"b"}"#);
}

#[tokio::test]
async fn every_provider_failing_returns_the_last_error() {
    let (gproxy, client, _) = pair().await;
    client.script(vec![
        Reply::Transport("p1 unreachable"),
        Reply::Transport("p2 unreachable"),
    ]);

    let error = gproxy
        .call(generate(), request(json!({"model": "pair"})))
        .scope("user:u1")
        .send()
        .await
        .expect_err("nothing answered");
    assert!(
        error.to_string().contains("p2 unreachable"),
        "the last provider's failure is the one reported: {error}"
    );
    assert_eq!(client.urls().len(), 2);
}

#[tokio::test]
async fn a_spent_budget_stops_before_the_first_provider() {
    let (gproxy, client, _) = pair().await;
    seed::spent_budget(&gproxy, "q", "user", "u1").await;
    seed::publish(&gproxy).await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);

    let error = gproxy
        .call(generate(), request(json!({"model": "pair"})))
        .scope("user:u1")
        .budgets(vec![BudgetOwner::new("user", "u1")])
        .send()
        .await
        .expect_err("the budget is spent");
    assert!(
        matches!(
            error,
            SdkError::Core(gproxy_core::CoreError::BudgetExhausted { .. })
        ),
        "{error}"
    );
    assert_eq!(error.status_code(), 429);
    assert!(
        client.urls().is_empty(),
        "a spent budget is not retried against another provider"
    );
}

#[tokio::test]
async fn the_gateway_session_wins_and_is_not_forwarded() {
    let (gproxy, client, observer) = pair().await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);

    let execution = gproxy
        .call(
            generate(),
            with_headers(
                json!({"model": "pair", "client_metadata": {"thread_id": "t-body"}}),
                &[(GATEWAY_SESSION_HEADER, "ours"), ("thread-id", "t-header")],
            ),
        )
        .scope("user:u1")
        .send()
        .await
        .unwrap();
    finish(execution).await;

    let seen = observer.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].1.id, "ours");
    assert_eq!(seen[0].1.source, SessionSource::Gateway);

    let sent = client.seen.lines();
    assert!(
        sent[0].contains("gateway=-"),
        "the gateway header is ours and never reaches the upstream: {}",
        sent[0]
    );
}

#[tokio::test]
async fn the_claude_encoded_user_id_yields_the_inner_session() {
    let (gproxy, client, observer) = pair().await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);
    let encoded = json!({"account_uuid": "acct", "session_id": "sess-42"}).to_string();

    let execution = gproxy
        .call(
            OperationKey {
                operation: Operation::GenerateContent,
                dialect: Dialect::Claude,
            },
            request(json!({"model": "pair", "metadata": {"user_id": encoded}})),
        )
        .scope("user:u1")
        .send()
        .await
        .unwrap();
    finish(execution).await;

    let seen = observer.seen();
    assert_eq!(seen[0].1.id, "sess-42");
    assert_eq!(seen[0].1.source, SessionSource::ClaudeCode);
    assert_eq!(
        seen[0].1.field.as_deref(),
        Some("metadata.user_id.session_id")
    );
}

#[tokio::test]
async fn a_request_without_a_session_gets_its_own_id_and_says_so() {
    let (gproxy, client, observer) = pair().await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);

    let execution = gproxy
        .call(generate(), request(json!({"model": "pair"})))
        .scope("user:u1")
        .request_id("req-7")
        .send()
        .await
        .unwrap();
    finish(execution).await;

    let seen = observer.seen();
    assert_eq!(seen[0].1.id, "req-7");
    assert_eq!(
        seen[0].1.source,
        SessionSource::RequestFallback,
        "a request id is never claimed to be a cross-turn session"
    );
}

#[tokio::test]
async fn a_call_without_a_scope_is_refused_before_anything_is_sent() {
    let (gproxy, client, _) = pair().await;
    let error = gproxy
        .call(generate(), request(json!({"model": "pair"})))
        .send()
        .await
        .expect_err("there is no safe default scope");
    assert!(matches!(error, SdkError::Invalid(_)), "{error}");
    assert_eq!(error.status_code(), 400);
    assert!(client.urls().is_empty());
}

#[tokio::test]
async fn the_narrowing_setters_reach_the_plan() {
    let (gproxy, client, _) = pair().await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);

    // p1 would come first; allowing only p2's credential leaves p2 alone.
    let credentials: BTreeSet<String> = ["c-p2".to_owned()].into();
    let execution = gproxy
        .call(generate(), request(json!({"model": "pair"})))
        .scope("user:u1")
        .credentials(credentials)
        .send()
        .await
        .unwrap();
    finish(execution).await;
    assert_eq!(client.urls(), ["https://p2.example/v1/responses"]);

    // An allowance that reaches nothing is a plan with no target.
    let error = gproxy
        .call(generate(), request(json!({"model": "pair"})))
        .scope("user:u1")
        .providers(BTreeSet::new())
        .send()
        .await
        .expect_err("no provider is allowed");
    assert!(matches!(error, SdkError::NoTarget(_)), "{error}");
}

#[tokio::test]
async fn an_explicit_model_overrides_the_body() {
    let (gproxy, client, _) = pair().await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok": true}))]);

    // The body names a route that does not exist; the setter names one that does.
    let execution = gproxy
        .call(generate(), request(json!({"model": "nonsense"})))
        .scope("user:u1")
        .model("test/m1")
        .request_id("req-9")
        .send()
        .await
        .unwrap();
    finish(execution).await;
    assert_eq!(usage_rows(&gproxy).await[0].model, "m1");
}

#[tokio::test]
async fn the_attempt_budget_is_shared_across_targets() {
    let (gproxy, client, _) = pair().await;
    client.script(vec![
        Reply::Http(StatusCode::SERVICE_UNAVAILABLE, json!({"error": "a"})),
        Reply::Http(StatusCode::SERVICE_UNAVAILABLE, json!({"error": "b"})),
    ]);

    // One attempt in total: the first target spends it and the second is
    // never reached, so its answer is the one the caller sees.
    let execution = gproxy
        .call(generate(), request(json!({"model": "pair"})))
        .scope("user:u1")
        .max_attempts(std::num::NonZeroU32::new(1).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(execution.response().status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(finish(execution).await, r#"{"error":"a"}"#);
    assert_eq!(client.urls(), ["https://p1.example/v1/responses"]);
}

#[tokio::test]
async fn a_refused_handshake_hands_over_to_the_next_provider() {
    let (gproxy, client, _) = seed::handle().await;
    for id in ["w1", "w2"] {
        seed::provider(&gproxy, id, "alt", &["m1"]).await;
        seed::credential(&gproxy, &format!("c-{id}"), id).await;
    }
    seed::route(
        &gproxy,
        "r",
        "sockets",
        gproxy_store::entity::routing::route::RouteStrategy::Failover,
        4,
        &[("m-w1", "w1", "m1", 0, 100), ("m-w2", "w2", "m1", 0, 100)],
    )
    .await;
    seed::publish(&gproxy).await;
    client.script_ws(vec![
        WsReply::Rejected(StatusCode::SERVICE_UNAVAILABLE),
        WsReply::Connected,
    ]);

    let execution = gproxy
        .connect(
            OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: Dialect::OpenAiResponsesWebSocket,
            },
            WireRequest {
                method: Method::GET,
                path: "/v1/realtime".into(),
                query: None,
                headers: HeaderMap::new(),
                body: (),
            },
        )
        .scope("user:u1")
        .model("sockets")
        .send()
        .await
        .unwrap();
    assert!(
        matches!(
            execution.response(),
            gproxy_protocol::capability::UpstreamConnection::Connected { .. }
        ),
        "a refused upgrade is a failed status like any other"
    );
    assert_eq!(
        client.urls(),
        [
            "wss://w1.example/v1/realtime",
            "wss://w2.example/v1/realtime"
        ]
    );
}

#[tokio::test]
async fn buffered_stream_keepalive_preserves_provider_failover_and_settlement() {
    use gproxy_store::entity::upstream::operation_rule;
    use sea_orm::Set;
    let (gproxy, client, _) = pair().await;
    for provider_id in ["p1", "p2"] {
        gproxy.store().operation_rules().create_many(vec![operation_rule::ActiveModel {
            id: Set(format!("buffered-{provider_id}")),
            provider_id: Set(provider_id.into()),
            operation: Set("stream_generate_content".into()),
            action: Set("routing".into()),
            target: Set(Some(json!({"openai":{"implementation":"transform_to", "target":{"operation":"generate_content", "dialect":"openai"}}}))),
        }]).await.unwrap();
    }
    seed::publish(&gproxy).await;
    client.script(vec![
        Reply::DelayedHttp(std::time::Duration::from_secs(20), StatusCode::SERVICE_UNAVAILABLE, json!({"error":{"message":"try next"}})),
        Reply::Http(StatusCode::OK, json!({
            "id":"resp_answer", "created_at":1, "model":"m1", "object":"response", "status":"completed",
            "error":null, "incomplete_details":null, "instructions":null, "metadata":null,
            "parallel_tool_calls":true, "temperature":null, "top_p":null, "tools":[], "tool_choice":"auto",
            "output":[{"type":"message","id":"msg_answer","role":"assistant","status":"completed","content":[{"type":"output_text","text":"hello","annotations":[],"logprobs":[]}]}],
            "usage":{"input_tokens":4,"output_tokens":2,"total_tokens":6,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"output_tokens_details":{"reasoning_tokens":0}}
        })),
    ]);
    tokio::time::pause();
    let result = gproxy
        .call(
            OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: Dialect::OpenAi,
            },
            request(json!({"model":"pair","stream":true,"input":"hi"})),
        )
        .scope("tenant")
        .send()
        .await
        .unwrap();
    assert_eq!(
        result.response().headers["content-type"],
        "text/event-stream"
    );
    let (response, completion) = result.into_parts();
    let body = support::read(response.body).await;
    assert!(body.starts_with(":\n\n"), "{body}");
    assert!(
        body.contains("response.completed") && body.contains("hello"),
        "{body}"
    );
    assert!(
        !body.contains("event: error"),
        "the second provider recovers the failure: {body}"
    );
    assert_eq!(
        completion.await.unwrap().state,
        gproxy_core::UsageState::Completed
    );
    assert_eq!(client.urls().len(), 2);
    assert!(
        client
            .seen
            .lines()
            .iter()
            .all(|line| line.contains("\"stream\":false"))
    );
}

#[tokio::test]
async fn managed_reset_strategy_and_affinity_reach_core_for_http_and_websocket() {
    use gproxy_sdk::dto::{CredentialPatch, ProviderPatch};
    use gproxy_store::entity::limits::credential_cycle::{self, CycleBoundary, CycleOpening};
    use sea_orm::Set;
    let (gproxy, client, observer) = seed::handle().await;
    seed::provider(&gproxy, "p", "alt", &["m1"]).await;
    for id in ["a", "b"] {
        seed::credential(&gproxy, id, "p").await;
    }
    seed::publish(&gproxy).await;
    let config = json!({"credential_strategy":"earliest_reset", "session_affinity":true});
    let provider = gproxy
        .manage()
        .providers()
        .update(
            "p",
            ProviderPatch {
                config: Some(config.clone()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(provider.config, config);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    for (id, delay) in [("a", 3_600_000), ("b", 600_000)] {
        gproxy
            .manage()
            .credentials()
            .update(
                id,
                CredentialPatch {
                    metadata: Some(json!({"observed_quota":true})),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        gproxy
            .store()
            .credential_cycles()
            .create_many(vec![credential_cycle::ActiveModel {
                id: Set(format!("cycle-{id}")),
                credential_id: Set(id.into()),
                open_key: Set(Some(credential_cycle::open_key(id, "5h"))),
                window_id: Set("5h".into()),
                dimension_id: Set(Some("5h".into())),
                scope: Set(json!("all")),
                starts_at_ms: Set(now),
                ends_at_ms: Set(Some(now + delay)),
                boundary: Set(CycleBoundary::Observed),
                opened_by: Set(CycleOpening::FirstUse),
                cost_usd: Set(gproxy_store::FixedDecimal::ZERO),
                sample_at_ms: Set(Some(now)),
                ..Default::default()
            }])
            .await
            .unwrap();
    }
    // Explicitly reload as a host using manual sync would do.
    seed::publish(&gproxy).await;
    assert!(gproxy.core().snapshot().providers["p"].session_affinity);
    client.script(vec![Reply::Http(StatusCode::OK, json!({"ok":true}))]);
    finish(
        gproxy
            .call(
                generate(),
                with_headers(
                    json!({"model":"alt/m1", "input":"hello"}),
                    &[("x-opencode-session", "s")],
                ),
            )
            .scope("user:u1")
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(client.authorizations.lock().unwrap()[0], "Bearer k-b");
    assert_eq!(observer.seen()[0].1.source, SessionSource::OpenCode);

    client.script_ws(vec![WsReply::Connected]);
    let execution = gproxy
        .connect(
            OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: Dialect::OpenAiResponsesWebSocket,
            },
            WireRequest {
                method: Method::GET,
                path: "/v1/responses".into(),
                query: None,
                headers: HeaderMap::new(),
                body: (),
            },
        )
        .scope("user:u1")
        .model("alt/m1")
        .session(gproxy_core::SessionIdentity {
            id: "s".into(),
            source: SessionSource::OpenCode,
            field: None,
            agent_session_id: None,
        })
        .send()
        .await
        .unwrap();
    assert!(matches!(
        execution.response(),
        gproxy_protocol::capability::UpstreamConnection::Connected { .. }
    ));
    assert_eq!(client.authorizations.lock().unwrap()[1], "Bearer k-b");
    assert!(
        observer
            .seen()
            .iter()
            .all(|(_, s)| s.id == "s" && s.source == SessionSource::OpenCode)
    );
}

#[tokio::test]
async fn ordinary_responses_connect_uses_managed_warmup_and_controls() {
    use futures_util::{SinkExt, StreamExt};
    use gproxy_protocol::{
        capability::UpstreamConnection,
        connection::{WsFrame, WsReceiver},
    };
    let (gproxy, client, _) = seed::handle().await;
    seed::provider(&gproxy, "p", "test", &["m1"]).await;
    seed::credential(&gproxy, "c", "p").await;
    seed::publish(&gproxy).await;
    client.script(vec![
        Reply::DelayedHttp(
            std::time::Duration::from_secs(60),
            StatusCode::OK,
            json!({}),
        ),
        Reply::DelayedHttp(
            std::time::Duration::from_secs(60),
            StatusCode::OK,
            json!({}),
        ),
    ]);
    let execution = gproxy
        .connect(
            OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: Dialect::OpenAiResponsesWebSocket,
            },
            WireRequest {
                method: Method::GET,
                path: "/v1/responses".into(),
                query: None,
                headers: HeaderMap::new(),
                body: (),
            },
        )
        .scope("user:u1")
        .model("p/m1")
        .send()
        .await
        .unwrap();
    let (connection, completion) = execution.into_parts();
    let UpstreamConnection::Connected { mut socket, .. } = connection else {
        panic!("connection rejected")
    };
    let next = async |incoming: &mut WsReceiver| {
        let WsFrame::Text(text) =
            tokio::time::timeout(std::time::Duration::from_secs(5), incoming.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap()
        else {
            panic!("expected event")
        };
        serde_json::from_str::<serde_json::Value>(&text).unwrap()
    };
    socket.outgoing.send(WsFrame::Text(json!({"type":"response.create","model":"p/m1","input":"warmup","generate":false,"store":false}).to_string())).await.unwrap();
    assert_eq!(next(&mut socket.incoming).await["type"], "response.created");
    let warmup = next(&mut socket.incoming).await;
    assert_eq!(warmup["type"], "response.completed");
    assert!(client.urls().is_empty());
    socket.outgoing.send(WsFrame::Text(json!({"type":"response.create","model":"p/m1","input":"continue","previous_response_id":warmup["response"]["id"],"store":false}).to_string())).await.unwrap();
    let current = next(&mut socket.incoming).await;
    assert_eq!(current["type"], "response.created");
    socket.outgoing.send(WsFrame::Text(json!({"type":"response.inject","response_id":current["response"]["id"],"input":[{"role":"user","content":"extra"}]}).to_string())).await.unwrap();
    assert_eq!(
        next(&mut socket.incoming).await["type"],
        "response.inject.created"
    );
    socket.outgoing.send(WsFrame::Text(json!({"type":"response.steer","previous_response_id":current["response"]["id"],"input":"changed"}).to_string())).await.unwrap();
    assert_eq!(
        next(&mut socket.incoming).await["type"],
        "response.steer.accepted"
    );
    assert_eq!(
        next(&mut socket.incoming).await["type"],
        "response.incomplete"
    );
    let successor = next(&mut socket.incoming).await;
    assert_eq!(successor["type"], "response.created");
    socket.outgoing.send(WsFrame::Text(json!({"type":"response.interrupt","response_id":successor["response"]["id"],"mode":"discard_partial_items"}).to_string())).await.unwrap();
    assert_eq!(
        next(&mut socket.incoming).await["type"],
        "response.interrupt.accepted"
    );
    assert_eq!(
        next(&mut socket.incoming).await["type"],
        "response.incomplete"
    );
    socket.outgoing.send(WsFrame::Close(None)).await.unwrap();
    while socket.incoming.next().await.is_some() {}
    completion.await.unwrap();
}

/// Record `id` as a file `scope` uploaded on `credential` of `provider`, as
/// core does after a successful create (`gproxy_core::owned`).
async fn own_file(gproxy: &Handle, scope: &str, provider: &str, credential: &str, id: &str) {
    use gproxy_store::entity::resource::resource_binding;
    use sea_orm::ActiveValue::Set;
    gproxy
        .store()
        .resource_bindings()
        .create_many(vec![resource_binding::ActiveModel {
            id: Set(format!("own-{id}")),
            scope: Set(format!("{scope}\u{1f}{provider}")),
            kind: Set(gproxy_core::owned::FILE_KIND.into()),
            public_id: Set(id.into()),
            generation: Set(0),
            assignment_id: Set(None),
            provider_id: Set(provider.into()),
            upstream_id: Set(Some(id.into())),
            user_id: Set(None),
            credential_id: Set(credential.into()),
            parent_binding_id: Set(None),
            secret: Set(None),
            summary: Set(json!({})),
            file_id: Set(None),
            created_at_ms: Set(0),
            updated_at_ms: Set(0),
            expires_at_ms: Set(None),
        }])
        .await
        .unwrap();
}

fn retrieve_file(id: &str) -> (OperationKey, WireRequest<HttpBody>) {
    (
        OperationKey {
            operation: Operation::RetrieveFile,
            dialect: Dialect::OpenAi,
        },
        WireRequest {
            method: Method::GET,
            path: format!("/v1/files/{id}"),
            query: None,
            headers: HeaderMap::new(),
            body: HttpBody::Bytes(Bytes::new()),
        },
    )
}

#[tokio::test]
async fn a_file_read_goes_straight_to_the_provider_holding_the_callers_file() {
    let (gproxy, client, _) = pair().await;
    own_file(&gproxy, "user:u1", "p2", "c-p2", "file-1").await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"id": "file-1"}))]);

    // One attempt in total: without narrowing, p1 would spend it on a
    // not-found and p2 would never be asked.
    let (key, request) = retrieve_file("file-1");
    let execution = gproxy
        .call(key, request)
        .scope("user:u1")
        .max_attempts(std::num::NonZeroU32::new(1).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(execution.response().status, StatusCode::OK);
    finish(execution).await;
    assert_eq!(client.urls(), ["https://p2.example/v1/files/file-1"]);

    // Another scope owns nothing anywhere: not found, nothing sent.
    let (key, request) = retrieve_file("file-1");
    let Err(error) = gproxy.call(key, request).scope("user:u2").send().await else {
        panic!("another scope must not reach the file")
    };
    assert_eq!(error.status_code(), 404, "{error}");
    assert_eq!(client.urls().len(), 1);
}

#[tokio::test]
async fn a_generation_referencing_a_file_goes_to_the_provider_holding_it() {
    let (gproxy, client, _) = pair().await;
    own_file(&gproxy, "user:u1", "p2", "c-p2", "file-9").await;
    client.script(vec![Reply::Http(StatusCode::OK, json!({"from": "p2"}))]);
    let body = json!({"model": "pair", "input": [{"role": "user", "content": [
        {"type": "input_file", "file_id": "file-9"}
    ]}]});

    // One attempt in total, and p1 is the failover route's first member.
    let execution = gproxy
        .call(generate(), request(body.clone()))
        .scope("user:u1")
        .max_attempts(std::num::NonZeroU32::new(1).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(finish(execution).await, r#"{"from":"p2"}"#);
    assert_eq!(client.urls(), ["https://p2.example/v1/responses"]);

    let Err(error) = gproxy
        .call(generate(), request(body))
        .scope("user:u2")
        .send()
        .await
    else {
        panic!("another scope must not use the file")
    };
    assert_eq!(error.status_code(), 404, "{error}");
    assert_eq!(client.urls().len(), 1);
}
