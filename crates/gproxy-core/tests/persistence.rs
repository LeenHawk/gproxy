#![cfg(not(target_arch = "wasm32"))]

//! Real SQLite verification of capture/usage persistence, including abandoned
//! execution futures and response bodies. No live upstream is required.
mod support;
use futures_util::StreamExt;
use gproxy_channel::OutboundClient;
use gproxy_core::{Core, PlaintextCodec, RequestContext, UsageState};
use gproxy_protocol::{
    HttpBody, WireResponse,
    capability::{CapabilityError, CapabilityFuture, UpstreamConnection},
    connection::Bytes,
};
use gproxy_store::entity::{
    config::setting,
    usage::{capture_event as event, capture_record as capture, usage_record},
};
use http::StatusCode;
use sea_orm::{EntityTrait, Set};
use serde_json::json;
use std::{sync::Arc, time::Duration};
use support::*;

async fn persistent() -> Harness {
    let mut h = harness(full(), "round_robin").await;
    h.core = Core::builder(h.core.store().clone())
        .cache(h.core.cache().clone())
        .secret_codec(Arc::new(PlaintextCodec))
        .channel(h.channel.clone())
        .unwrap()
        .snapshot(h.core.snapshot())
        .build()
        .unwrap(); // Deliberately no custom Observer.
    h.core
        .store()
        .provider_rewrite_rule_sets()
        .delete_many(&["bind".into()])
        .await
        .unwrap();
    settings(&h, true, true, true, true).await;
    h
}
async fn settings(h: &Harness, settlement: bool, usage: bool, log: bool, body: bool) {
    h.core
        .store()
        .settings()
        .update(setting::ActiveModel {
            config_revision: Set((h.core.snapshot().revision.0 + 1) as i64),
            enable_settlement: Set(settlement),
            enable_usage: Set(usage),
            enable_upstream_log: Set(log),
            enable_upstream_log_body: Set(body),
            ..Default::default()
        })
        .await
        .unwrap();
    h.core.reload_data().await.unwrap();
}
async fn captures(h: &Harness) -> Vec<capture::Model> {
    h.core
        .store()
        .capture_records()
        .query(capture::Entity::find())
        .await
        .unwrap()
}
async fn usages(h: &Harness) -> Vec<usage_record::Model> {
    h.core
        .store()
        .usage_records()
        .query(usage_record::Entity::find())
        .await
        .unwrap()
}
async fn events(h: &Harness) -> Vec<event::Model> {
    h.core
        .store()
        .capture_events()
        .query(event::Entity::find())
        .await
        .unwrap()
}
async fn run(h: &Harness, id: &str, attempts: u32) -> gproxy_core::UsageReport {
    let execution = h
        .core
        .stream_generate_content(h.context(id, attempts, None), request("{}"))
        .await
        .unwrap();
    let (response, usage) = execution.into_parts();
    read(response.body).await;
    usage.await.unwrap()
}

