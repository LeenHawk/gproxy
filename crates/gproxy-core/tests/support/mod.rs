//! Scripted channel, client and observer shared by the execution test files.
#![allow(dead_code)]

use futures_util::StreamExt;
use gproxy_channel::{
    BaseChannel, ChannelError, OutboundClient,
    channel::{
        ChannelServices, CredentialContext, CredentialRefresh, CredentialUpdate, CredentialView,
        NormalizedUsage, OperationContext, OperationFuture, PrepareContext, ProviderView,
        QuotaAllowance, QuotaDimension, QuotaEntry, QuotaHeaderContext, QuotaHeaders, QuotaMetric,
        QuotaModel, QuotaQuery, QuotaScope, QuotaSnapshot, QuotaSubject, QuotaTracking, QuotaValue,
        QuotaWindow, RefreshContext, ServiceContext, UsageContext, UsageExtractor,
    },
};
use gproxy_core::{
    CaptureEnd, CaptureEvent, CapturePolicy, CaptureSink, Core, ExchangeContext, ExecutionTarget,
    ObservationPolicy, Observer, PlaintextCodec, RequestContext, SecretCodec, SessionIdentity,
    SessionSource, TraceEvent, UsageReport,
};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    capability::{CapabilityError, CapabilityFuture, UpstreamConnection},
    connection::{Bytes, TransportError, WebSocket, WsFrame},
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
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio_util::sync::CancellationToken;

pub const KEY: OperationKey = OperationKey {
    operation: Operation::StreamGenerateContent,
    dialect: Dialect::OpenAi,
};

