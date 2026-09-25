//! Durable fixtures for the resolution and call tests.
//!
//! Rows are written straight through `Store`, the way `gproxy-core`'s own
//! harness seeds: the management families are another phase's business and a
//! test of resolution should not depend on them. Every writer bumps
//! `config_revision` through `commit_revision` and reloads, so the handle sees
//! exactly what a peer's write would have produced.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use futures_util::SinkExt;
use gproxy_channel::{
    BaseChannel, ChannelError,
    channel::{PrepareContext, ProviderView, QuotaScope},
};
use gproxy_core::{
    BlockSource, CapturePolicy, CaptureSink, CredentialBlock, CredentialBlocks, ExchangeContext,
    ObservationPolicy, Observer, PlaintextCodec, RequestContext, SecretCodec, SessionIdentity,
    StoreObserver, TraceEvent, UsageReport, keys,
};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, WireResponse,
    capability::{
        CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture,
        UpstreamConnection,
    },
    connection::{Bytes, HeaderMap, HeaderValue, TransportError, WebSocket},
};
use gproxy_sdk::{ClientPool, Gproxy, GproxyBuilder, OutboundClient, SyncMode};
use gproxy_store::entity::{
    limits::quota,
    routing::{route, route_member},
    upstream::{credential, provider, provider_model},
};
use http::StatusCode;
use sea_orm::{DatabaseConnection, Set};
use serde_json::{Value, json};

use super::{Log, TestChannel};

pub type Handle = Gproxy<DatabaseConnection>;

/// A second channel, so a test can tell "restricted to this channel" from
/// "restricted to this provider". Same preparation as [`TestChannel`]: an
/// absolute URL from the provider's base plus the request path, and the
/// credential's key as a bearer token.
pub struct AltChannel;

impl BaseChannel for AltChannel {
    fn quota_model(&self) -> Option<&dyn gproxy_channel::channel::QuotaModel> {
        Some(self)
    }
    fn id(&self) -> &'static str {
        "alt"
    }
    fn native_dialects(&self, _: ProviderView<'_>, _: Operation) -> Vec<Dialect> {
        vec![
            Dialect::OpenAi,
            Dialect::OpenAiChat,
            Dialect::Claude,
            Dialect::Gemini,
            // So a websocket handshake is passthrough here and the connect
            // entry can be exercised without a conversion envelope.
            Dialect::OpenAiResponsesWebSocket,
        ]
    }
    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let url = match ctx.endpoint_override {
            Some(url) => url.to_owned(),
            None => format!(
                "{}{}",
                ctx.provider.base_url.unwrap_or_default(),
                ctx.request.path
            ),
        };
        let key = ["api_key", "access_token"]
            .iter()
            .find_map(|field| ctx.credential.secret.get(*field).and_then(Value::as_str))
            .ok_or(ChannelError::InvalidCredential)?;
        let mut builder = http::Request::builder()
            .method(ctx.request.method)
            .uri(url)
            .header("authorization", format!("Bearer {key}"));
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
        let base = ctx
            .provider
            .base_url
            .unwrap_or_default()
            .replace("https", "wss");
        http::Request::builder()
            .method(ctx.request.method)
            .uri(format!("{base}{}", ctx.request.path))
            .header(
                "authorization",
                format!(
                    "Bearer {}",
                    ctx.credential.secret["api_key"].as_str().unwrap()
                ),
            )
            .body(())
            .map_err(|_| ChannelError::InvalidCredential)
    }
}