#[tokio::test]
async fn retries_keep_both_bodies_and_usage_but_write_one_request_summary() {
    let h = persistent().await;
    h.script(vec![
        json_reply(
            StatusCode::TOO_MANY_REQUESTS,
            json!({"error": "retry", "usage": {"input_tokens": 2, "output_tokens": 1}}),
        ),
        json_reply(
            StatusCode::OK,
            json!({"ok": true, "usage": {"input_tokens": 4, "output_tokens": 3}}),
        ),
    ]);
    let mut ctx = (*h.context("retry", 2, None)).clone();
    ctx.attribution.user_id = Some("historical-user".into());
    ctx.attribution.api_key_id = Some("historical-key".into());
    ctx.attribution.model = Some("public-alias".into());
    let execution = h
        .core
        .stream_generate_content(Arc::new(ctx), request("{}"))
        .await
        .unwrap();
    let (response, completion) = execution.into_parts();
    read(response.body).await;
    let report = completion.await.unwrap();
    assert_eq!(report.exchanges.len(), 2);
    let mut rows = captures(&h).await;
    rows.sort_by_key(|r| r.attempt_ordinal);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].attempt_id.as_deref(), Some("retry-1"));
    assert_eq!(rows[1].attempt_id.as_deref(), Some("retry-2"));
    assert_ne!(rows[0].credential_id, rows[1].credential_id);
    assert_eq!(rows[0].response_status, Some(429));
    assert_eq!(rows[0].state, capture::CaptureState::Failed);
    assert_eq!(
        rows[0].response_body_state,
        capture::CaptureBodyState::Complete
    );
    assert_eq!(
        rows[0].metrics.as_ref().unwrap()["tokens"]["input_tokens"],
        2
    );
    assert_eq!(rows[1].state, capture::CaptureState::Completed);
    let ev = events(&h).await;
    for row in &rows {
        assert_eq!(row.initiator_request_id.as_deref(), Some("retry"));
        assert!(ev.iter().any(|e| e.capture_id == row.id && e.direction == event::CaptureDirection::Response));
        assert!(
            row.request_headers
                .as_ref()
                .unwrap()
                .to_string()
                .contains("[redacted]")
        );
        assert!(
            !row.request_headers
                .as_ref()
                .unwrap()
                .to_string()
                .contains("Bearer")
        );
    }
    let usage = usages(&h).await;
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].user_id.as_deref(), Some("historical-user"));
    assert_eq!(usage[0].model, "public-alias");
    assert_eq!(usage[0].metrics["tokens"]["input_tokens"], 6);
    assert_eq!(usage[0].metrics["tokens"]["output_tokens"], 4);
    assert_eq!(usage[0].metrics["exchanges"].as_array().unwrap().len(), 2);
    assert_eq!(usage[0].metrics["state"], "completed");
}

#[tokio::test]
async fn dropped_response_preserves_received_bytes_and_partial_usage() {
    let h = persistent().await;
    h.script(vec![(
        StatusCode::OK,
        vec![("content-type", "application/json")],
        vec![
            Bytes::from_static(br#"{"usage":{"input_tokens":3,"output_tokens":1}}"#),
            Bytes::from_static(b"never read"),
        ],
    )]);
    let execution = h
        .core
        .stream_generate_content(h.context("drop-body", 1, None), request("{}"))
        .await
        .unwrap();
    let (response, completion) = execution.into_parts();
    let HttpBody::Stream(mut body) = response.body else {
        panic!("observed stream")
    };
    body.next().await.unwrap().unwrap();
    drop(body);
    let report = tokio::time::timeout(Duration::from_secs(3), completion)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(report.state, UsageState::Cancelled);
    assert_eq!(report.exchanges[0].usage.tokens.input_tokens, Some(3));
    let rows = captures(&h).await;
    assert_eq!(rows[0].state, capture::CaptureState::Cancelled);
    assert_eq!(
        rows[0].response_body_state,
        capture::CaptureBodyState::Partial
    );
    assert_eq!(
        rows[0].request_body_state,
        capture::CaptureBodyState::Complete
    );
    let ev = events(&h).await;
    assert!(ev.iter().any(|e| e.payload.starts_with(b"{\"usage\"")));
    assert!(!ev.iter().any(|e| e.payload == b"never read"));
    assert_eq!(usages(&h).await[0].metrics["state"], "cancelled");
}

struct PendingClient(Arc<tokio::sync::Notify>);
impl OutboundClient for PendingClient {
    fn send<'a>(
        &'a self,
        _: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse, CapabilityError>> {
        Box::pin(async move {
            self.0.notify_one();
            std::future::pending().await
        })
    }
    fn connect<'a>(
        &'a self,
        _: http::Request<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(std::future::pending())
    }
}
fn replace_client(ctx: &mut Arc<RequestContext>, client: Arc<dyn OutboundClient>) {
    for credential in &mut Arc::get_mut(ctx).unwrap().target.credentials {
        let credential = Arc::get_mut(credential).unwrap();
        credential.client = client.clone();
        credential.websocket_client = client.clone();
    }
}
#[tokio::test]
async fn dropping_send_future_before_headers_still_finalizes_records() {
    let h = persistent().await;
    let started = Arc::new(tokio::sync::Notify::new());
    let mut ctx = h.context("drop-send", 1, None);
    replace_client(&mut ctx, Arc::new(PendingClient(started.clone())));
    let mut call = Box::pin(h.core.stream_generate_content(ctx, request("{}")));
    tokio::select! {
        _ = &mut call => panic!("pending upstream"),
        _ = started.notified() => {}
    }
    drop(call);
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if usages(&h).await.len() == 1
                && captures(&h)
                    .await
                    .first()
                    .is_some_and(|r| r.ended_at_ms.is_some())
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let rows = captures(&h).await;
    assert_eq!(rows[0].state, capture::CaptureState::Cancelled);
    assert_eq!(rows[0].response_status, None);
    assert_eq!(
        rows[0].response_body_state,
        capture::CaptureBodyState::Partial
    );
    let usage = usages(&h).await;
    assert_eq!(usage[0].metrics["state"], "cancelled");
    assert!(usage[0].metrics["tokens"]["input_tokens"].is_null());
}

