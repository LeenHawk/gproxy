#![cfg(not(target_arch = "wasm32"))]

use futures_util::StreamExt;
use gproxy_channel::{
    BaseChannel, ChannelError, OutboundClient,
    channel::{NormalizedUsage, PrepareContext, UsageContext, UsageExtractor},
};
use gproxy_core::{
    BlockSource, CaptureEnd, CaptureEvent, CapturePolicy, CaptureSink, Core, CoreError,
    ExchangeContext, ExecutionTarget, ObservationPolicy, Observer, PlaintextCodec, RequestContext,
    SecretCodec, SessionIdentity, SessionSource, TraceEvent, UsageReport, UsageState, keys,
};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    capability::{CapabilityError, CapabilityFuture},
    connection::Bytes,
};
use gproxy_store::{
    Store,
    entity::{
        config::setting,
        upstream::{
            credential, operation_endpoint, provider, provider_rewrite_rule_set, rewrite_rule,
            rewrite_rule_set,
        },
    },
};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbBackend, Set};
use serde_json::json;
use std::{
    collections::VecDeque,
    num::NonZeroU32,
    sync::{Arc, Mutex},
};
use tokio_util::sync::CancellationToken;

const KEY: OperationKey = OperationKey {
    operation: Operation::StreamGenerateContent,
    dialect: Dialect::OpenAi,
};

#[derive(Default)]
struct Log(Mutex<Vec<String>>);
impl Log {
    fn push(&self, line: impl Into<String>) {
        self.0.lock().unwrap().push(line.into());
    }
    fn lines(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

struct Recorder {
    policy: Mutex<ObservationPolicy>,
    log: Arc<Log>,
    reports: Mutex<Vec<UsageReport>>,
}
impl Recorder {
    fn new(policy: ObservationPolicy) -> Arc<Self> {
        Arc::new(Self {
            policy: Mutex::new(policy),
            log: Arc::default(),
            reports: Mutex::new(Vec::new()),
        })
    }
}
struct Sink(Arc<Log>, String);
impl CaptureSink for Sink {
    fn record(&mut self, sequence: u64, event: CaptureEvent<'_>) {
        let line = match event {
            CaptureEvent::RequestHead {
                method,
                uri,
                headers,
            } => format!(
                "{sequence} req {method} {uri} tag={}",
                headers
                    .get("x-client-tag")
                    .map(|v| v.to_str().unwrap().to_owned())
                    .unwrap_or_default()
            ),
            CaptureEvent::RequestChunk(bytes) => {
                format!("{sequence} req-chunk {}", String::from_utf8_lossy(bytes))
            }
            CaptureEvent::ResponseHead { status, .. } => format!("{sequence} resp {status}"),
            CaptureEvent::ResponseChunk(bytes) => {
                format!("{sequence} resp-chunk {}", String::from_utf8_lossy(bytes))
            }
            CaptureEvent::Frame { .. } => format!("{sequence} frame"),
        };
        self.0.push(format!("{} {line}", self.1));
    }
    fn finish(self: Box<Self>, end: CaptureEnd) -> CapabilityFuture<'static, ()> {
        self.0.push(format!("{} finish {end:?}", self.1));
        Box::pin(async {})
    }
}
impl Observer for Recorder {
    fn policy(&self, _: &RequestContext) -> ObservationPolicy {
        *self.policy.lock().unwrap()
    }
    fn capture(&self, exchange: &ExchangeContext, _: CapturePolicy) -> Box<dyn CaptureSink> {
        Box::new(Sink(self.log.clone(), exchange.attempt.attempt_id.clone()))
    }
    fn usage<'a>(&'a self, report: &'a UsageReport) -> CapabilityFuture<'a, ()> {
        self.reports.lock().unwrap().push(report.clone());
        Box::pin(async {})
    }
    fn trace(&self, event: TraceEvent<'_>) {
        if let TraceEvent::AttemptFinished {
            attempt, outcome, ..
        } = event
        {
            self.log.push(format!(
                "trace {} {} {outcome:?}",
                attempt.attempt_id, attempt.credential.id
            ));
        }
    }
}

struct TestChannel;
impl BaseChannel for TestChannel {
    fn id(&self) -> &'static str {
        "test"
    }
    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let url = match ctx.endpoint_override {
            Some(url) => url.to_owned(),
            None => format!("{}{}", ctx.provider.base_url.unwrap(), ctx.request.path),
        };
        let mut builder = http::Request::builder()
            .method(ctx.request.method)
            .uri(url)
            .header(
                "authorization",
                format!(
                    "Bearer {}",
                    ctx.credential.secret["api_key"].as_str().unwrap()
                ),
            );
        for (name, value) in &ctx.request.headers {
            builder = builder.header(name, value);
        }
        builder
            .body(ctx.request.body)
            .map_err(|_| ChannelError::InvalidCredential)
    }
    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        Some(self)
    }
}
impl UsageExtractor for TestChannel {
    fn extract(&self, ctx: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        let value: serde_json::Value = match serde_json::from_slice(ctx.response.body) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        if value.get("usage").is_none() {
            return Ok(None);
        }
        let mut usage = NormalizedUsage::default();
        usage.tokens.input_tokens = value["usage"]["input_tokens"].as_u64();
        usage.tokens.output_tokens = value["usage"]["output_tokens"].as_u64();
        Ok(Some(usage))
    }
}

