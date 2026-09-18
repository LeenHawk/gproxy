use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

use futures_util::{StreamExt, stream};
use gproxy_channel::channel::{
    CredentialRefresh, CredentialUpdate, CredentialView, PrepareContext, ProviderView,
    RefreshContext,
};
use gproxy_channel::{BaseChannel, ChannelBinding, ChannelError, OutboundClient};
use gproxy_protocol::capability::{
    CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture,
};
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse};
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

const KEY: OperationKey = OperationKey {
    operation: Operation::GenerateContent,
    dialect: Dialect::OpenAiChat,
};

struct Minimal;

// A pass-through channel only needs identity and common request preparation.
impl BaseChannel for Minimal {
    fn id(&self) -> &'static str {
        "minimal"
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let key = ctx.credential.secret["api_key"]
            .as_str()
            .ok_or(ChannelError::InvalidCredential)?;
        http::Request::builder()
            .method(ctx.request.method)
            .uri(ctx.provider.base_url.unwrap())
            .header("authorization", format!("Bearer {key}"))
            .body(ctx.request.body)
            .map_err(|_| ChannelError::InvalidCredential)
    }
}

#[derive(Default)]
struct RecordingClient {
    calls: AtomicUsize,
    auth: Mutex<Vec<String>>,
}

impl OutboundClient for RecordingClient {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse, CapabilityError>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.auth.lock().unwrap().push(
                request.headers()["authorization"]
                    .to_str()
                    .unwrap()
                    .to_owned(),
            );
            Ok(WireResponse {
                status: StatusCode::TOO_MANY_REQUESTS,
                headers: HeaderMap::new(),
                body: request.into_body(),
            })
        })
    }
}

fn provider(config: &Value) -> ProviderView<'_> {
    ProviderView {
        id: "p1",
        channel: "minimal",
        base_url: Some("https://example.com/v1/chat/completions"),
        config,
    }
}

fn credential(secret: &Value) -> CredentialView<'_> {
    CredentialView {
        id: "c1",
        provider_id: "p1",
        auth_kind: "api_key",
        secret,
        version: 0,
        expires_at_ms: None,
    }
}

fn request(body: HttpBody) -> WireRequest {
    WireRequest {
        method: Method::POST,
        path: "/v1/chat/completions".into(),
        query: None,
        headers: HeaderMap::new(),
        body,
    }
}

#[tokio::test]
async fn explicit_bindings_keep_credentials_and_clients_separate() {
    let config = json!({});
    let first = json!({"api_key": "first"});
    let second = json!({"api_key": "second"});
    let a = RecordingClient::default();
    let b = RecordingClient::default();
    assert!(Minimal.credential_refresh().is_none());
    let binding_a = ChannelBinding::new(&Minimal, provider(&config), credential(&first), &a);
    let binding_b = ChannelBinding::new(&Minimal, provider(&config), credential(&second), &b);
    for binding in [binding_a, binding_b] {
        let response = binding
            .send(KEY, request(HttpBody::Bytes(Bytes::from_static(b"body"))))
            .await
            .unwrap();
        assert_eq!(response.status, StatusCode::TOO_MANY_REQUESTS);
        let HttpBody::Bytes(body) = response.body else {
            panic!("expected bytes")
        };
        assert_eq!(body, "body");
    }
    assert_eq!(*a.auth.lock().unwrap(), ["Bearer first"]);
    assert_eq!(*b.auth.lock().unwrap(), ["Bearer second"]);
}

#[tokio::test]
async fn invalid_credentials_do_not_reach_client() {
    let config = json!({});
    let invalid_secret = json!({});
    let client = RecordingClient::default();
    let binding = ChannelBinding::new(
        &Minimal,
        provider(&config),
        credential(&invalid_secret),
        &client,
    );
    assert!(matches!(
        binding
            .send(KEY, request(HttpBody::Bytes(Bytes::new())))
            .await,
        Err(ChannelError::InvalidCredential)
    ));
    assert_eq!(client.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn body_stays_lazy_and_preserves_late_transfer_failure() {
    let config = json!({});
    let secret = json!({"api_key": "key"});
    let client = RecordingClient::default();
    let polls = std::sync::Arc::new(AtomicUsize::new(0));
    let observed = polls.clone();
    let body = HttpBody::Stream(Box::pin(
        stream::iter([
            Ok(Bytes::from_static(b"first")),
            Err(std::io::Error::other("disconnected").into()),
        ])
        .inspect(move |_| {
            observed.fetch_add(1, Ordering::SeqCst);
        }),
    ));
    let binding = ChannelBinding::new(&Minimal, provider(&config), credential(&secret), &client);
    let response = binding.send(KEY, request(body)).await.unwrap();
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    let HttpBody::Stream(mut body) = response.body else {
        panic!("stream buffered")
    };
    assert_eq!(body.next().await.unwrap().unwrap(), "first");
    assert_eq!(
        body.next().await.unwrap().unwrap_err().to_string(),
        "disconnected"
    );
    assert!(body.next().await.is_none());
    assert_eq!(client.calls.load(Ordering::SeqCst), 1);
}

struct Refreshable;
impl BaseChannel for Refreshable {
    fn id(&self) -> &'static str {
        Minimal.id()
    }
    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        Minimal.prepare(ctx)
    }
    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
    }
}
impl CredentialRefresh for Refreshable {
    fn refresh<'a>(
        &'a self,
        ctx: RefreshContext<'a>,
    ) -> CapabilityFuture<'a, Result<CredentialUpdate, ChannelError>> {
        Box::pin(async move {
            assert_eq!(ctx.credential.version, 0);
            Ok(CredentialUpdate {
                secret: json!({"api_key": "rotated"}),
                expires_at_ms: Some(1234),
            })
        })
    }
}

#[tokio::test]
async fn optional_trait_is_available_through_base_trait_object() {
    let config = json!({});
    let secret = json!({"api_key": "key"});
    let client = RecordingClient::default();
    let channel: &dyn BaseChannel = &Refreshable;
    let update = channel
        .credential_refresh()
        .unwrap()
        .refresh(RefreshContext {
            provider: provider(&config),
            credential: credential(&secret),
            client: &client,
        })
        .await
        .unwrap();
    assert_eq!(update.secret["api_key"], "rotated");
    assert_eq!(update.expires_at_ms, Some(1234));
    assert_eq!(secret["api_key"], "key");
}

struct FailingClient;
impl OutboundClient for FailingClient {
    fn send<'a>(
        &'a self,
        _: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse, CapabilityError>> {
        Box::pin(async {
            Err(CapabilityError::new(
                CapabilityErrorKind::Transport,
                CapabilityErrorStage::Start,
                "connection failed",
            ))
        })
    }
}

#[tokio::test]
async fn transport_failure_remains_distinct_from_http_error_response() {
    let config = json!({});
    let secret = json!({"api_key": "key"});
    let binding = ChannelBinding::new(
        &Minimal,
        provider(&config),
        credential(&secret),
        &FailingClient,
    );
    let Err(ChannelError::Transport(error)) = binding
        .send(KEY, request(HttpBody::Bytes(Bytes::new())))
        .await
    else {
        panic!("expected transport error")
    };
    assert_eq!(error.kind(), CapabilityErrorKind::Transport);
    assert_eq!(error.stage(), CapabilityErrorStage::Start);
}