/// What the upstream answers next. A transport failure is a distinct variant
/// because a rejected HTTP status is still a successful `send`.
pub enum Reply {
    Http(StatusCode, Value),
    DelayedHttp(std::time::Duration, StatusCode, Value),
    Transport(&'static str),
}

/// What the upstream does with the next handshake.
pub enum WsReply {
    /// The upgrade was refused; the complete HTTP response is retained.
    Rejected(StatusCode),
    /// The socket is open and immediately silent.
    Connected,
}

/// Answers each request from a queue and records the URL and body it was
/// given, so a test can assert which provider was reached and what was
/// forwarded to it.
#[derive(Default)]
pub struct SeedClient {
    pub authorizations: Mutex<Vec<String>>,
    pub replies: Mutex<VecDeque<Reply>>,
    pub ws_replies: Mutex<VecDeque<WsReply>>,
    pub seen: Arc<Log>,
}

impl SeedClient {
    pub fn script(&self, replies: Vec<Reply>) {
        *self.replies.lock().unwrap() = replies.into();
    }
    pub fn script_ws(&self, replies: Vec<WsReply>) {
        *self.ws_replies.lock().unwrap() = replies.into();
    }
    /// The request URLs in the order they were sent.
    pub fn urls(&self) -> Vec<String> {
        self.seen
            .lines()
            .iter()
            .filter_map(|line| line.split_whitespace().nth(1).map(str::to_owned))
            .collect()
    }
}

impl OutboundClient for SeedClient {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            self.authorizations.lock().unwrap().push(
                request
                    .headers()
                    .get("authorization")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
            );
            let (parts, body) = request.into_parts();
            let body = super::read(body).await;
            let gateway = parts
                .headers
                .get(gproxy_sdk::GATEWAY_SESSION_HEADER)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("-");
            self.seen.push(format!(
                "{} {} gateway={gateway} body={body}",
                parts.method, parts.uri,
            ));
            let reply = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("scripted reply");
            let (status, body) = match reply {
                Reply::Http(status, body) => (status, body),
                Reply::DelayedHttp(delay, status, body) => {
                    tokio::time::sleep(delay).await;
                    (status, body)
                }
                Reply::Transport(reason) => {
                    return Err(CapabilityError::new(
                        CapabilityErrorKind::Transport,
                        CapabilityErrorStage::Start,
                        reason,
                    ));
                }
            };
            let mut headers = HeaderMap::new();
            headers.insert("content-type", HeaderValue::from_static("application/json"));
            Ok(WireResponse {
                status,
                headers,
                body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&body).unwrap())),
            })
        })
    }

    fn connect<'a>(
        &'a self,
        request: http::Request<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(async move {
            self.authorizations.lock().unwrap().push(
                request
                    .headers()
                    .get("authorization")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
            );
            self.seen
                .push(format!("WS {} gateway=- body=", request.uri()));
            Ok(
                match self
                    .ws_replies
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("scripted handshake")
                {
                    WsReply::Rejected(status) => UpstreamConnection::Rejected(WireResponse {
                        status,
                        headers: HeaderMap::new(),
                        body: HttpBody::Bytes(Bytes::from_static(b"refused")),
                    }),
                    WsReply::Connected => UpstreamConnection::Connected {
                        handshake: WireResponse {
                            status: StatusCode::SWITCHING_PROTOCOLS,
                            headers: HeaderMap::new(),
                            body: (),
                        },
                        socket: WebSocket {
                            incoming: Box::pin(futures_util::stream::empty()),
                            outgoing: Box::pin(futures_util::sink::drain().sink_map_err(
                                |_: std::convert::Infallible| -> TransportError {
                                    unreachable!("draining never fails")
                                },
                            )),
                        },
                    },
                },
            )
        })
    }
}

/// Records the session identity of every request core observed, then defers to
/// the ordinary Store observation so usage rows are still written.
pub struct SeedObserver {
    inner: StoreObserver<DatabaseConnection>,
    pub sessions: Mutex<Vec<(String, SessionIdentity)>>,
}

impl SeedObserver {
    /// The `(request id, session)` pairs seen so far.
    pub fn seen(&self) -> Vec<(String, SessionIdentity)> {
        self.sessions
            .lock()
            .unwrap()
            .iter()
            .map(|(id, session)| (id.clone(), session.clone()))
            .collect()
    }
}