#[tokio::test]
async fn settings_separate_retention_settlement_and_capture_body() {
    let h = persistent().await;
    h.core
        .store()
        .price_rules()
        .create_many(vec![
            gproxy_store::entity::pricing::price_rule::ActiveModel {
                id: Set("price".into()),
                model_pattern: Set("gpt-*".into()),
                currency: Set("USD".into()),
                ..Default::default()
            },
        ])
        .await
        .unwrap();
    h.core
        .store()
        .price_rates()
        .create_many(vec![
            gproxy_store::entity::pricing::price_rate::ActiveModel {
                id: Set("rate".into()),
                price_rule_id: Set("price".into()),
                metric: Set("input_tokens".into()),
                unit: Set(gproxy_store::entity::pricing::price_unit::PriceUnit::Token),
                unit_quantity: Set("1".parse().unwrap()),
                value: Set("1".parse().unwrap()),
                ..Default::default()
            },
        ])
        .await
        .unwrap();
    let reply = || {
        json_reply(
            StatusCode::OK,
            json!({"usage": {"input_tokens": 2, "output_tokens": 1}}),
        )
    };
    settings(&h, true, false, false, true).await;
    h.script(vec![reply()]);
    let report = run(&h, "settle-only", 1).await;
    assert!(report.cost.is_some());
    assert!(usages(&h).await.is_empty());
    assert!(captures(&h).await.is_empty());

    settings(&h, false, true, true, false).await;
    h.script(vec![reply()]);
    let report = run(&h, "record-only", 1).await;
    assert!(report.cost.is_none());
    assert_eq!(report.exchanges[0].usage.tokens.input_tokens, Some(2));
    assert_eq!(usages(&h).await.len(), 1);
    assert_eq!(
        captures(&h).await[0].response_body_state,
        capture::CaptureBodyState::NotCaptured
    );
    assert!(events(&h).await.is_empty());

    settings(&h, false, false, false, false).await;
    assert!(h.core.snapshot().estimation.is_none());
    h.script(vec![reply()]);
    assert_eq!(run(&h, "off", 1).await.state, UsageState::Skipped);
    assert_eq!(usages(&h).await.len(), 1);
    assert_eq!(captures(&h).await.len(), 1);
}

#[tokio::test]
async fn websocket_frames_and_handshake_are_persisted_without_double_usage() {
    use gproxy_protocol::{Dialect, Operation, OperationKey, connection::WsFrame};
    let h = persistent().await;
    h.script_ws(vec![WsReply::Connected(vec![
        WsFrame::Text("hello".into()),
        WsFrame::Close(None),
    ])]);
    let key = OperationKey {
        operation: Operation::ConnectRealtime,
        dialect: Dialect::OpenAi,
    };
    let ctx = h.context_for("p", key, "ws", 1, None);
    let execution = h
        .core
        .connect_realtime(
            ctx,
            gproxy_protocol::WireRequest {
                method: http::Method::GET,
                path: "/v1/realtime".into(),
                query: None,
                headers: http::HeaderMap::new(),
                body: (),
            },
        )
        .await
        .unwrap();
    let (connection, completion) = execution.into_parts();
    let UpstreamConnection::Connected { mut socket, .. } = connection else {
        panic!("connected")
    };
    while let Some(frame) = socket.incoming.next().await {
        frame.unwrap();
    }
    drop(socket);
    completion.await.unwrap();
    let rows = captures(&h).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, capture::CaptureKind::WsConnection);
    assert_eq!(rows[0].response_status, Some(101));
    assert!(
        events(&h)
            .await
            .iter()
            .any(|e| e.kind == event::CaptureEventKind::WsText && e.payload == b"hello")
    );
    assert_eq!(usages(&h).await.len(), 1);
}

