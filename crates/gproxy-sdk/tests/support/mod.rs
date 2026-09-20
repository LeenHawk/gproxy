//! Scripted channel and client shared by the handle's test files.
//!
//! Trimmed from `gproxy-core`'s own harness: the same `TestChannel` identity
//! and request preparation, plus the three login flows and the refresh and
//! quota abilities, which is what the management and login families exercise.
//! Nothing here reaches a network; every upstream answer is scripted.
#![allow(dead_code)]

pub mod seed;

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use futures_util::StreamExt;
use gproxy_channel::{
    BaseChannel, ChannelError, OutboundClient,
    channel::{
        AcquiredCredential, AuthorizationCode, AuthorizationRequest, AuthorizationStart,
        CookieLogin, CredentialContext, CredentialRefresh, CredentialUpdate, CredentialView,
        DeviceAuthorization, DevicePoll, LoginContext, OAuthAuthorizationCode, OAuthCredential,
        OAuthDeviceCode, OperationFuture, PrepareContext, ProviderView, QuotaQuery, QuotaSnapshot,
    },
};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, WireRequest, WireResponse,
    capability::{CapabilityError, CapabilityFuture, UpstreamConnection},
    connection::{Bytes, TransportError, WebSocket, WsFrame},
};
use gproxy_sdk::{Cache, ClientPool, Gproxy, GproxyBuilder, SyncMode};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use sea_orm::DatabaseConnection;
use serde_json::{Value, json};

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

pub enum RefreshReply {
    Rotated {
        api_key: &'static str,
        expires_at_ms: Option<i64>,
    },
    Rejected(&'static str),
    Failed,
}

/// A channel with an identity, one prepared request shape and every optional
/// ability the handle drives. Each ability answers from its own queue, so a
/// test scripts exactly the steps it exercises.
#[derive(Default)]
pub struct TestChannel {
    pub refreshes: Mutex<VecDeque<RefreshReply>>,
    pub refresh_calls: Mutex<Vec<(String, i64)>>,
    pub quota_snapshots: Mutex<VecDeque<QuotaSnapshot>>,
    /// Overrides the authorize URL derived from the request.
    pub authorize_starts: Mutex<VecDeque<AuthorizationStart>>,
    pub code_exchanges: Mutex<VecDeque<Result<OAuthCredential, &'static str>>>,
    pub device_authorizations: Mutex<VecDeque<DeviceAuthorization>>,
    pub device_polls: Mutex<VecDeque<DevicePoll>>,
    pub cookie_exchanges: Mutex<VecDeque<Result<AcquiredCredential, &'static str>>>,
    /// What each login step was given, in order.
    pub login_calls: Arc<Log>,
}

impl BaseChannel for TestChannel {
    fn id(&self) -> &'static str {
        "test"
    }
    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
    }
    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }
    fn oauth_authorization_code(&self) -> Option<&dyn OAuthAuthorizationCode> {
        Some(self)
    }
    fn oauth_device_code(&self) -> Option<&dyn OAuthDeviceCode> {
        Some(self)
    }
    fn cookie_login(&self) -> Option<&dyn CookieLogin> {
        Some(self)
    }
    /// `{"dialects": ["claude", …]}` on the provider, or everything.
    fn native_dialects(&self, provider: ProviderView<'_>, _: Operation) -> Vec<Dialect> {
        let configured: Vec<Dialect> = provider
            .config
            .get("dialects")
            .cloned()
            .map(|value| serde_json::from_value(value).unwrap())
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
            None => format!(
                "{}{}",
                ctx.provider.base_url.unwrap_or_default(),
                ctx.request.path
            ),
        };
        let mut builder = http::Request::builder()
            .method(ctx.request.method)
            .uri(url)
            .header(
                "authorization",
                format!("Bearer {}", bearer(ctx.credential)?),
            );
        for (name, value) in &ctx.request.headers {
            builder = builder.header(name, value);
        }
        builder
            .body(ctx.request.body)
            .map_err(|_| ChannelError::InvalidCredential)
    }
}