impl Observer for SeedObserver {
    fn policy(&self, request: &RequestContext) -> ObservationPolicy {
        if let Some(session) = request.session.clone() {
            self.sessions
                .lock()
                .unwrap()
                .push((request.request_id.clone(), session));
        }
        self.inner.policy(request)
    }
    fn capture(&self, exchange: &ExchangeContext, policy: CapturePolicy) -> Box<dyn CaptureSink> {
        self.inner.capture(exchange, policy)
    }
    fn usage<'a>(
        &'a self,
        request: &'a RequestContext,
        report: &'a UsageReport,
    ) -> CapabilityFuture<'a, ()> {
        self.inner.usage(request, report)
    }
    fn trace(&self, event: TraceEvent<'_>) {
        self.inner.trace(event);
    }
}

/// A handle over a private in-memory database with both test channels, a
/// scripted client and a recording observer. Synchronization is manual: the
/// seeding helpers reload explicitly.
pub async fn handle() -> (Handle, Arc<SeedClient>, Arc<SeedObserver>) {
    let client = Arc::new(SeedClient::default());
    // The connection is opened here rather than by `GproxyBuilder::sqlite_memory`
    // so the observer can be built over the same database before the handle
    // exists. min = max = 1 is what keeps a `sqlite::memory:` database alive.
    let mut options = sea_orm::ConnectOptions::new("sqlite::memory:");
    options
        .min_connections(1)
        .max_connections(1)
        .sqlx_logging(false);
    let connection = sea_orm::Database::connect(options).await.unwrap();
    let observer = Arc::new(SeedObserver {
        inner: StoreObserver::new(Arc::new(gproxy_store::Store::new(connection.clone()))),
        sessions: Mutex::new(Vec::new()),
    });
    let gproxy = GproxyBuilder::connection(connection)
        .plaintext_secrets()
        .without_default_channels()
        .channel(Arc::new(TestChannel::default()))
        .channel(Arc::new(AltChannel))
        .client_pool(ClientPool::with_client(client.clone()))
        .observer(observer.clone())
        .sync_mode(SyncMode::Manual)
        .build()
        .await
        .unwrap();
    (gproxy, client, observer)
}

/// Advance the durable revision and reload, the way a peer's management write
/// would be seen here.
pub async fn publish(gproxy: &Handle) {
    gproxy.store().commit_revision(vec![]).await.unwrap();
    gproxy.reload().await.unwrap();
}

/// A provider on `channel`, reachable at `https://{id}.example`, whose model
/// catalog lists `models`.
pub async fn provider(gproxy: &Handle, id: &str, channel: &str, models: &[&str]) {
    provider_named(gproxy, id, id, channel, models, json!({})).await;
}