/// A channel that consumes two physical responses and returns a buffered local
/// representation, as catalog/account adapters do.
struct TwoCalls(TestChannel);
impl gproxy_channel::BaseChannel for TwoCalls {
    fn id(&self) -> &'static str {
        "test"
    }
    fn native_dialects(
        &self,
        provider: gproxy_channel::channel::ProviderView<'_>,
        operation: gproxy_protocol::Operation,
    ) -> Vec<gproxy_protocol::Dialect> {
        self.0.native_dialects(provider, operation)
    }
    fn usage_extractor(&self) -> Option<&dyn gproxy_channel::channel::UsageExtractor> {
        Some(&self.0)
    }
    fn list_models<'a>(
        &'a self,
        ctx: gproxy_channel::channel::OperationContext<'a>,
    ) -> gproxy_channel::channel::OperationFuture<'a, WireResponse> {
        Box::pin(async move {
            for path in ["/first", "/second"] {
                let request = http::Request::builder()
                    .uri(format!("https://up.example{path}"))
                    .header("authorization", "Bearer ka")
                    .body(HttpBody::Bytes(Bytes::new()))
                    .unwrap();
                let response = ctx.client.send(request).await?;
                read(response.body).await;
            }
            Ok(WireResponse {
                status: StatusCode::OK,
                headers: http::HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::from_static(b"{\"data\":[]}")),
            })
        })
    }
}
#[tokio::test]
async fn channel_internal_calls_get_distinct_logs_and_buffered_return_settles() {
    let mut h = persistent().await;
    h.core = Core::builder(h.core.store().clone())
        .cache(h.core.cache().clone())
        .secret_codec(Arc::new(PlaintextCodec))
        .channel(Arc::new(TwoCalls(TestChannel::default())))
        .unwrap()
        .build()
        .unwrap();
    h.core.reload_data().await.unwrap();
    h.script(vec![
        json_reply(StatusCode::OK, json!({"usage": {"input_tokens": 1}})),
        json_reply(StatusCode::OK, json!({"usage": {"input_tokens": 2}})),
    ]);
    let key = gproxy_protocol::OperationKey {
        operation: gproxy_protocol::Operation::ListModels,
        dialect: gproxy_protocol::Dialect::OpenAi,
    };
    let execution = tokio::time::timeout(
        Duration::from_secs(3),
        h.core
            .list_models(h.context_for("p", key, "internal", 1, None), request("{}")),
    )
    .await
    .unwrap()
    .unwrap();
    let (response, completion) = execution.into_parts();
    assert_eq!(read(response.body).await, "{\"data\":[]}");
    let report = completion.await.unwrap();
    assert!(report.exchanges.is_empty());
    assert_eq!(report.state, UsageState::Skipped);
    let rows = captures(&h).await;
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|r| r.attempt_id.as_deref() == Some("internal-1")
                && r.state == capture::CaptureState::Completed)
    );
    assert_ne!(rows[0].id, rows[1].id);
    assert_ne!(rows[0].request_url, rows[1].request_url);
    assert!(
        usages(&h).await.is_empty(),
        "model catalog calls only produce logs, not usage rows"
    );
}