/// An API key or the access token a login produced; a credential with neither
/// cannot be used against this upstream.
fn bearer<'a>(credential: CredentialView<'a>) -> Result<&'a str, ChannelError> {
    ["api_key", "access_token"]
        .iter()
        .find_map(|field| credential.secret.get(*field).and_then(Value::as_str))
        .ok_or(ChannelError::InvalidCredential)
}

impl OAuthAuthorizationCode for TestChannel {
    fn authorize<'a>(
        &'a self,
        context: LoginContext<'a>,
        request: AuthorizationRequest<'a>,
    ) -> OperationFuture<'a, AuthorizationStart> {
        self.login_calls.push(format!(
            "authorize {} state={} challenge={}",
            context.provider.id, request.state, request.code_challenge
        ));
        let start = self
            .authorize_starts
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| AuthorizationStart {
                authorize_url: format!(
                    "https://login.example/authorize?state={}&code_challenge={}",
                    request.state, request.code_challenge
                ),
                redirect_uri: request.redirect_uri.to_owned(),
            });
        Box::pin(async move { Ok(start) })
    }

    fn exchange<'a>(
        &'a self,
        _: LoginContext<'a>,
        grant: AuthorizationCode<'a>,
    ) -> OperationFuture<'a, OAuthCredential> {
        self.login_calls.push(format!(
            "exchange code={} verifier={} state={}",
            grant.code, grant.code_verifier, grant.state
        ));
        let reply = self
            .code_exchanges
            .lock()
            .unwrap()
            .pop_front()
            .expect("scripted authorization code exchange");
        Box::pin(async move { reply.map_err(|reason| ChannelError::InvalidConfig(reason.into())) })
    }
}

impl OAuthDeviceCode for TestChannel {
    fn start<'a>(&'a self, context: LoginContext<'a>) -> OperationFuture<'a, DeviceAuthorization> {
        self.login_calls
            .push(format!("device-start {}", context.provider.id));
        let authorization = self
            .device_authorizations
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| device_authorization("dev-1", "USER-CODE"));
        Box::pin(async move { Ok(authorization) })
    }

    fn poll<'a>(
        &'a self,
        _: LoginContext<'a>,
        authorization: &'a DeviceAuthorization,
    ) -> OperationFuture<'a, DevicePoll> {
        self.login_calls
            .push(format!("device-poll {}", authorization.device_code));
        let reply = self
            .device_polls
            .lock()
            .unwrap()
            .pop_front()
            .expect("scripted device poll");
        Box::pin(async move { Ok(reply) })
    }
}

impl CookieLogin for TestChannel {
    fn exchange_cookie<'a>(
        &'a self,
        _: LoginContext<'a>,
        cookie: &'a str,
    ) -> OperationFuture<'a, AcquiredCredential> {
        self.login_calls.push(format!("cookie {cookie}"));
        let reply = self
            .cookie_exchanges
            .lock()
            .unwrap()
            .pop_front()
            .expect("scripted cookie exchange");
        Box::pin(async move { reply.map_err(|reason| ChannelError::InvalidConfig(reason.into())) })
    }
}

impl CredentialRefresh for TestChannel {
    fn refresh<'a>(
        &'a self,
        context: gproxy_channel::channel::RefreshContext<'a>,
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
                    secret: json!({ "api_key": api_key }),
                    expires_at_ms,
                }),
                RefreshReply::Rejected(reason) => Err(ChannelError::RefreshRejected(reason.into())),
                RefreshReply::Failed => Err(ChannelError::InvalidResponse("upstream down".into())),
            }
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

pub fn oauth_credential(access_token: &str) -> OAuthCredential {
    OAuthCredential {
        access_token: access_token.to_owned(),
        refresh_token: Some(format!("refresh-{access_token}")),
        id_token: None,
        token_type: Some("Bearer".into()),
        scopes: vec!["all".into()],
        expires_at_ms: None,
        refresh_expires_at_ms: None,
        provider_fields: Default::default(),
    }
}

