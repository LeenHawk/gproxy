//! A fake upstream on a local port, an in-memory store, and a channel that
//! signs in with OAuth against the fake.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use gproxy_channel::{BaseChannel, channel::PrepareContext};
use gproxy_client::{ClientPool, ConnectionConfig, OutboundClient};
use gproxy_lite::{Engine, KeyValue, MemoryState};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, WireResponse,
    capability::CapabilityFuture,
    codec::{CodecLimits, read_http_body},
};
use gproxy_upstream::{
    AuthorizationCode, AuthorizationRequest, AuthorizationStart, CredentialRefresh,
    CredentialUpdate, LoginContext, OAuthAuthorizationCode, OAuthCredential, ProviderView,
    RefreshContext, UpstreamError, UpstreamFuture, UpstreamKind,
};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

pub const ACCESS_KEY: &str = "lite-access-key";

const LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: 1 << 20,
    max_value_bytes: 1 << 20,
    max_body_bytes: 1 << 20,
    max_line_bytes: 1 << 20,
    max_part_bytes: 1 << 20,
    max_parts: 16,
};

/// One request the fake upstream received.
#[derive(Clone, Debug)]
pub struct Seen {
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

impl Seen {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// What the fake answers: a status and a body, `text/event-stream` when the
/// body is a list of events.
pub enum Answer {
    Json(u16, Value),
    Events(Vec<String>),
}

type Respond = dyn Fn(&Seen) -> Answer + Send + Sync;

pub struct Upstream {
    pub url: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Upstream {
    pub async fn start(respond: impl Fn(&Seen) -> Answer + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let respond: Arc<Respond> = Arc::new(respond);
        let log = seen.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(connection(stream, log.clone(), respond.clone()));
            }
        });
        Self { url, seen }
    }

