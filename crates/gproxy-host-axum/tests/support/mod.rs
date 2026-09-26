//! A scripted upstream for the data-plane tests.
//!
//! Trimmed from `gproxy-sdk`'s own harness — a test crate cannot import
//! another crate's test module, so this is the small copy that carries only
//! what these tests need: one channel with a vendor service, one client that
//! answers from a queue, and the seeding helpers that write identity and
//! upstream rows straight through `Store`.
//!
//! Nothing here reaches a network.
#![allow(dead_code)]

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use gproxy_app::{App, AppConfig, Caller, snapshot::encode_key_hash};
use gproxy_channel::{
    BaseChannel, ChannelError,
    channel::{
        CallerRole, ChannelServices, PrepareContext, ProviderView, ServiceContext, ServiceView,
    },
};
use gproxy_core::PlaintextCodec;
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireResponse,
    capability::{
        CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture,
        UpstreamConnection,
    },
    connection::{
        Bytes, HeaderMap, HeaderValue, TransportError, WebSocket as ProtocolSocket, WsFrame,
    },
};
use gproxy_sdk::{
    ClientPool, Gproxy, GproxyBuilder, OutboundClient, SdkError, SecretCodec, SyncMode,
};
use gproxy_seaorm::FixedDecimal;
use gproxy_store::entity::{
    identity::{
        api_key, membership_role::MembershipRole, organization, organization_member, permission,
        team, team_member, user,
    },
    limits::{quota, rate_limit},
    upstream::{credential, provider, provider_model},
    usage::usage_record,
};
use http::StatusCode;
use sea_orm::{DatabaseConnection, Set};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub type Handle = Gproxy<DatabaseConnection>;

/// A channel with one prepared request shape and one vendor service, so the
/// data plane and the service views can be exercised against the same
/// provider rows.
pub struct TestChannel;