/// The same, with an explicit public name and provider config — the name a
/// `provider/model` prefix matches, and the `credential_strategy` a sticky
/// session needs.
pub async fn provider_named(
    gproxy: &Handle,
    id: &str,
    name: &str,
    channel: &str,
    models: &[&str],
    config: Value,
) {
    gproxy
        .store()
        .providers()
        .create_many(vec![provider::ActiveModel {
            id: Set(id.into()),
            name: Set(name.into()),
            channel: Set(channel.into()),
            base_url: Set(Some(format!("https://{id}.example"))),
            config: Set(config),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    if !models.is_empty() {
        gproxy
            .store()
            .provider_models()
            .create_many(
                models
                    .iter()
                    .map(|model| provider_model::ActiveModel {
                        id: Set(format!("{id}-{model}")),
                        provider_id: Set(id.into()),
                        upstream_name: Set((*model).into()),
                        metadata: Set(json!({})),
                        ..Default::default()
                    })
                    .collect(),
            )
            .await
            .unwrap();
    }
}

/// An enabled API-key credential whose key is `k-{id}`.
pub async fn credential(gproxy: &Handle, id: &str, provider_id: &str) {
    write_credential(
        gproxy,
        id,
        provider_id,
        true,
        credential::CredentialStatus::Active,
    )
    .await;
}

/// A credential the operator switched off.
pub async fn disabled_credential(gproxy: &Handle, id: &str, provider_id: &str) {
    write_credential(
        gproxy,
        id,
        provider_id,
        false,
        credential::CredentialStatus::Active,
    )
    .await;
}

/// A credential whose refresh was definitively refused.
pub async fn dead_credential(gproxy: &Handle, id: &str, provider_id: &str) {
    write_credential(
        gproxy,
        id,
        provider_id,
        true,
        credential::CredentialStatus::Dead,
    )
    .await;
}

async fn write_credential(
    gproxy: &Handle,
    id: &str,
    provider_id: &str,
    enabled: bool,
    status: credential::CredentialStatus,
) {
    gproxy
        .store()
        .credentials()
        .create_many(vec![credential::ActiveModel {
            id: Set(id.into()),
            provider_id: Set(provider_id.into()),
            auth_kind: Set("api_key".into()),
            secret: Set(PlaintextCodec
                .seal(id, &json!({"api_key": format!("k-{id}")}))
                .unwrap()),
            metadata: Set(json!({})),
            enabled: Set(enabled),
            status: Set(status),
            ..Default::default()
        }])
        .await
        .unwrap();
}

/// One route member: `(member id, provider id, upstream model, tier, weight)`.
pub type Member<'a> = (&'a str, &'a str, &'a str, u32, u32);

/// A route with its members, exposed under `name`.
pub async fn route(
    gproxy: &Handle,
    id: &str,
    name: &str,
    strategy: route::RouteStrategy,
    max_attempts: u32,
    members: &[Member<'_>],
) {
    gproxy
        .store()
        .routes()
        .create_many(vec![route::ActiveModel {
            id: Set(id.into()),
            name: Set(name.into()),
            strategy: Set(strategy),
            max_attempts: Set(max_attempts),
            ..Default::default()
        }])
        .await
        .unwrap();
    gproxy
        .store()
        .route_members()
        .create_many(
            members
                .iter()
                .map(
                    |(member, provider, model, tier, weight)| route_member::ActiveModel {
                        id: Set((*member).into()),
                        route_id: Set(id.into()),
                        provider_id: Set((*provider).into()),
                        upstream_model: Set((*model).into()),
                        tier: Set(*tier),
                        weight: Set(*weight),
                        ..Default::default()
                    },
                )
                .collect(),
        )
        .await
        .unwrap();
}

/// A cost budget that is spent the moment it is checked: limit zero over a
/// permanent window.
pub async fn spent_budget(gproxy: &Handle, id: &str, owner_kind: &str, owner_id: &str) {
    gproxy
        .store()
        .quotas()
        .create_many(vec![quota::ActiveModel {
            id: Set(id.into()),
            owner_kind: Set(owner_kind.into()),
            owner_id: Set(owner_id.into()),
            window_key: Set(format!("{id}-key")),
            metric: Set("cost".into()),
            unit: Set("USD".into()),
            limit_value: Set("0".parse().unwrap()),
            period: Set("total".into()),
            ..Default::default()
        }])
        .await
        .unwrap();
}

/// Rate-limit a credential for every model and operation for the next hour,
/// by writing the cache payload core's own selection reads.
pub async fn block(gproxy: &Handle, provider_id: &str, credential_id: &str) {
    let now = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let blocks = CredentialBlocks {
        blocks: vec![CredentialBlock {
            scope: QuotaScope::All,
            operation: None,
            until_ms: now + 3_600_000,
            source: BlockSource::RateLimited,
            observed_at_ms: now,
        }],
        failures: Vec::new(),
        last_success_at_ms: None,
    };
    gproxy
        .cache()
        .put(
            &keys::credential_blocks(provider_id, credential_id),
            serde_json::to_vec(&blocks).unwrap(),
            std::time::Duration::from_secs(3600),
        )
        .await
        .unwrap();
}

impl gproxy_channel::channel::QuotaModel for AltChannel {
    fn dimensions(
        &self,
        provider: ProviderView<'_>,
        credential: gproxy_channel::channel::CredentialView<'_>,
    ) -> Vec<gproxy_channel::channel::QuotaDimension> {
        gproxy_channel::channel::QuotaModel::dimensions(
            &TestChannel::default(),
            provider,
            credential,
        )
    }
}