type Reply = (StatusCode, Vec<(&'static str, &'static str)>, Vec<Bytes>);
#[derive(Default)]
struct ScriptClient {
    replies: Mutex<VecDeque<Reply>>,
    seen: Arc<Log>,
}
impl OutboundClient for ScriptClient {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse, CapabilityError>> {
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let body = match body {
                HttpBody::Bytes(b) => b,
                HttpBody::Stream(mut s) => {
                    let mut out = Vec::new();
                    while let Some(chunk) = s.next().await {
                        out.extend_from_slice(&chunk.unwrap());
                    }
                    Bytes::from(out)
                }
            };
            self.seen.push(format!(
                "{} {} auth={} tag={} body={}",
                parts.method,
                parts.uri,
                parts.headers["authorization"].to_str().unwrap(),
                parts
                    .headers
                    .get("x-client-tag")
                    .map(|v| v.to_str().unwrap().to_owned())
                    .unwrap_or_default(),
                String::from_utf8_lossy(&body)
            ));
            let (status, headers, chunks) = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("scripted reply");
            let mut map = HeaderMap::new();
            for (name, value) in headers {
                map.insert(name, HeaderValue::from_static(value));
            }
            Ok(WireResponse {
                status,
                headers: map,
                body: HttpBody::Stream(Box::pin(futures_util::stream::iter(
                    chunks.into_iter().map(Ok),
                ))),
            })
        })
    }
}

struct Harness {
    core: Core<DatabaseConnection>,
    observer: Arc<Recorder>,
    client: Arc<ScriptClient>,
}