    pub fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

async fn connection(mut stream: TcpStream, log: Arc<Mutex<Vec<Seen>>>, respond: Arc<Respond>) {
    let mut buffer = Vec::new();
    let head_end = loop {
        let mut chunk = [0u8; 4096];
        let read = stream.read(&mut chunk).await.unwrap_or(0);
        if read == 0 {
            return;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(at) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let mut lines = head.lines();
    let mut request_line = lines.next().unwrap_or_default().split(' ');
    request_line.next();
    let path = request_line.next().unwrap_or_default().to_owned();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    let length: usize = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0);
    while buffer.len() < head_end + length {
        let mut chunk = [0u8; 4096];
        let read = stream.read(&mut chunk).await.unwrap_or(0);
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let raw = &buffer[head_end..];
    let body = serde_json::from_slice(raw)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(raw).into_owned()));
    let seen = Seen {
        path,
        headers,
        body,
    };
    log.lock().unwrap().push(seen.clone());
    match respond(&seen) {
        Answer::Json(status, body) => {
            let body = body.to_string();
            let head = format!(
                "HTTP/1.1 {status} OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes()).await;
        }
        Answer::Events(events) => {
            let _ = stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n",
                )
                .await;
            for event in events {
                let _ = stream.write_all(event.as_bytes()).await;
                let _ = stream.flush().await;
                // Separate writes, so the client sees a stream, not one body.
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        }
    }
    let _ = stream.shutdown().await;
}

pub async fn client() -> Arc<dyn OutboundClient> {
    let pool = ClientPool::new(4, Duration::from_secs(30)).unwrap();
    pool.get(&ConnectionConfig::default()).await.unwrap()
}

pub fn channels() -> Vec<Arc<dyn BaseChannel>> {
    let mut channels = gproxy_channel::channels::compiled_in();
    channels.push(Arc::new(FakeOAuth));
    channels
}

pub async fn engine(config: Value, state: Arc<dyn KeyValue>) -> Engine {
    Engine::new(&config.to_string(), &channels(), state, client().await)
        .unwrap_or_else(|error| panic!("{error}"))
}

pub fn memory() -> Arc<dyn KeyValue> {
    Arc::new(MemoryState::default())
}

/// A request carrying the access key.
pub fn request(method: &str, path: &str, body: Option<Value>) -> http::Request<HttpBody> {
    let body = body.map_or_else(Default::default, |body| body.to_string().into());
    http::Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {ACCESS_KEY}"))
        .header("content-type", "application/json")
        .body(HttpBody::Bytes(body))
        .unwrap()
}

pub async fn text(response: WireResponse<HttpBody>) -> (u16, String) {
    let status = response.status.as_u16();
    let bytes = read_http_body(response.body, LIMITS).await.unwrap();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

pub async fn json(response: WireResponse<HttpBody>) -> (u16, Value) {
    let (status, text) = text(response).await;
    let value = serde_json::from_str(&text).unwrap_or_else(|_| panic!("not JSON: {text}"));
    (status, value)
}

/// A channel that signs in with an authorization code and renews its token,
/// both against `{base_url}/oauth/token`, and sends Chat Completions with the
/// access token as the bearer.
pub struct FakeOAuth;

async fn token(
    client: &dyn OutboundClient,
    provider: ProviderView<'_>,
    grant: Value,
) -> Result<(String, i64), UpstreamError> {
    let request = http::Request::builder()
        .method("POST")
        .uri(format!("{}/oauth/token", provider.base_url.unwrap()))
        .header("content-type", "application/json")
        .body(HttpBody::Bytes(grant.to_string().into()))
        .unwrap();
    let response = client.send(request).await?;
    let status = response.status;
    let body = read_http_body(response.body, LIMITS).await.unwrap();
    if !status.is_success() {
        return Err(UpstreamError::RefreshRejected(
            String::from_utf8_lossy(&body).into_owned(),
        ));
    }
    let body: Value = serde_json::from_slice(&body).unwrap();
    Ok((
        body["access_token"].as_str().unwrap().to_owned(),
        body["expires_at_ms"].as_i64().unwrap(),
    ))
}

impl UpstreamKind for FakeOAuth {
    fn id(&self) -> &'static str {
        "fake_oauth"
    }
    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
    }
    fn oauth_authorization_code(&self) -> Option<&dyn OAuthAuthorizationCode> {
        Some(self)
    }
}

impl OAuthAuthorizationCode for FakeOAuth {
    fn authorize<'a>(
        &'a self,
        _context: LoginContext<'a>,
        request: AuthorizationRequest<'a>,
    ) -> UpstreamFuture<'a, AuthorizationStart> {
        Box::pin(async move {
            Ok(AuthorizationStart {
                authorize_url: format!(
                    "https://auth.invalid/authorize?state={}&code_challenge={}",
                    request.state, request.code_challenge
                ),
                redirect_uri: "http://localhost:1455/callback".into(),
                provider_state: Default::default(),
            })
        })
    }

    fn exchange<'a>(
        &'a self,
        context: LoginContext<'a>,
        grant: AuthorizationCode<'a>,
    ) -> UpstreamFuture<'a, OAuthCredential> {
        Box::pin(async move {
            let (access_token, expires_at_ms) = token(
                context.client,
                context.provider,
                json!({
                    "grant_type": "authorization_code",
                    "code": grant.code,
                    "code_verifier": grant.code_verifier,
                    "redirect_uri": grant.redirect_uri,
                }),
            )
            .await?;
            Ok(OAuthCredential {
                access_token,
                refresh_token: Some("refresh-1".into()),
                id_token: None,
                token_type: None,
                scopes: Vec::new(),
                expires_at_ms: Some(expires_at_ms),
                refresh_expires_at_ms: None,
                provider_fields: Default::default(),
                provider_secrets: Default::default(),
            })
        })
    }
}

impl CredentialRefresh for FakeOAuth {
    fn refresh<'a>(
        &'a self,
        context: RefreshContext<'a>,
    ) -> CapabilityFuture<'a, Result<CredentialUpdate, UpstreamError>> {
        Box::pin(async move {
            let (access_token, expires_at_ms) = token(
                context.client,
                context.provider,
                json!({
                    "grant_type": "refresh_token",
                    "refresh_token": context.credential.secret["refresh_token"],
                }),
            )
            .await?;
            let mut secret = context.credential.secret.clone();
            secret["access_token"] = access_token.into();
            secret["expires_at_ms"] = expires_at_ms.into();
            Ok(CredentialUpdate {
                secret,
                metadata: None,
                expires_at_ms: Some(expires_at_ms),
            })
        })
    }
}

impl BaseChannel for FakeOAuth {
    fn native_dialects(&self, _: ProviderView<'_>, _: Operation) -> Vec<Dialect> {
        vec![Dialect::OpenAiChat]
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, UpstreamError> {
        let token = ctx.credential.secret["access_token"]
            .as_str()
            .ok_or(UpstreamError::InvalidCredential)?;
        http::Request::builder()
            .method(ctx.request.method)
            .uri(format!(
                "{}{}",
                ctx.provider.base_url.unwrap(),
                ctx.request.path
            ))
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(ctx.request.body)
            .map_err(|error| UpstreamError::InvalidConfig(error.to_string()))
    }
}