#[tokio::test]
async fn ready_stream_chunks_keep_order_and_boundaries_without_queue_loss() {
    let h = persistent().await;
    let chunks: Vec<_> = (0..600)
        .map(|i| Bytes::from(format!("chunk-{i}\n")))
        .collect();
    h.script(vec![(StatusCode::OK, vec![], chunks.clone())]);
    run(&h, "many-chunks", 1).await;
    let mut logged: Vec<_> = events(&h)
        .await
        .into_iter()
        .filter(|e| e.direction == event::CaptureDirection::Response)
        .collect();
    logged.sort_by_key(|e| e.sequence);
    assert_eq!(logged.len(), chunks.len());
    for (row, original) in logged.iter().zip(chunks) {
        assert_eq!(row.payload, original);
    }
    assert_eq!(
        captures(&h).await[0].response_body_state,
        capture::CaptureBodyState::Complete
    );
}

struct FailOnce {
    failed: std::sync::atomic::AtomicBool,
    inner: Arc<ScriptClient>,
}
impl OutboundClient for FailOnce {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse, CapabilityError>> {
        Box::pin(async move {
            if !self.failed.swap(true, std::sync::atomic::Ordering::SeqCst) {
                Err(CapabilityError::new(
                    gproxy_protocol::capability::CapabilityErrorKind::Transport,
                    gproxy_protocol::capability::CapabilityErrorStage::Start,
                    "connection reset",
                ))
            } else {
                self.inner.send(request).await
            }
        })
    }
    fn connect<'a>(
        &'a self,
        request: http::Request<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        self.inner.connect(request)
    }
}
#[tokio::test]
async fn transport_failure_is_retained_when_retry_succeeds() {
    let h = persistent().await;
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"usage": {"input_tokens": 7}}),
    )]);
    let mut ctx = h.context("transport-retry", 2, None);
    replace_client(
        &mut ctx,
        Arc::new(FailOnce {
            failed: false.into(),
            inner: h.client.clone(),
        }),
    );
    let execution = h
        .core
        .stream_generate_content(ctx, request("{}"))
        .await
        .unwrap();
    let (response, completion) = execution.into_parts();
    read(response.body).await;
    assert_eq!(completion.await.unwrap().state, UsageState::Completed);
    let mut rows = captures(&h).await;
    rows.sort_by_key(|r| r.attempt_ordinal);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].state, capture::CaptureState::Failed);
    assert_eq!(rows[0].response_status, None);
    assert_eq!(rows[0].metrics, None);
    assert_eq!(rows[1].state, capture::CaptureState::Completed);
    assert_eq!(usages(&h).await[0].metrics["tokens"]["input_tokens"], 7);
}

struct InterruptedBody {
    error: bool,
}
impl OutboundClient for InterruptedBody {
    fn send<'a>(
        &'a self,
        _: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse, CapabilityError>> {
        Box::pin(async move {
            let first = futures_util::stream::iter([Ok(Bytes::from_static(
                br#"{"usage":{"input_tokens":5,"output_tokens":2}}"#,
            ))]);
            let body: gproxy_protocol::connection::ByteStream = if self.error {
                let error: gproxy_protocol::connection::TransportError =
                    Box::new(std::io::Error::other("connection reset"));
                Box::pin(first.chain(futures_util::stream::iter([Err(error)])))
            } else {
                Box::pin(first.chain(futures_util::stream::pending()))
            };
            Ok(WireResponse {
                status: StatusCode::OK,
                headers: http::HeaderMap::new(),
                body: HttpBody::Stream(body),
            })
        })
    }
    fn connect<'a>(
        &'a self,
        _: http::Request<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(std::future::pending())
    }
}
#[tokio::test]
async fn body_transport_error_and_explicit_cancellation_keep_partial_usage() {
    for error in [true, false] {
        let h = persistent().await;
        let mut ctx = h.context("interrupted", 1, None);
        let cancellation = ctx.cancellation.clone();
        replace_client(&mut ctx, Arc::new(InterruptedBody { error }));
        let execution = h
            .core
            .stream_generate_content(ctx, request("{}"))
            .await
            .unwrap();
        let (response, completion) = execution.into_parts();
        let HttpBody::Stream(mut body) = response.body else {
            panic!("stream")
        };
        body.next().await.unwrap().unwrap();
        if !error {
            cancellation.cancel();
        }
        assert!(body.next().await.unwrap().is_err());
        let report = completion.await.unwrap();
        assert_eq!(
            report.state,
            if error {
                UsageState::Failed
            } else {
                UsageState::Cancelled
            }
        );
        assert_eq!(report.exchanges[0].usage.tokens.input_tokens, Some(5));
        assert_eq!(
            report.exchanges[0].usage.completeness,
            gproxy_channel::channel::UsageCompleteness::Partial
        );
        let rows = captures(&h).await;
        assert_eq!(
            rows[0].state,
            if error {
                capture::CaptureState::Failed
            } else {
                capture::CaptureState::Cancelled
            }
        );
        assert_eq!(
            rows[0].response_body_state,
            capture::CaptureBodyState::Partial
        );
        assert!(rows[0].error.is_some());
        assert_eq!(usages(&h).await.len(), 1);
    }
}

