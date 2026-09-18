//! Scripted channel, client and observer shared by the execution test files.
#![allow(dead_code)]

use futures_util::StreamExt;
use gproxy_channel::{
    BaseChannel, ChannelError, OutboundClient,
    channel::{NormalizedUsage, PrepareContext, ProviderView, UsageContext, UsageExtractor},
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
    sync::{Arc, Mutex},
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

pub struct TestChannel;
impl BaseChannel for TestChannel {
    fn id(&self) -> &'static str {
        "test"
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
}

pub async fn harness(policy: ObservationPolicy, strategy: &str) -> Harness {
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
    pub fn script(&self, replies: Vec<Reply>) {
        *self.client.replies.lock().unwrap() = replies.into();
    }
    /// Swap the pooled clients for the scripted one, keeping everything else.
    pub fn context(&self, id: &str, attempts: u32, session: Option<&str>) -> Arc<RequestContext> {
        self.context_for("p", KEY, id, attempts, session)
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
            operation,
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