async fn harness(policy: ObservationPolicy, strategy: &str) -> Harness {
    let mut options = ConnectOptions::new("sqlite::memory:");
    options.max_connections(1).sqlx_logging(false);
    let db = Database::connect(options).await.unwrap();
    gproxy_store::schema(DbBackend::Sqlite)
        .apply(&db)
        .await
        .unwrap();
    let store = Store::new(db);
    store
        .settings()
        .update(setting::ActiveModel {
            config_revision: Set(1),
            ..Default::default()
        })
        .await
        .unwrap();
    store
        .users()
        .create_many(vec![gproxy_store::entity::identity::user::ActiveModel {
            id: Set("u".into()),
            name: Set("u".into()),
            role: Set("user".into()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .providers()
        .create_many(vec![provider::ActiveModel {
            id: Set("p".into()),
            name: Set("p".into()),
            channel: Set("test".into()),
            base_url: Set(Some("https://up.example".into())),
            config: Set(json!({"credential_strategy": strategy})),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    let cred = |id: &str, key: &str| credential::ActiveModel {
        id: Set(id.into()),
        provider_id: Set("p".into()),
        user_id: Set(Some("u".into())),
        auth_kind: Set("api_key".into()),
        secret: Set(PlaintextCodec.seal(id, &json!({"api_key": key})).unwrap()),
        metadata: Set(json!({})),
        ..Default::default()
    };
    store
        .credentials()
        .create_many(vec![cred("a", "ka"), cred("b", "kb")])
        .await
        .unwrap();
    store
        .operation_endpoints()
        .create_many(vec![operation_endpoint::ActiveModel {
            id: Set("e".into()),
            provider_id: Set("p".into()),
            operation: Set("stream_generate_content".into()),
            dialect: Set("openai".into()),
            url: Set("https://alt.example/v1/responses".into()),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .rewrite_rule_sets()
        .create_many(vec![rewrite_rule_set::ActiveModel {
            id: Set("set".into()),
            name: Set("set".into()),
            created_at_ms: Set(0),
            updated_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    store
        .rewrite_rules()
        .create_many(vec![
            rewrite_rule::ActiveModel {
                id: Set("hdr".into()),
                rule_set_id: Set("set".into()),
                phase: Set("request".into()),
                target: Set(rewrite_rule::RewriteTarget::Header),
                target_name: Set(Some("x-client-tag".into())),
                pattern: Set("^old$".into()),
                replacement: Set("new".into()),
                created_at_ms: Set(0),
                updated_at_ms: Set(0),
                ..Default::default()
            },
            rewrite_rule::ActiveModel {
                id: Set("sse".into()),
                rule_set_id: Set("set".into()),
                phase: Set("response".into()),
                paths: Set(Some(json!(["delta"]))),
                pattern: Set("secret".into()),
                replacement: Set("***".into()),
                created_at_ms: Set(0),
                updated_at_ms: Set(0),
                ..Default::default()
            },
        ])
        .await
        .unwrap();
    store
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
    let observer = Recorder::new(policy);
    let core = Core::builder(Arc::new(store))
        .cache(Arc::new(gproxy_cache::MemoryCache::default()))
        .observer(observer.clone())
        .secret_codec(Arc::new(PlaintextCodec))
        .channel(Arc::new(TestChannel))
        .unwrap()
        .build()
        .unwrap();
    core.reload_data().await.unwrap();
    Harness {
        core,
        observer,
        client: Arc::new(ScriptClient::default()),
    }
}

impl Harness {
    fn script(&self, replies: Vec<Reply>) {
        *self.client.replies.lock().unwrap() = replies.into();
    }
    /// Swap the pooled clients for the scripted one, keeping everything else.
    fn context(&self, id: &str, attempts: u32, session: Option<&str>) -> Arc<RequestContext> {
        let snapshot = self.core.snapshot();
        let provider = snapshot.providers["p"].clone();
        let credentials = provider
            .credential_ids
            .iter()
            .map(|id| {
                let c = &snapshot.credentials[id];
                Arc::new(gproxy_core::CredentialData {
                    id: c.id.clone(),
                    provider_id: c.provider_id.clone(),
                    label: None,
                    auth_kind: c.auth_kind.clone(),
                    enabled: c.enabled,
                    metadata: c.metadata.clone(),
                    client: self.client.clone(),
                    websocket_client: self.client.clone(),
                    state: c.state.clone(),
                    quota: Vec::new(),
                })
            })
            .collect();
        Arc::new(RequestContext {
            request_id: id.into(),
            snapshot: snapshot.clone(),
            scope: "tenant".into(),
            session: session.map(|s| SessionIdentity {
                id: s.into(),
                source: SessionSource::Gateway,
                field: None,
            }),
            operation: KEY,
            target: ExecutionTarget {
                provider,
                upstream_model: Some("gpt-x".into()),
                credentials,
            },
            max_attempts: NonZeroU32::new(attempts).unwrap(),
            started_at_ms: 0,
            deadline: None,
            cancellation: CancellationToken::new(),
        })
    }
}

fn request(body: &str) -> WireRequest<HttpBody> {
    let mut headers = HeaderMap::new();
    headers.insert("x-client-tag", HeaderValue::from_static("old"));
    WireRequest {
        method: Method::POST,
        path: "/v1/responses".into(),
        query: None,
        headers,
        body: HttpBody::Bytes(Bytes::from(body.to_owned())),
    }
}

fn json_reply(status: StatusCode, body: serde_json::Value) -> Reply {
    (
        status,
        vec![("content-type", "application/json")],
        vec![Bytes::from(serde_json::to_vec(&body).unwrap())],
    )
}

async fn read(body: HttpBody) -> String {
    match body {
        HttpBody::Bytes(b) => String::from_utf8(b.to_vec()).unwrap(),
        HttpBody::Stream(mut s) => {
            let mut out = Vec::new();
            while let Some(chunk) = s.next().await {
                out.extend_from_slice(&chunk.unwrap());
            }
            String::from_utf8(out).unwrap()
        }
    }
}

fn full() -> ObservationPolicy {
    ObservationPolicy {
        usage: true,
        capture: CapturePolicy::Full,
        trace: true,
    }
}

#[tokio::test]
async fn rotates_credentials_rewrites_request_uses_endpoint_and_settles_usage() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![
        json_reply(
            StatusCode::OK,
            json!({"ok": 1, "usage": {"input_tokens": 3, "output_tokens": 5}}),
        ),
        json_reply(StatusCode::OK, json!({"ok": 2})),
    ]);
    let execution = h
        .core
        .stream_generate_content(h.context("r1", 3, None), request("{\"q\":1}"))
        .await
        .unwrap();
    assert_eq!(execution.response().status, StatusCode::OK);
    let (response, completion) = execution.into_parts();
    assert_eq!(
        read(response.body).await,
        "{\"ok\":1,\"usage\":{\"input_tokens\":3,\"output_tokens\":5}}"
    );
    let report = completion.await.unwrap();
    assert_eq!(report.state, UsageState::Completed);
    assert_eq!(report.exchanges.len(), 1);
    assert_eq!(report.exchanges[0].credential_id, "a");
    assert_eq!(report.exchanges[0].usage.tokens.input_tokens, Some(3));
    assert_eq!(
        h.observer.reports.lock().unwrap().len(),
        1,
        "funnel called usage once"
    );

    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].starts_with(
            "POST https://alt.example/v1/responses auth=Bearer ka tag=new body={\"q\":1}"
        ),
        "{}",
        seen[0]
    );
    let log = h.observer.log.lines();
    assert!(
        log.iter()
            .any(|l| l.starts_with("r1-1 0 req POST https://alt.example/v1/responses tag=new")),
        "{log:?}"
    );
    assert!(log.iter().any(|l| l.contains("req-chunk {\"q\":1}")));
    assert!(log.iter().any(|l| l.contains("resp 200 OK")));
    assert!(log.iter().any(|l| l.contains("resp-chunk {\"ok\":1")));
    assert!(log.iter().any(|l| l == "r1-1 finish Complete"), "{log:?}");
    assert!(log.iter().any(|l| l.starts_with("trace r1-1 a Succeeded")));

    let second = h
        .core
        .stream_generate_content(h.context("r2", 3, None), request("{}"))
        .await
        .unwrap();
    let (response, completion) = second.into_parts();
    read(response.body).await;
    assert_eq!(completion.await.unwrap().state, UsageState::Completed);
    assert!(
        h.client.seen.lines()[1].contains("auth=Bearer kb"),
        "round robin advances"
    );
}

#[tokio::test]
async fn rate_limit_blocks_the_credential_persistently_and_fails_over() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![
        (
            StatusCode::TOO_MANY_REQUESTS,
            vec![("retry-after", "120")],
            vec![Bytes::from_static(b"slow")],
        ),
        json_reply(StatusCode::OK, json!({"ok": true})),
        json_reply(StatusCode::OK, json!({"ok": true})),
    ]);
    let execution = h
        .core
        .stream_generate_content(h.context("r1", 3, None), request("{}"))
        .await
        .unwrap();
    let (response, completion) = execution.into_parts();
    assert_eq!(response.status, StatusCode::OK);
    read(response.body).await;
    let report = completion.await.unwrap();
    assert_eq!(
        report.exchanges.len(),
        0,
        "a JSON body without usage reports nothing"
    );
    let seen = h.client.seen.lines();
    assert!(seen[0].contains("auth=Bearer ka"));
    assert!(seen[1].contains("auth=Bearer kb"));

    let rows = h
        .core
        .store()
        .load_control_data()
        .await
        .unwrap()
        .credential_blocks;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].credential_id, "a");
    assert_eq!(rows[0].source["kind"], "rate_limited");
    let cached: gproxy_core::CredentialBlocks = serde_json::from_slice(
        &h.core
            .cache()
            .get(&keys::credential_blocks("p", "a"))
            .await
            .unwrap()
            .unwrap()
            .value,
    )
    .unwrap();
    assert!(matches!(cached.blocks[0].source, BlockSource::RateLimited));
    assert!(cached.blocks[0].until_ms - cached.blocks[0].observed_at_ms == 120_000);
    let log = h.observer.log.lines();
    assert!(
        log.iter().any(|l| l == "r1-1 finish Interrupted"),
        "dropped 429 body still closes capture: {log:?}"
    );

    let third = h
        .core
        .stream_generate_content(h.context("r2", 3, None), request("{}"))
        .await
        .unwrap();
    read(third.into_parts().0.body).await;
    assert!(
        h.client.seen.lines()[2].contains("auth=Bearer kb"),
        "blocked credential is skipped"
    );
}

#[tokio::test]
async fn repeated_server_errors_trip_a_failure_block_and_the_last_answer_is_returned() {
    let h = harness(full(), "sticky").await;
    h.script(vec![
        json_reply(StatusCode::BAD_GATEWAY, json!({"e": 1})),
        json_reply(StatusCode::BAD_GATEWAY, json!({"e": 2})),
        json_reply(StatusCode::BAD_GATEWAY, json!({"e": 3})),
        json_reply(StatusCode::BAD_GATEWAY, json!({"e": 4})),
    ]);
    let ctx = h.context("r1", 4, Some("s1"));
    let one_credential = Arc::new(RequestContext {
        target: ExecutionTarget {
            provider: ctx.target.provider.clone(),
            upstream_model: ctx.target.upstream_model.clone(),
            credentials: vec![ctx.target.credentials[0].clone()],
        },
        request_id: ctx.request_id.clone(),
        snapshot: ctx.snapshot.clone(),
        scope: ctx.scope.clone(),
        session: ctx.session.clone(),
        operation: ctx.operation,
        max_attempts: ctx.max_attempts,
        started_at_ms: 0,
        deadline: None,
        cancellation: CancellationToken::new(),
    });
    let execution = h
        .core
        .stream_generate_content(one_credential.clone(), request("{}"))
        .await
        .unwrap();
    let (response, completion) = execution.into_parts();
    assert_eq!(response.status, StatusCode::BAD_GATEWAY);
    assert_eq!(
        read(response.body).await,
        "{\"e\":3}",
        "three attempts, then the third failure is credential-blocked"
    );
    assert_eq!(completion.await.unwrap().state, UsageState::Completed);
    let cached: gproxy_core::CredentialBlocks = serde_json::from_slice(
        &h.core
            .cache()
            .get(&keys::credential_blocks("p", "a"))
            .await
            .unwrap()
            .unwrap()
            .value,
    )
    .unwrap();
    assert!(
        matches!(
            cached.blocks[0].source,
            BlockSource::Failures { consecutive: 3 }
        ),
        "{:?}",
        cached.blocks
    );
    assert_eq!(
        cached.blocks[0].scope,
        gproxy_channel::channel::QuotaScope::Models(vec!["gpt-x".into()])
    );

    let error = h
        .core
        .stream_generate_content(one_credential, request("{}"))
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::NoUsableCredential), "{error}");
    let report = h.observer.reports.lock().unwrap().pop().unwrap();
    assert_eq!(
        report.state,
        UsageState::Failed,
        "a failed request still reaches the funnel"
    );
}

#[tokio::test]
async fn policy_off_skips_capture_and_usage_and_dead_credentials_are_never_picked() {
    let h = harness(
        ObservationPolicy {
            usage: false,
            capture: CapturePolicy::Off,
            trace: false,
        },
        "round_robin",
    )
    .await;
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"usage": {"input_tokens": 1}}),
    )]);
    let execution = h
        .core
        .stream_generate_content(h.context("r1", 1, None), request("{}"))
        .await
        .unwrap();
    let (response, completion) = execution.into_parts();
    read(response.body).await;
    let report = completion.await.unwrap();
    assert_eq!(report.state, UsageState::Skipped);
    assert!(report.exchanges.is_empty());
    assert!(
        h.observer.reports.lock().unwrap().is_empty(),
        "usage funnel not called when disabled"
    );
    assert!(h.observer.log.lines().is_empty(), "no capture, no trace");

    h.core
        .store()
        .credentials()
        .set_status_many(vec![
            gproxy_store::operations::credentials::CredentialStatusUpdate {
                id: "a".into(),
                expected_version: 0,
                status: gproxy_core::CredentialStatus::Dead,
                reason: Some("invalid_grant".into()),
            },
            gproxy_store::operations::credentials::CredentialStatusUpdate {
                id: "b".into(),
                expected_version: 0,
                status: gproxy_core::CredentialStatus::Dead,
                reason: Some("revoked".into()),
            },
        ])
        .await
        .unwrap();
    h.core
        .reload_credentials(&["a".into(), "b".into()])
        .await
        .unwrap();
    let error = h
        .core
        .stream_generate_content(h.context("r2", 2, None), request("{}"))
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::CredentialDead { .. }), "{error}");
}