impl BaseChannel for TestChannel {
    fn id(&self) -> &'static str {
        "test"
    }

    fn native_dialects(&self, _: ProviderView<'_>, _: Operation) -> Vec<Dialect> {
        vec![
            Dialect::OpenAi,
            Dialect::OpenAiChat,
            Dialect::Claude,
            Dialect::Gemini,
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
        let key = ctx
            .credential
            .secret
            .get("api_key")
            .and_then(Value::as_str)
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

    /// The handshake half of `prepare`, which is what makes this channel's
    /// providers reachable over `/v1/realtime` and friends. A handshake has no
    /// body, and the query has to survive: a realtime continuation names its
    /// call in `?call_id=`.
    fn prepare_connect(
        &self,
        ctx: PrepareContext<'_, ()>,
    ) -> Result<http::Request<()>, ChannelError> {
        let path = match ctx.request.query.as_deref() {
            Some(query) => format!("{}?{query}", ctx.request.path),
            None => ctx.request.path.to_owned(),
        };
        let url = match ctx.endpoint_override {
            Some(url) => url.to_owned(),
            None => format!("{}{path}", ctx.provider.base_url.unwrap_or_default()),
        };
        let key = ctx
            .credential
            .secret
            .get("api_key")
            .and_then(Value::as_str)
            .ok_or(ChannelError::InvalidCredential)?;
        let mut builder = http::Request::builder()
            .method(ctx.request.method)
            .uri(url)
            .header("authorization", format!("Bearer {key}"));
        for (name, value) in &ctx.request.headers {
            builder = builder.header(name, value);
        }
        builder
            .body(())
            .map_err(|_| ChannelError::InvalidCredential)
    }

    fn services(&self) -> Option<&dyn ChannelServices> {
        Some(self)
    }
}

/// The service answers locally with what the host told it about the caller,
/// which is exactly what these tests are asserting on: the role, the view,
/// the synthesized identity, the credential it was given, and the usage the
/// host attributed — the last one proving `ServiceRequest::user_id` arrived,
/// since that is the only thing the caller's token totals are read by.
impl ChannelServices for TestChannel {
    /// Two declared vendor routes, shaped like the real ones: paths that are
    /// nothing like the model API's, so the mount grammar has to recognise
    /// them as surfaces before it will strip a provider prefix in front of
    /// them. The second is a socket, like Codex's remote-control server.
    fn routes(&self) -> &[gproxy_channel::channel::ServiceRoute] {
        use gproxy_channel::channel::{ServiceClass, ServiceRoute, ServiceTransport};
        const ROUTES: &[ServiceRoute] = &[
            ServiceRoute {
                method: http::Method::GET,
                path_template: "/backend-api/wham/usage",
                transport: ServiceTransport::Http,
                idempotent: true,
                class: ServiceClass::Usage,
            },
            ServiceRoute {
                method: http::Method::GET,
                path_template: "/backend-api/wham/remote/control/server",
                transport: ServiceTransport::WebSocket,
                idempotent: true,
                class: ServiceClass::Restricted,
            },
        ];
        ROUTES
    }

    /// The same rule the Codex channel applies to its remote-control socket:
    /// a synthesized view is refused, because the socket pairs a device with
    /// the shared account. A `Credential` view forwards the handshake.
    fn connect<'a>(
        &'a self,
        context: ServiceContext<'a, ()>,
    ) -> gproxy_channel::channel::OperationFuture<'a, UpstreamConnection> {
        Box::pin(async move {
            if context.view.is_synthesized() {
                let mut headers = HeaderMap::new();
                headers.insert("content-type", HeaderValue::from_static("application/json"));
                return Ok(UpstreamConnection::Rejected(WireResponse {
                    status: StatusCode::FORBIDDEN,
                    headers,
                    body: HttpBody::Bytes(Bytes::from_static(
                        br#"{"error":"remote control needs a credential view"}"#,
                    )),
                }));
            }
            let request = http::Request::builder()
                .method(http::Method::GET)
                .uri(format!(
                    "{}{}",
                    context
                        .account
                        .provider
                        .base_url
                        .unwrap_or_default()
                        .replace("https://", "wss://"),
                    context.request.path
                ))
                .body(())
                .unwrap();
            Ok(context.account.client.connect(request).await?)
        })
    }

    fn call<'a>(
        &'a self,
        context: ServiceContext<'a>,
    ) -> gproxy_channel::channel::OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async move {
            let usage = context.caller.usage().await?;
            let body = json!({
                "role": match context.caller.role() {
                    CallerRole::Admin => "admin",
                    CallerRole::Member => "member",
                },
                "view": match &context.view {
                    ServiceView::Caller => "caller".to_owned(),
                    ServiceView::Pool => "pool".to_owned(),
                    ServiceView::Credential(id) => format!("credential:{id}"),
                },
                "identity": context.caller.identity().id,
                "account": context.account.credential.id,
                "path": context.request.path,
                "input_tokens": usage.input_tokens,
            });
            let mut headers = HeaderMap::new();
            headers.insert("content-type", HeaderValue::from_static("application/json"));
            Ok(WireResponse {
                status: StatusCode::OK,
                headers,
                body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&body).unwrap())),
            })
        })
    }
}

/// What the upstream answers next.
pub enum Reply {
    Http(StatusCode, Value),
    /// A streaming body whose end the test decides: every value sent on the
    /// channel becomes a chunk, and dropping the sender ends the response.
    /// This is what makes the lease-lifetime assertions possible — there is no
    /// other way to observe "the response is still being written".
    Stream(StatusCode, tokio::sync::mpsc::UnboundedReceiver<Bytes>),
    /// The same, watched. Every chunk the gateway takes is counted and the body
    /// says when it was let go, which is the only way to see a cancelled call
    /// from outside: an upstream nobody polls any more looks exactly like a slow
    /// one until somebody counts.
    Probed(
        StatusCode,
        tokio::sync::mpsc::UnboundedReceiver<Bytes>,
        Arc<Probe>,
    ),
    /// An upstream that never answers at all, for the disconnect that happens
    /// before a head exists. There is no body to watch, so the probe reports
    /// only whether the gateway gave the call up.
    Stalls(Arc<Probe>),
}