#[tokio::test]
async fn preparation_failure_is_failed_not_cancelled_and_creates_no_fake_exchange() {
    use gproxy_store::entity::upstream::{provider_rewrite_rule_set, rewrite_rule};
    let h = persistent().await;
    h.core
        .store()
        .provider_rewrite_rule_sets()
        .create_many(vec![provider_rewrite_rule_set::ActiveModel {
            id: Set("bind".into()),
            provider_id: Set("p".into()),
            rule_set_id: Set("set".into()),
            created_at_ms: Set(0),
            updated_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    h.core
        .store()
        .rewrite_rules()
        .update_many(vec![rewrite_rule::ActiveModel {
            id: Set("hdr".into()),
            replacement: Set("invalid\nheader".into()),
            ..Default::default()
        }])
        .await
        .unwrap();
    settings(&h, true, true, true, true).await;
    let error = h
        .core
        .stream_generate_content(h.context("prepare-failed", 1, None), request("{}"))
        .await
        .err()
        .unwrap();
    assert!(matches!(error, gproxy_core::CoreError::Rewrite(_)));
    assert!(captures(&h).await.is_empty());
    let rows = usages(&h).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].metrics["state"], "failed");
    assert_eq!(rows[0].metrics["completeness"], "unknown");
}

#[tokio::test]
async fn caller_usage_reads_persisted_user_attribution_instead_of_opaque_scope() {
    use gproxy_core::{CallerRole, ServiceRequest, ServiceView};
    let h = persistent().await;
    h.channel
        .expose_services
        .store(true, std::sync::atomic::Ordering::Relaxed);
    // The real user differs from the isolation scope. Include a decoy user
    // named exactly like that scope and an unattributed record to catch leaks.
    for (id, user_id, tokens) in [
        ("actual-user", Some("user-a"), 3),
        ("scope-decoy", Some("tenant"), 99),
        ("unattributed", None, 50),
    ] {
        h.script(vec![json_reply(
            StatusCode::OK,
            json!({"usage": {"input_tokens": tokens}}),
        )]);
        let mut context = (*h.context(id, 1, None)).clone();
        assert_eq!(context.scope, "tenant");
        context.attribution.user_id = user_id.map(str::to_owned);
        let execution = h
            .core
            .stream_generate_content(Arc::new(context), request("{}"))
            .await
            .unwrap();
        let (response, completion) = execution.into_parts();
        read(response.body).await;
        completion.await.unwrap();
    }
    for (user_id, expected) in [(Some("user-a"), 3), (None, 0)] {
        let response = h
            .core
            .call_service(ServiceRequest {
                scope: "tenant".into(),
                user_id: user_id.map(str::to_owned),
                caller: CallerRole::Member,
                view: ServiceView::Caller,
                target: h.target("p"),
                budgets: Vec::new(),
                request: gproxy_protocol::WireRequest {
                    method: http::Method::GET,
                    path: "/usage".into(),
                    query: None,
                    headers: http::HeaderMap::new(),
                    body: HttpBody::Bytes(Bytes::new()),
                },
            })
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_str(&read(response.body).await).unwrap();
        assert_eq!(body["input_tokens"], expected);
    }
}