#[derive(Default)]
pub struct Log(pub Mutex<Vec<String>>);
impl Log {
    pub fn push(&self, line: impl Into<String>) {
        self.0.lock().unwrap().push(line.into());
    }
    pub fn lines(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

pub struct Recorder {
    pub policy: Mutex<ObservationPolicy>,
    pub log: Arc<Log>,
    pub reports: Mutex<Vec<UsageReport>>,
}
impl Recorder {
    pub fn new(policy: ObservationPolicy) -> Arc<Self> {
        Arc::new(Self {
            policy: Mutex::new(policy),
            log: Arc::default(),
            reports: Mutex::new(Vec::new()),
        })
    }
}
pub struct Sink(pub Arc<Log>, pub String);
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

pub enum RefreshReply {
    Rotated {
        api_key: &'static str,
        expires_at_ms: Option<i64>,
    },
    Rejected(&'static str),
    Failed,
}

#[derive(Default)]
pub struct TestChannel {
    pub refreshes: Mutex<VecDeque<RefreshReply>>,
    pub refresh_calls: Mutex<Vec<(String, i64)>>,
    pub quota_snapshots: Mutex<VecDeque<QuotaSnapshot>>,
    /// When set, the channel exposes `ChannelServices`; off by default so the
    /// "channel without services" path is the one most tests see.
    pub expose_services: AtomicBool,
    /// Credential ids the service calls ran with, in order.
    pub service_calls: Mutex<Vec<String>>,
}
impl BaseChannel for TestChannel {
    fn id(&self) -> &'static str {
        "test"
    }
    fn services(&self) -> Option<&dyn ChannelServices> {
        self.expose_services
            .load(Ordering::Relaxed)
            .then_some(self as &dyn ChannelServices)
    }
    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
    }
    fn quota_model(&self) -> Option<&dyn QuotaModel> {
        Some(self)
    }
    fn quota_headers(&self) -> Option<&dyn QuotaHeaders> {
        Some(self)
    }
    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }
    fn native_dialects(&self, provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        if operation == Operation::StreamGenerateContent
            && provider.config.get("buffered_only") == Some(&json!(true))
        {
            return Vec::new();
        }
        let configured: Vec<Dialect> = provider
            .config
            .get("dialects")
            .cloned()
            .map(|v| serde_json::from_value(v).unwrap())
            .unwrap_or_default();
        if configured.is_empty() {
            vec![
                Dialect::OpenAi,
                Dialect::OpenAiChat,
                Dialect::Claude,
                Dialect::Gemini,
            ]
        } else {
            configured
        }
    }
    /// Compaction counts its turns in the channel's scoped state and tells the
    /// upstream which turn this is: the multi-request memory claudeweb-style
    /// channels rely on.
    fn compact_content<'a>(
        &'a self,
        mut context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async move {
            let entry = context.state.get("turns").await?;
            let (turn, expected) = match &entry {
                Some(entry) => (
                    String::from_utf8_lossy(&entry.payload)
                        .parse::<u32>()
                        .unwrap_or(0)
                        + 1,
                    Some(entry.version.clone()),
                ),
                None => (1, None),
            };
            let outcome = context
                .state
                .compare_exchange(
                    "turns",
                    expected,
                    Some(gproxy_protocol::capability::StateWrite {
                        payload: Bytes::from(turn.to_string()),
                        expires_at: Some(
                            std::time::SystemTime::UNIX_EPOCH
                                + std::time::Duration::from_secs(4_102_444_800),
                        ),
                    }),
                )
                .await?;
            assert!(matches!(
                outcome,
                gproxy_protocol::capability::CasResult::Applied(_)
            ));
            context
                .request
                .headers
                .insert("x-turn", HeaderValue::from_str(&turn.to_string()).unwrap());
            let request = self.prepare(PrepareContext {
                provider: context.provider,
                credential: context.credential,
                operation: OperationKey {
                    operation: Operation::CompactContent,
                    dialect: context.dialect,
                },
                request: context.request,
                endpoint_override: context.endpoint_override,
            })?;
            Ok(context.client.send(request).await?)
        })
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
    fn prepare_connect(
        &self,
        ctx: PrepareContext<'_, ()>,
    ) -> Result<http::Request<()>, ChannelError> {
        let mut builder = http::Request::builder()
            .method(ctx.request.method)
            .uri(format!(
                "{}{}",
                ctx.provider.base_url.unwrap(),
                ctx.request.path
            ))
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
            .body(())
            .map_err(|_| ChannelError::InvalidCredential)
    }
    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        Some(self)
    }
}
/// Dimensions come from credential metadata:
/// `{"quota":[{"id","metric":"requests"|"tokens","window_seconds":N,"limit":N,
/// "tracking":"counted"|"reported"}]}`. Scope is always `All`.
impl QuotaModel for TestChannel {
    fn dimensions(
        &self,
        _: ProviderView<'_>,
        credential: CredentialView<'_>,
    ) -> Vec<QuotaDimension> {
        credential
            .metadata
            .get("quota")
            .and_then(|q| q.as_array())
            .into_iter()
            .flatten()
            .map(|d| QuotaDimension {
                id: d["id"].as_str().unwrap().to_owned(),
                label: None,
                scope: QuotaScope::All,
                operations: None,
                metric: match d["metric"].as_str() {
                    Some("tokens") => QuotaMetric::Tokens,
                    _ => QuotaMetric::Requests,
                },
                window: QuotaWindow::Rolling {
                    seconds: d["window_seconds"].as_i64().unwrap_or(3600),
                },
                limit: d["limit"].as_u64().map(rust_decimal::Decimal::from),
                tracking: match d["tracking"].as_str() {
                    Some("reported") => QuotaTracking::Reported,
                    _ => QuotaTracking::Counted,
                },
            })
            .collect()
    }
}
/// `x-test-quota: <dimension>=<remaining>[;reset=<unix ms>]`, one entry.
impl QuotaHeaders for TestChannel {
    fn observe(&self, context: QuotaHeaderContext<'_>) -> Result<Vec<QuotaEntry>, ChannelError> {
        let Some(value) = context.headers.get("x-test-quota") else {
            return Ok(Vec::new());
        };
        let text = value.to_str().unwrap_or_default();
        let (dimension, rest) = text.split_once('=').unwrap_or((text, "0"));
        let (remaining, reset) = rest.split_once(";reset=").unwrap_or((rest, ""));
        Ok(vec![QuotaEntry {
            id: dimension.to_owned(),
            source_id: dimension.to_owned(),
            label: None,
            subject: QuotaSubject::Account,
            model_scope: QuotaScope::All,
            value: QuotaValue::Window(QuotaAllowance {
                remaining: remaining.parse().ok(),
                period_end_ms: reset.parse().ok(),
                ..Default::default()
            }),
        }])
    }
}
/// A scripted service surface: `/bindings/{kind}` lists the caller's
/// bindings, `POST /bind/{kind}/{id}` saves one for the selected credential,
/// anything else is forwarded to the provider base with the credential's
/// key. Every call records `credential:role:identity`.
impl ChannelServices for TestChannel {
    fn call<'a>(
        &'a self,
        context: ServiceContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async move {
            self.service_calls.lock().unwrap().push(format!(
                "{}:{:?}:{}",
                context.account.credential.id,
                context.caller.role(),
                context.caller.identity().id
            ));
            let path = context.request.path.clone();
            if let Some(kind) = path.strip_prefix("/bindings/") {
                let ids: Vec<String> = context
                    .caller
                    .list_bindings(kind)
                    .await?
                    .into_iter()
                    .map(|b| format!("{}@{}", b.upstream_id, b.credential_id))
                    .collect();
                let mut headers = HeaderMap::new();
                headers.insert("content-type", HeaderValue::from_static("application/json"));
                return Ok(WireResponse {
                    status: StatusCode::OK,
                    headers,
                    body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&ids).unwrap())),
                });
            }
            if let Some(rest) = path.strip_prefix("/bind/") {
                let (kind, id) = rest.split_once('/').unwrap();
                context
                    .caller
                    .save_binding(gproxy_channel::channel::ResourceBindingRecord {
                        kind: kind.to_owned(),
                        upstream_id: id.to_owned(),
                        credential_id: context.account.credential.id.to_owned(),
                        summary: json!({"id": id}),
                    })
                    .await?;
                return Ok(WireResponse {
                    status: StatusCode::CREATED,
                    headers: HeaderMap::new(),
                    body: HttpBody::Bytes(Bytes::new()),
                });
            }
            let request = self.prepare(PrepareContext {
                provider: context.account.provider,
                credential: context.account.credential,
                operation: KEY,
                request: context.request,
                endpoint_override: None,
            })?;
            Ok(context.account.client.send(request).await?)
        })
    }
}
impl QuotaQuery for TestChannel {
    fn query<'a>(&'a self, _: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        let reply = self
            .quota_snapshots
            .lock()
            .unwrap()
            .pop_front()
            .expect("scripted quota snapshot");
        Box::pin(async move { Ok(reply) })
    }
}
impl CredentialRefresh for TestChannel {
    fn refresh<'a>(
        &'a self,
        context: RefreshContext<'a>,
    ) -> CapabilityFuture<'a, Result<CredentialUpdate, ChannelError>> {
        self.refresh_calls
            .lock()
            .unwrap()
            .push((context.credential.id.to_owned(), context.credential.version));
        let reply = self
            .refreshes
            .lock()
            .unwrap()
            .pop_front()
            .expect("scripted refresh reply");
        Box::pin(async move {
            match reply {
                RefreshReply::Rotated {
                    api_key,
                    expires_at_ms,
                } => Ok(CredentialUpdate {
                    secret: json!({"api_key": api_key}),
                    expires_at_ms,
                }),
                RefreshReply::Rejected(reason) => Err(ChannelError::RefreshRejected(reason.into())),
                RefreshReply::Failed => Err(ChannelError::InvalidResponse("upstream down".into())),
            }
        })
    }
}
impl UsageExtractor for TestChannel {
    /// JSON bodies read `usage` directly; SSE bodies (Claude events) take
    /// input tokens from `message_start` and output tokens from the last
    /// `usage` seen.
    fn extract(&self, ctx: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        let values: Vec<serde_json::Value> = match serde_json::from_slice(ctx.response.body) {
            Ok(value) => vec![value],
            Err(_) => std::str::from_utf8(ctx.response.body)
                .unwrap_or_default()
                .lines()
                .filter_map(|line| line.strip_prefix("data: "))
                .filter_map(|data| serde_json::from_str(data).ok())
                .collect(),
        };
        let mut usage = NormalizedUsage::default();
        let mut seen = false;
        for value in &values {
            for candidate in [&value["usage"], &value["message"]["usage"]] {
                if candidate.is_object() {
                    seen = true;
                    if let Some(input) = candidate["input_tokens"].as_u64() {
                        usage.tokens.input_tokens = Some(input);
                    }
                    if let Some(output) = candidate["output_tokens"].as_u64() {
                        usage.tokens.output_tokens = Some(output);
                    }
                }
            }
        }
        Ok(seen.then_some(usage))
    }
}