/// What the gateway did with a scripted upstream body.
#[derive(Default)]
pub struct Probe {
    delivered: std::sync::atomic::AtomicUsize,
    dropped: std::sync::atomic::AtomicBool,
}

impl Probe {
    /// Chunks the gateway actually took. A chunk the test queued after the
    /// client left and that is still queued is the assertion.
    pub fn delivered(&self) -> usize {
        self.delivered.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Whether the gateway let the upstream call go — the only thing an
    /// upstream that never answered can be observed by.
    pub fn dropped(&self) -> bool {
        self.dropped.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Holds a probe for as long as the gateway holds the body, and records the
/// release. A plain flag would need somebody to remember to set it.
struct Watched(Arc<Probe>);

impl Drop for Watched {
    fn drop(&mut self) {
        self.0
            .dropped
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

/// What the upstream answers the next handshake with.
pub enum WsReply {
    /// The upstream refused the upgrade. Its whole HTTP response is what the
    /// client must be given, which is the rule the host tests assert.
    Rejected(StatusCode, Value),
    /// The handshake succeeded; the test drives the socket through [`WsPeer`].
    Connected(WsPeer),
}

/// The scripted socket itself, queued on the client and taken once the
/// handshake reaches it.
pub struct WsPeer {
    socket: Mutex<Option<ProtocolSocket>>,
}

/// The upstream end of a scripted socket, held by the test.
///
/// Frames pushed on `send` arrive at the gateway as upstream frames; frames
/// the gateway forwards upstream arrive on `received`. Dropping `send` is how
/// a test makes the upstream vanish without a close frame.
pub struct WsHandle {
    pub send: tokio::sync::mpsc::UnboundedSender<Result<WsFrame, TransportError>>,
    received: tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<WsFrame>>,
}

impl WsHandle {
    /// The next frame the gateway forwarded upstream, or `None` if nothing
    /// arrived before the timeout.
    pub async fn next(&self) -> Option<WsFrame> {
        let mut received = self.received.lock().await;
        tokio::time::timeout(std::time::Duration::from_secs(5), received.recv())
            .await
            .ok()
            .flatten()
    }
}

impl WsPeer {
    /// A scripted socket and the handle that drives it.
    ///
    /// The upstream half behaves like a real peer in the one way the pump
    /// depends on: once the gateway sends it a close frame, its incoming
    /// stream **ends**. That is the second half of RFC 6455's closing
    /// handshake, and core's exchange is only finished when the stream it
    /// wrapped runs out — a harness that kept the stream open for ever would
    /// make every clean close look like a hung one.
    pub fn new() -> (Self, WsHandle) {
        let (send, incoming) = tokio::sync::mpsc::unbounded_channel();
        let (outgoing, received) = tokio::sync::mpsc::unbounded_channel();
        let closed = tokio_util::sync::CancellationToken::new();
        let socket = ProtocolSocket {
            incoming: Box::pin(futures_util::stream::unfold(
                (incoming, closed.clone()),
                |(mut incoming, closed)| async move {
                    tokio::select! {
                        () = closed.cancelled() => None,
                        frame = incoming.recv() => frame.map(|frame| (frame, (incoming, closed))),
                    }
                },
            )),
            outgoing: Box::pin(futures_util::sink::unfold(
                (outgoing, closed),
                |(outgoing, closed), frame: WsFrame| async move {
                    let closing = matches!(frame, WsFrame::Close(_));
                    // A closed receiver means the test stopped listening, not
                    // that the socket broke.
                    let _ = outgoing.send(frame);
                    if closing {
                        closed.cancel();
                    }
                    Ok::<_, TransportError>((outgoing, closed))
                },
            )),
        };
        let handle = WsHandle {
            send,
            received: tokio::sync::Mutex::new(received),
        };
        (
            Self {
                socket: Mutex::new(Some(socket)),
            },
            handle,
        )
    }
}

/// Answers each request from a queue and records the URL it was given, so a
/// test can assert which provider was reached — and, when the queue is
/// untouched, that nothing was sent at all.
#[derive(Default)]
pub struct ScriptClient {
    replies: Mutex<VecDeque<Reply>>,
    sockets: Mutex<VecDeque<WsReply>>,
    seen: Mutex<Vec<String>>,
}

impl ScriptClient {
    pub fn script(&self, replies: Vec<Reply>) {
        *self.replies.lock().unwrap() = replies.into();
    }

    /// Queue one more reply without discarding what is already scripted.
    pub fn push(&self, reply: Reply) {
        self.replies.lock().unwrap().push_back(reply);
    }

    /// Queue one handshake answer.
    pub fn push_socket(&self, reply: WsReply) {
        self.sockets.lock().unwrap().push_back(reply);
    }

    /// Queue one accepted handshake and hand back the test's end of it.
    pub fn accept_socket(&self) -> WsHandle {
        let (peer, handle) = WsPeer::new();
        self.push_socket(WsReply::Connected(peer));
        handle
    }

    /// The request URLs in the order they were sent.
    pub fn urls(&self) -> Vec<String> {
        self.seen.lock().unwrap().clone()
    }
}

impl OutboundClient for ScriptClient {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(request.uri().to_string());
            let reply = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("scripted reply");
            let mut headers = HeaderMap::new();
            headers.insert("content-type", HeaderValue::from_static("application/json"));
            let (status, body) = match reply {
                Reply::Http(status, body) => {
                    let body = Bytes::from(serde_json::to_vec(&body).unwrap());
                    // As a real upstream sends it: a buffered answer declares
                    // its length, and the gateway relays the header.
                    headers.insert(
                        "content-length",
                        HeaderValue::from_str(&body.len().to_string()).unwrap(),
                    );
                    (status, HttpBody::Bytes(body))
                }
                Reply::Stream(status, receiver) => (
                    status,
                    HttpBody::Stream(Box::pin(futures_util::stream::unfold(
                        receiver,
                        |mut receiver| async move {
                            receiver.recv().await.map(|bytes| (Ok(bytes), receiver))
                        },
                    ))),
                ),
                Reply::Probed(status, receiver, probe) => (
                    status,
                    HttpBody::Stream(Box::pin(futures_util::stream::unfold(
                        (receiver, Watched(probe)),
                        |(mut receiver, watched)| async move {
                            let bytes = receiver.recv().await?;
                            watched
                                .0
                                .delivered
                                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                            Some((Ok(bytes), (receiver, watched)))
                        },
                    ))),
                ),
                Reply::Stalls(probe) => {
                    // Never resolves. The guard is what the test reads: it is
                    // dropped exactly when the gateway stops waiting for this
                    // answer.
                    let _watched = Watched(probe);
                    std::future::pending::<(StatusCode, HttpBody)>().await
                }
            };
            Ok(WireResponse {
                status,
                headers,
                body,
            })
        })
    }

    fn connect<'a>(
        &'a self,
        request: http::Request<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(request.uri().to_string());
            let reply = self.sockets.lock().unwrap().pop_front();
            match reply {
                Some(WsReply::Connected(peer)) => {
                    let socket = peer
                        .socket
                        .lock()
                        .unwrap()
                        .take()
                        .expect("a scripted socket is connected once");
                    let mut headers = HeaderMap::new();
                    headers.insert("x-upstream-handshake", HeaderValue::from_static("ok"));
                    Ok(UpstreamConnection::Connected {
                        handshake: WireResponse {
                            status: StatusCode::SWITCHING_PROTOCOLS,
                            headers,
                            body: (),
                        },
                        socket,
                    })
                }
                Some(WsReply::Rejected(status, body)) => {
                    let mut headers = HeaderMap::new();
                    headers.insert("content-type", HeaderValue::from_static("application/json"));
                    Ok(UpstreamConnection::Rejected(WireResponse {
                        status,
                        headers,
                        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&body).unwrap())),
                    }))
                }
                None => Err(CapabilityError::new(
                    CapabilityErrorKind::Unsupported,
                    CapabilityErrorStage::Start,
                    "no websocket scripted",
                )),
            }
        })
    }
}