pub fn device_authorization(device_code: &str, user_code: &str) -> DeviceAuthorization {
    DeviceAuthorization {
        device_code: device_code.to_owned(),
        user_code: user_code.to_owned(),
        verification_uri: "https://login.example/device".into(),
        verification_uri_complete: None,
        expires_at_ms: None,
        interval_secs: 5,
        provider_state: Default::default(),
    }
}

pub type Reply = (StatusCode, Vec<(&'static str, &'static str)>, Vec<Bytes>);

pub enum WsReply {
    Rejected(StatusCode, &'static str),
    Connected(Vec<WsFrame>),
}

/// Answers each request with the next scripted reply and records what it was
/// asked for. Running out of replies is a test bug, not an upstream failure.
#[derive(Default)]
pub struct ScriptClient {
    pub replies: Mutex<VecDeque<Reply>>,
    pub ws_replies: Mutex<VecDeque<WsReply>>,
    pub sent_frames: Arc<Mutex<Vec<WsFrame>>>,
    pub seen: Arc<Log>,
}

impl ScriptClient {
    pub fn script(&self, replies: Vec<Reply>) {
        *self.replies.lock().unwrap() = replies.into();
    }
    pub fn script_ws(&self, replies: Vec<WsReply>) {
        *self.ws_replies.lock().unwrap() = replies.into();
    }
}

impl OutboundClient for ScriptClient {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let body = read(body).await;
            self.seen.push(format!(
                "{} {} auth={} body={body}",
                parts.method,
                parts.uri,
                parts
                    .headers
                    .get("authorization")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or_default(),
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
            self.seen.push(format!("WS {}", request.uri()));
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

/// A handle on a private in-memory database, with the scripted channel and
/// client and no synchronization of its own: the test decides when it reloads.
pub async fn sdk() -> Gproxy<DatabaseConnection> {
    sdk_parts().await.0
}

pub async fn sdk_parts() -> (
    Gproxy<DatabaseConnection>,
    Arc<TestChannel>,
    Arc<ScriptClient>,
) {
    let channel = Arc::new(TestChannel::default());
    let client = Arc::new(ScriptClient::default());
    let gproxy = GproxyBuilder::sqlite_memory()
        .await
        .unwrap()
        .plaintext_secrets()
        .without_default_channels()
        .channel(channel.clone())
        .client_pool(ClientPool::with_client(client.clone()))
        .sync_mode(SyncMode::Manual)
        .build()
        .await
        .unwrap();
    (gproxy, channel, client)
}

/// A handle on a database file, with a cache the caller chooses: two of these
/// over one file and one shared cache are two instances of the same
/// deployment, which is how peer notification is exercised.
pub async fn sdk_on_file(path: &str, cache: Arc<dyn Cache>) -> Gproxy<DatabaseConnection> {
    GproxyBuilder::sqlite(path)
        .await
        .unwrap()
        .cache(cache)
        .plaintext_secrets()
        .without_default_channels()
        .channel(Arc::new(TestChannel::default()))
        .client_pool(ClientPool::with_client(Arc::new(ScriptClient::default())))
        .sync_mode(SyncMode::Manual)
        .build()
        .await
        .unwrap()
}

pub fn request(body: &str) -> WireRequest<HttpBody> {
    WireRequest {
        method: Method::POST,
        path: "/v1/responses".into(),
        query: None,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(body.to_owned())),
    }
}

pub fn json_reply(status: StatusCode, body: Value) -> Reply {
    (
        status,
        vec![("content-type", "application/json")],
        vec![Bytes::from(serde_json::to_vec(&body).unwrap())],
    )
}

pub async fn read(body: HttpBody) -> String {
    match body {
        HttpBody::Bytes(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                out.extend_from_slice(&chunk.unwrap());
            }
            String::from_utf8_lossy(&out).into_owned()
        }
    }
}