pub type Reply = (StatusCode, Vec<(&'static str, &'static str)>, Vec<Bytes>);
pub enum WsReply {
    Rejected(StatusCode, &'static str),
    Connected(Vec<WsFrame>),
}
#[derive(Default)]
pub struct ScriptClient {
    pub replies: Mutex<VecDeque<Reply>>,
    pub ws_replies: Mutex<VecDeque<WsReply>>,
    pub sent_frames: Arc<Mutex<Vec<WsFrame>>>,
    pub seen: Arc<Log>,
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

    fn connect<'a>(
        &'a self,
        request: http::Request<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(async move {
            self.seen.push(format!(
                "WS {} auth={} tag={}",
                request.uri(),
                request.headers()["authorization"].to_str().unwrap(),
                request
                    .headers()
                    .get("x-client-tag")
                    .map(|v| v.to_str().unwrap().to_owned())
                    .unwrap_or_default(),
            ));
            let reply = self
                .ws_replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("scripted ws reply");
            Ok(match reply {
                WsReply::Rejected(status, body) => UpstreamConnection::Rejected(WireResponse {
                    status,
                    headers: HeaderMap::new(),
                    body: HttpBody::Bytes(Bytes::from_static(body.as_bytes())),
                }),
                WsReply::Connected(frames) => {
                    let sent = self.sent_frames.clone();
                    UpstreamConnection::Connected {
                        handshake: WireResponse {
                            status: StatusCode::SWITCHING_PROTOCOLS,
                            headers: HeaderMap::new(),
                            body: (),
                        },
                        socket: WebSocket {
                            incoming: Box::pin(futures_util::stream::iter(
                                frames.into_iter().map(Ok),
                            )),
                            outgoing: Box::pin(futures_util::sink::unfold(
                                sent,
                                |sent, frame: WsFrame| async move {
                                    sent.lock().unwrap().push(frame);
                                    Ok::<_, TransportError>(sent)
                                },
                            )),
                        },
                    }
                }
            })
        })
    }
}