/// A handle on a private in-memory database with the scripted channel and
/// client, and no synchronization of its own: the tests reload explicitly.
pub async fn handle() -> (Handle, Arc<ScriptClient>) {
    let client = Arc::new(ScriptClient::default());
    let gproxy = GproxyBuilder::sqlite_memory()
        .await
        .unwrap()
        .plaintext_secrets()
        .without_default_channels()
        .channel(Arc::new(TestChannel))
        .client_pool(ClientPool::with_client(client.clone()))
        .cache(Arc::new(
            gproxy_cache::MemoryCache::new(gproxy_cache::MemoryOptions::default()).unwrap(),
        ))
        .sync_mode(SyncMode::Manual)
        .build()
        .await
        .unwrap();
    (gproxy, client)
}

/// Advance the durable revision and republish both snapshots, the way a
/// peer's management write would be seen here.
pub async fn publish(app: &App<DatabaseConnection>) {
    app.gproxy().store().commit_revision(vec![]).await.unwrap();
    app.reload_all().await.unwrap();
}

pub fn digest_of(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

/// The caller behind a seeded key, through the real authenticator: the
/// binding the data plane enforces has to be the one authentication produced.
pub async fn caller_for(app: &App<DatabaseConnection>, key: &str) -> Caller {
    let data = app.data();
    app.authenticator(&data)
        .authenticate_token(key)
        .await
        .unwrap()
}

// ---------------------------------------------------------------- seeding --

pub async fn person(handle: &Handle, id: &str, role: &str) {
    handle
        .store()
        .users()
        .create_many(vec![user::ActiveModel {
            id: Set(id.into()),
            name: Set(id.into()),
            role: Set(role.into()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
}

/// An API key whose plaintext is its own id, optionally bound to an
/// organization or a team.
pub async fn api_key(
    handle: &Handle,
    id: &str,
    user_id: &str,
    organization_id: Option<&str>,
    team_id: Option<&str>,
) {
    handle
        .store()
        .api_keys()
        .create_many(vec![api_key::ActiveModel {
            id: Set(id.into()),
            user_id: Set(user_id.into()),
            name: Set(id.into()),
            key_hash: Set(encode_key_hash(&digest_of(id))),
            prefix: Set("sk-".into()),
            organization_id: Set(organization_id.map(Into::into)),
            team_id: Set(team_id.map(Into::into)),
            // Test keys may manage: the suites that use them exercise scope and
            // administration, which predate the flag. The flag has its own test.
            management: Set(true),
            ..Default::default()
        }])
        .await
        .unwrap();
}

pub async fn organization(handle: &Handle, id: &str) {
    handle
        .store()
        .organizations()
        .create_many(vec![organization::ActiveModel {
            id: Set(id.into()),
            name: Set(id.into()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
}

pub async fn team(handle: &Handle, id: &str, organization_id: &str) {
    handle
        .store()
        .teams()
        .create_many(vec![team::ActiveModel {
            id: Set(id.into()),
            organization_id: Set(organization_id.into()),
            name: Set(id.into()),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
}

pub async fn org_member(handle: &Handle, org: &str, user: &str, role: MembershipRole) {
    handle
        .store()
        .organization_members()
        .create_many(vec![organization_member::ActiveModel {
            organization_id: Set(org.into()),
            user_id: Set(user.into()),
            role: Set(role),
        }])
        .await
        .unwrap();
}

pub async fn team_member(handle: &Handle, team: &str, user: &str, role: MembershipRole) {
    handle
        .store()
        .team_members()
        .create_many(vec![team_member::ActiveModel {
            team_id: Set(team.into()),
            user_id: Set(user.into()),
            role: Set(role),
        }])
        .await
        .unwrap();
}

/// An `allow` rule over one provider (or every provider) and one model glob.
pub async fn allow(handle: &Handle, id: &str, user_id: &str, provider_id: Option<&str>) {
    handle
        .store()
        .permissions()
        .create_many(vec![permission::ActiveModel {
            id: Set(id.into()),
            user_id: Set(Some(user_id.into())),
            provider_id: Set(provider_id.map(Into::into)),
            model_pattern: Set("*".into()),
            action: Set("allow".into()),
            priority: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
}

/// A provider on the `test` channel, reachable at `https://{id}.example`,
/// whose catalog lists `models`.
pub async fn provider(handle: &Handle, id: &str, models: &[&str]) {
    handle
        .store()
        .providers()
        .create_many(vec![provider::ActiveModel {
            id: Set(id.into()),
            name: Set(id.into()),
            channel: Set("test".into()),
            base_url: Set(Some(format!("https://{id}.example"))),
            config: Set(json!({})),
            created_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
    handle
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

/// An enabled API-key credential whose key is `k-{id}`, owned by whichever of
/// the three columns is set.
pub async fn credential(
    handle: &Handle,
    id: &str,
    provider_id: &str,
    user: Option<&str>,
    team: Option<&str>,
    org: Option<&str>,
) {
    handle
        .store()
        .credentials()
        .create_many(vec![credential::ActiveModel {
            id: Set(id.into()),
            provider_id: Set(provider_id.into()),
            user_id: Set(user.map(Into::into)),
            team_id: Set(team.map(Into::into)),
            organization_id: Set(org.map(Into::into)),
            auth_kind: Set("api_key".into()),
            secret: Set(PlaintextCodec
                .seal(id, &json!({"api_key": format!("k-{id}")}))
                .unwrap()),
            metadata: Set(json!({})),
            enabled: Set(true),
            ..Default::default()
        }])
        .await
        .unwrap();
}

/// A cost budget that is spent the moment it is checked: limit zero over a
/// permanent window.
pub async fn spent_budget(handle: &Handle, id: &str, owner_kind: &str, owner_id: &str) {
    handle
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

/// A per-window request limit on one user.
pub async fn rate_limit(handle: &Handle, id: &str, user_id: &str, value: i64, period_seconds: i64) {
    handle
        .store()
        .rate_limits()
        .create_many(vec![rate_limit::ActiveModel {
            id: Set(id.into()),
            user_id: Set(Some(user_id.into())),
            api_key_id: Set(None),
            metric: Set("requests".into()),
            limit_value: Set(FixedDecimal::from_atoms(value * FixedDecimal::FACTOR)),
            period_seconds: Set(period_seconds),
            model_pattern: Set(Some("*".into())),
            enabled: Set(true),
        }])
        .await
        .unwrap();
}

/// A settled usage row, so the `Caller` service view has something to report.
pub async fn usage_row(handle: &Handle, request_id: &str, user_id: &str, input_tokens: u64) {
    handle
        .store()
        .usage_records()
        .create_many(vec![usage_record::ActiveModel {
            request_id: Set(request_id.into()),
            user_id: Set(Some(user_id.into())),
            model: Set("test/m1".into()),
            operation: Set("generate_content".into()),
            metrics: Set(json!({"input_tokens": input_tokens})),
            started_at_ms: Set(0),
            ..Default::default()
        }])
        .await
        .unwrap();
}

// ------------------------------------------------------------- assertions --

pub fn generate() -> OperationKey {
    OperationKey {
        operation: Operation::GenerateContent,
        dialect: Dialect::OpenAi,
    }
}

/// One POST to `/v1/responses` with a JSON body, split into the parts and
/// bytes a host would hand over.
pub fn parts(body: Value) -> (http::request::Parts, Bytes) {
    let request = http::Request::builder()
        .method(http::Method::POST)
        .uri("/v1/responses")
        .body(())
        .unwrap();
    let (parts, ()) = request.into_parts();
    (parts, Bytes::from(serde_json::to_vec(&body).unwrap()))
}

/// The same for a vendor service path, which carries no model.
pub fn service_parts(path: &str) -> (http::request::Parts, Bytes) {
    let request = http::Request::builder()
        .method(http::Method::GET)
        .uri(path)
        .body(())
        .unwrap();
    let (parts, ()) = request.into_parts();
    (parts, Bytes::new())
}

/// Drain an HTTP body into a JSON value.
pub async fn read_json(body: HttpBody) -> Value {
    serde_json::from_slice(&read_bytes(body).await).unwrap()
}

pub async fn read_bytes(body: HttpBody) -> Vec<u8> {
    use futures_util::StreamExt;
    match body {
        HttpBody::Bytes(bytes) => bytes.to_vec(),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                out.extend_from_slice(&chunk.unwrap());
            }
            out
        }
    }
}

/// An application over a fresh in-memory instance. The caller seeds through
/// `app.gproxy().store()` and then calls [`publish`].
pub async fn app() -> (App<DatabaseConnection>, Arc<ScriptClient>) {
    let (gproxy, client) = handle().await;
    (App::new(gproxy, AppConfig::default()), client)
}

/// So a test can name the sdk failure it expects without importing the crate.
pub fn is_no_target(error: &SdkError) -> bool {
    matches!(error, SdkError::NoTarget(_))
}

// -------------------------------------------------------------- the host --

use axum::{Router, body::Body};
use gproxy_app::Operations;
use gproxy_host_axum::HostState;
use gproxy_store::entity::routing::{route, route_member};
use http::Request;
use tower::ServiceExt as _;

/// An assembled instance behind a router, over a fresh in-memory database.
///
/// The caller seeds through `host.handle()` (or the helpers above) and then
/// calls [`Host::publish`]; the router reads the published snapshots on every
/// request, so a seed that has not been published is invisible exactly as it
/// would be in production.
pub struct Host {
    pub app: Arc<App<DatabaseConnection>>,
    pub client: Arc<ScriptClient>,
    pub router: Router,
}

impl Host {
    pub async fn new() -> Self {
        Self::with_config(AppConfig::default()).await
    }

    pub async fn with_config(config: AppConfig) -> Self {
        let (gproxy, client) = handle().await;
        // Match native first-start initialization: subsequent policy reads use the row.
        gproxy
            .store()
            .settings()
            .update(gproxy_store::entity::config::setting::ActiveModel {
                cors_origins: sea_orm::Set(serde_json::json!(config.cors_origins)),
                trusted_proxies: sea_orm::Set(serde_json::json!(config.trusted_proxies)),
                ..Default::default()
            })
            .await
            .unwrap();
        let app = Arc::new(App::new(gproxy, config));
        let router = gproxy_host_axum::router(HostState::new(app.clone()));
        Self {
            app,
            client,
            router,
        }
    }

    pub fn handle(&self) -> Handle {
        self.app.gproxy().clone()
    }

    /// Republish both snapshots, the way a management write would.
    pub async fn publish(&self) {
        publish(&self.app).await;
    }

    /// Drive one request through the whole router.
    pub async fn send(&self, request: Request<Body>) -> Answer {
        let response = self.raw(request).await;
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = axum::body::to_bytes(response.into_body(), 16 * 1024 * 1024)
            .await
            .unwrap();
        // A response hands its settlement off as it ends; a test reads what
        // the request wrote, so it waits for that too.
        gproxy_host_axum::response::settled().await;
        Answer {
            status,
            headers,
            bytes,
        }
    }

    /// The same, without draining the body — for the tests that need to
    /// observe an instance *while* a response is still being written.
    pub async fn raw(&self, request: Request<Body>) -> axum::response::Response {
        self.router.clone().oneshot(request).await.unwrap()
    }

    /// Bind the router to a loopback port and serve it until the returned
    /// value is dropped.
    ///
    /// Only the websocket tests need this. `oneshot` never produces hyper's
    /// `OnUpgrade` extension — an upgrade is made of it — so a handshake
    /// driven through the service directly can only ever be refused. Anything
    /// that is answered over HTTP is still driven with `oneshot`.
    pub async fn bind(&self) -> Bound {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let router = self.router.clone();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, router.into_make_service()).await;
        });
        Bound { address, server }
    }

    /// The identity snapshot, for seeding through the same families the API
    /// uses. Held by the caller and passed to [`Host::operations`], because
    /// `Operations` borrows it — the same one-load-per-request rule the host
    /// itself follows.
    pub fn data(&self) -> Arc<gproxy_app::AppData> {
        self.app.data()
    }

    pub fn operations<'a>(
        &'a self,
        data: &'a gproxy_app::AppData,
    ) -> Operations<'a, DatabaseConnection> {
        Operations::new(self.app.gproxy(), data, self.app.config())
    }
}

/// A router served on a real loopback port, torn down when this is dropped.
pub struct Bound {
    pub address: std::net::SocketAddr,
    server: tokio::task::JoinHandle<()>,
}

impl Bound {
    /// The `ws://` URL of one path on this instance.
    pub fn ws(&self, path: &str) -> String {
        format!("ws://{}{path}", self.address)
    }

    /// The `http://` URL of one path on this instance.
    pub fn http(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }
}

impl Drop for Bound {
    fn drop(&mut self) {
        self.server.abort();
    }
}

/// One drained response.
pub struct Answer {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub bytes: Bytes,
}

impl Answer {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.bytes)
            .unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(&self.bytes)))
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name)?.to_str().ok()
    }
}

/// The authority every built request claims, because a real HTTP/1.1 client
/// always sends one and some answers (the OAuth issuer identifier) are derived
/// from it.
pub const HOST: &str = "gproxy.local";

/// A `POST` with a JSON body.
pub fn post(uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(http::Method::POST)
        .uri(uri)
        .header("host", HOST)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap()
}

/// A `POST` with a form body, which is what an OAuth client sends.
pub fn form(uri: &str, pairs: &[(&str, &str)]) -> Request<Body> {
    Request::builder()
        .method(http::Method::POST)
        .uri(uri)
        .header("host", HOST)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(serde_urlencoded::to_string(pairs).unwrap()))
        .unwrap()
}

pub fn get(uri: &str) -> Request<Body> {
    request(http::Method::GET, uri)
}

pub fn request(method: http::Method, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("host", HOST)
        .body(Body::empty())
        .unwrap()
}

/// Set a header on a built request, replacing any the builder already put
/// there — `host` in particular, which every request carries by default.
pub fn with(mut request: Request<Body>, name: &str, value: &str) -> Request<Body> {
    request.headers_mut().insert(
        http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
        HeaderValue::from_str(value).unwrap(),
    );
    request
}

/// A key as a client presents it.
pub fn keyed(request: Request<Body>, key: &str) -> Request<Body> {
    with(request, "authorization", &format!("Bearer {key}"))
}

/// An exposed model name `{namespace}/{name}` over one provider, which is what
/// creates the namespace mount `/{namespace}`.
pub async fn exposed(handle: &Handle, name: &str, provider_id: &str, upstream_model: &str) {
    let id = name.replace('/', "-");
    handle
        .store()
        .routes()
        .create_many(vec![route::ActiveModel {
            id: Set(id.clone()),
            name: Set(name.into()),
            enabled: Set(true),
            max_attempts: Set(1),
            ..Default::default()
        }])
        .await
        .unwrap();
    handle
        .store()
        .route_members()
        .create_many(vec![route_member::ActiveModel {
            id: Set(format!("{id}-m1")),
            route_id: Set(id.clone()),
            provider_id: Set(provider_id.into()),
            upstream_model: Set(upstream_model.into()),
            enabled: Set(true),
            ..Default::default()
        }])
        .await
        .unwrap();
}