#[tokio::test]
async fn streaming_request_bodies_are_buffered_for_replay_and_sse_responses_are_rewritten_per_event()
 {
    let h = harness(full(), "round_robin").await;
    h.script(vec![
        json_reply(StatusCode::INTERNAL_SERVER_ERROR, json!({})),
        (
            StatusCode::OK,
            vec![("content-type", "text/event-stream")],
            vec![
                Bytes::from_static(b"event: message\ndata: {\"delta\":\"my sec"),
                Bytes::from_static(b"ret\"}\n\ndata: [DONE]\n\n"),
            ],
        ),
    ]);
    let body: Vec<Result<Bytes, gproxy_protocol::connection::TransportError>> = vec![
        Ok(Bytes::from_static(b"{\"pa")),
        Ok(Bytes::from_static(b"rt\":2}")),
    ];
    let wire = WireRequest {
        method: Method::POST,
        path: "/v1/responses".into(),
        query: None,
        headers: HeaderMap::new(),
        body: HttpBody::Stream(Box::pin(futures_util::stream::iter(body))),
    };
    let execution = h
        .core
        .stream_generate_content(h.context("r1", 2, None), wire)
        .await
        .unwrap();
    let (response, completion) = execution.into_parts();
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        read(response.body).await,
        "event: message\ndata: {\"delta\":\"my ***\"}\n\ndata: [DONE]\n\n"
    );
    assert_eq!(completion.await.unwrap().state, UsageState::Completed);
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 2);
    assert!(seen[0].ends_with("body={\"part\":2}"), "{}", seen[0]);
    assert!(
        seen[1].ends_with("body={\"part\":2}"),
        "buffered body replayed: {}",
        seen[1]
    );
    assert!(seen[0].contains("auth=Bearer ka") && seen[1].contains("auth=Bearer kb"));
}

#[tokio::test]
async fn cancellation_before_dispatch_settles_as_cancelled() {
    let h = harness(full(), "round_robin").await;
    let ctx = h.context("r1", 2, None);
    ctx.cancellation.cancel();
    let error = h
        .core
        .stream_generate_content(ctx, request("{}"))
        .await
        .unwrap_err();
    assert!(matches!(error, CoreError::Cancelled));
    let report = h.observer.reports.lock().unwrap().pop().unwrap();
    assert_eq!(report.state, UsageState::Cancelled);
    assert!(h.client.seen.lines().is_empty());
}