pub struct Harness {
    pub core: Core<DatabaseConnection>,
    pub observer: Arc<Recorder>,
    pub client: Arc<ScriptClient>,
    pub channel: Arc<TestChannel>,
}

pub async fn harness(policy: ObservationPolicy, strategy: &str) -> Harness {
    harness_with_storage(policy, strategy, None).await
}

/// Same seed as `harness`, plus an optional file backend for published bodies.
pub async fn harness_with_storage(
    policy: ObservationPolicy,
    strategy: &str,
    storage: Option<gproxy_file::Operator>,
) -> Harness {
    harness_with_publication(policy, strategy, storage, None).await
}

/// A host link builder that maps every publication to `{base}/{id}`, or
/// refuses everything when `base` is `None`.
pub struct FixedLinks(pub Option<&'static str>);
impl gproxy_core::PublicationUrl for FixedLinks {
    fn url_for(&self, publication: &gproxy_core::PublicationRef<'_>) -> Option<String> {
        self.0.map(|base| format!("{base}/{}", publication.id))
    }
}

/// Same seed as `harness_with_storage`, plus an optional host link builder
/// for URL publications.
pub async fn harness_with_publication(
    policy: ObservationPolicy,
    strategy: &str,
    storage: Option<gproxy_file::Operator>,
    links: Option<Arc<dyn gproxy_core::PublicationUrl>>,
) -> Harness {
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
        .create_many(vec![
            provider::ActiveModel {
                id: Set("p".into()),
                name: Set("p".into()),
                channel: Set("test".into()),
                base_url: Set(Some("https://up.example".into())),
                config: Set(json!({"credential_strategy": strategy})),
                created_at_ms: Set(0),
                ..Default::default()
            },
            provider::ActiveModel {
                id: Set("claude".into()),
                name: Set("claude".into()),
                channel: Set("test".into()),
                base_url: Set(Some("https://claude.example".into())),
                config: Set(json!({"dialects": ["claude"]})),
                created_at_ms: Set(0),
                ..Default::default()
            },
            provider::ActiveModel {
                id: Set("ws".into()),
                name: Set("ws".into()),
                channel: Set("test".into()),
                base_url: Set(Some("https://ws.example".into())),
                config: Set(json!({"dialects": ["openai_responses_websocket"]})),
                created_at_ms: Set(0),
                ..Default::default()
            },
            provider::ActiveModel {
                id: Set("claude-buffered".into()),
                name: Set("claude-buffered".into()),
                channel: Set("test".into()),
                base_url: Set(Some("https://buffered.example".into())),
                config: Set(json!({"dialects": ["claude"], "buffered_only": true})),
                created_at_ms: Set(0),
                ..Default::default()
            },
        ])
        .await
        .unwrap();
    let cred = |id: &str, key: &str| credential::ActiveModel {
        id: Set(id.into()),
        provider_id: Set(if id.starts_with("cb") {
            "claude-buffered"
        } else if id.starts_with("ws") {
            "ws"
        } else if id.starts_with("cl") {
            "claude"
        } else {
            "p"
        }
        .into()),
        user_id: Set(Some("u".into())),
        auth_kind: Set("api_key".into()),
        secret: Set(PlaintextCodec.seal(id, &json!({"api_key": key})).unwrap()),
        metadata: Set(json!({})),
        ..Default::default()
    };
    store
        .credentials()
        .create_many(vec![
            cred("a", "ka"),
            cred("b", "kb"),
            cred("cl1", "k1"),
            cred("cl2", "k2"),
            cred("cb1", "kb1"),
            cred("ws1", "kws"),
        ])
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
    let channel = Arc::new(TestChannel::default());
    let core = Core::builder(Arc::new(store))
        .cache(Arc::new(gproxy_cache::MemoryCache::default()))
        .observer(observer.clone())
        .secret_codec(Arc::new(PlaintextCodec))
        .channel(channel.clone())
        .unwrap()
        .file_storage(storage);
    let core = match links {
        Some(links) => core.publication_url(links),
        None => core,
    }
    .build()
    .unwrap();
    core.reload_data().await.unwrap();
    Harness {
        core,
        observer,
        client: Arc::new(ScriptClient::default()),
        channel,
    }
}

/// Add a provider that only speaks `dialect`, with one API-key credential
/// `{id}-key` whose key is `k-{id}`, and publish the new snapshot.
pub async fn seed_provider(h: &Harness, id: &str, base_url: &str, dialect: &str) {
    let store = h.core.store();
    store
        .providers()
        .create_many(vec![provider::ActiveModel {
            id: Set(id.into()),
            name: Set(id.into()),
            channel: Set("test".into()),
            base_url: Set(Some(base_url.into())),
            config: Set(json!({"dialects": [dialect]})),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    let cred_id = format!("{id}-key");
    store
        .credentials()
        .create_many(vec![credential::ActiveModel {
            id: Set(cred_id.clone()),
            provider_id: Set(id.into()),
            user_id: Set(Some("u".into())),
            auth_kind: Set("api_key".into()),
            secret: Set(PlaintextCodec
                .seal(&cred_id, &json!({"api_key": format!("k-{id}")}))
                .unwrap()),
            metadata: Set(json!({})),
            ..Default::default()
        }])
        .await
        .unwrap();
    let revision = h.core.snapshot().revision.0 + 1;
    store
        .settings()
        .update(setting::ActiveModel {
            config_revision: Set(i64::try_from(revision).unwrap()),
            ..Default::default()
        })
        .await
        .unwrap();
    h.core.reload_data().await.unwrap();
}

impl Harness {
    pub fn script(&self, replies: Vec<Reply>) {
        *self.client.replies.lock().unwrap() = replies.into();
    }
    /// Swap the pooled clients for the scripted one, keeping everything else.
    pub fn context(&self, id: &str, attempts: u32, session: Option<&str>) -> Arc<RequestContext> {
        self.context_for("p", KEY, id, attempts, session)
    }
    /// The provider and all its credentials, each wired to the scripted client.
    pub fn target(&self, provider_id: &str) -> ExecutionTarget {
        let snapshot = self.core.snapshot();
        let provider = snapshot.providers[provider_id].clone();
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
                    quota: c.quota.clone(),
                })
            })
            .collect();
        ExecutionTarget {
            provider,
            upstream_model: Some("gpt-x".into()),
            credentials,
        }
    }
    pub fn context_for(
        &self,
        provider_id: &str,
        operation: OperationKey,
        id: &str,
        attempts: u32,
        session: Option<&str>,
    ) -> Arc<RequestContext> {
        let snapshot = self.core.snapshot();
        let target = self.target(provider_id);
        Arc::new(RequestContext {
            request_id: id.into(),
            snapshot: snapshot.clone(),
            scope: "tenant".into(),
            session: session.map(|s| SessionIdentity {
                id: s.into(),
                source: SessionSource::Gateway,
                field: None,
                agent_session_id: None,
            }),
            operation,
            target,
            max_attempts: NonZeroU32::new(attempts).unwrap(),
            started_at_ms: 0,
            deadline: None,
            cancellation: CancellationToken::new(),
        })
    }
}

pub fn request(body: &str) -> WireRequest<HttpBody> {
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

impl Harness {
    pub fn script_ws(&self, replies: Vec<WsReply>) {
        *self.client.ws_replies.lock().unwrap() = replies.into();
    }
}

pub fn json_reply(status: StatusCode, body: serde_json::Value) -> Reply {
    (
        status,
        vec![("content-type", "application/json")],
        vec![Bytes::from(serde_json::to_vec(&body).unwrap())],
    )
}

pub async fn read(body: HttpBody) -> String {
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

pub fn full() -> ObservationPolicy {
    ObservationPolicy {
        usage: true,
        capture: CapturePolicy::Full,
        trace: true,
    }
}
