use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use futures_util::{StreamExt, stream};
use gproxy_channel::channel::{
    AcquiredCredential, CookieLogin, CredentialRefresh, CredentialUpdate, CredentialView,
    LoginContext, OperationFuture, PrepareContext, ProviderView, RefreshContext,
};
use gproxy_channel::{
    BaseChannel, ChannelBinding, ChannelCapabilities, ChannelError, LoginMode, OutboundClient,
};
// Only the descriptor tests read it, and each of those is behind its own
// channel feature, so importing it unconditionally is an unused import in a
// build with no channel compiled in.
#[cfg(any(
    feature = "claudecode",
    feature = "claudeweb",
    feature = "codex",
    feature = "custom"
))]
use gproxy_channel::ConfigKeyKind;
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
        metadata: &serde_json::Value::Null,
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
    let a = Arc::new(RecordingClient::default());
    let b = Arc::new(RecordingClient::default());
    assert!(Minimal.credential_refresh().is_none());
    let binding_a = ChannelBinding::new(&Minimal, provider(&config), credential(&first), a.clone());
    let binding_b =
        ChannelBinding::new(&Minimal, provider(&config), credential(&second), b.clone());
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
    let client = Arc::new(RecordingClient::default());
    let binding = ChannelBinding::new(
        &Minimal,
        provider(&config),
        credential(&invalid_secret),
        client.clone(),
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
    let client = Arc::new(RecordingClient::default());
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
    let binding = ChannelBinding::new(
        &Minimal,
        provider(&config),
        credential(&secret),
        client.clone(),
    );
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
    let client = Arc::new(RecordingClient::default());
    let channel: &dyn BaseChannel = &Refreshable;
    let update = channel
        .credential_refresh()
        .unwrap()
        .refresh(RefreshContext {
            provider: provider(&config),
            credential: credential(&secret),
            client: client.as_ref(),
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
        Arc::new(FailingClient),
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

/// A cookie channel that can also renew itself: two capabilities the default
/// descriptor has to find on its own.
struct Cookied;
impl BaseChannel for Cookied {
    fn id(&self) -> &'static str {
        "cookied"
    }
    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(&Refreshable)
    }
    fn cookie_login(&self) -> Option<&dyn CookieLogin> {
        Some(self)
    }
}
impl CookieLogin for Cookied {
    fn exchange_cookie<'a>(
        &'a self,
        _: LoginContext<'a>,
        _: &'a str,
    ) -> OperationFuture<'a, AcquiredCredential> {
        Box::pin(async {
            Ok(AcquiredCredential {
                secret: json!({"session_key": "s"}),
                expires_at_ms: None,
                metadata: Value::Null,
            })
        })
    }
}

#[test]
fn the_default_descriptor_is_derived_from_the_capability_accessors() {
    let minimal = Minimal.descriptor();
    assert_eq!(minimal.id, "minimal");
    assert_eq!(
        minimal.display_name, "minimal",
        "a channel that says nothing is named after its id"
    );
    assert!(minimal.login_modes.is_empty());
    assert_eq!(minimal.capabilities, ChannelCapabilities::default());
    assert!(minimal.config_keys.is_empty());

    let refreshable = Refreshable.descriptor();
    assert!(refreshable.capabilities.refresh);
    assert!(!refreshable.capabilities.quota_query);
    assert!(
        refreshable.login_modes.is_empty(),
        "refreshing is not a way to log in"
    );

    let cookied = Cookied.descriptor();
    assert_eq!(cookied.login_modes, [LoginMode::Cookie]);
    assert!(cookied.capabilities.refresh);
    assert!(!cookied.capabilities.services);
    assert!(
        !cookied.capabilities.websocket,
        "no accessor describes transport"
    );
}

/// Each real channel names itself, declares its login flows and lists the keys
/// it decodes out of the provider row. Spot-checked, not enumerated: the point
/// is that the descriptor is written from the channel's own `*Config`.
#[cfg(feature = "custom")]
#[test]
fn custom_describes_an_api_key_upstream() {
    let descriptor = gproxy_channel::channels::custom::Custom.descriptor();
    assert_eq!(descriptor.id, "custom");
    assert_eq!(descriptor.login_modes, [LoginMode::ApiKey]);
    assert_eq!(descriptor.capabilities, ChannelCapabilities::default());
    let base_url = descriptor.config_key("base_url").unwrap();
    assert!(base_url.required, "custom cannot build a URL without it");
    assert_eq!(base_url.kind, ConfigKeyKind::String);
    assert_eq!(
        descriptor.config_key("dialects").unwrap().kind,
        ConfigKeyKind::Json
    );
    assert!(
        !descriptor
            .config_key("enable_claude_magic_cache")
            .unwrap()
            .required
    );
    // The host's own keys ride along on every channel.
    assert!(descriptor.config_key("credential_strategy").is_some());
    assert_eq!(
        descriptor.config_key("allowed_headers").unwrap().kind,
        ConfigKeyKind::HeaderList
    );
}

#[cfg(feature = "codex")]
#[test]
fn codex_describes_both_oauth_flows_and_its_websocket() {
    let descriptor = gproxy_channel::channels::codex::Codex.descriptor();
    assert_eq!(descriptor.id, "codex");
    assert_eq!(
        descriptor.login_modes,
        [LoginMode::AuthorizationCode, LoginMode::DeviceCode]
    );
    assert_eq!(
        descriptor.capabilities,
        ChannelCapabilities {
            refresh: true,
            quota_query: true,
            // Reset credits are queried and redeemed (`codex/quota.rs`).
            quota_reset: true,
            services: true,
            websocket: true,
        }
    );
    assert!(
        gproxy_channel::channels::codex::Codex
            .quota_reset()
            .is_some()
    );
    assert!(!descriptor.config_key("base_url").unwrap().required);
    assert_eq!(
        descriptor.config_key("issuer").unwrap().kind,
        ConfigKeyKind::String
    );
    assert_eq!(
        descriptor
            .config_key("synthesize_cli_identity")
            .unwrap()
            .kind,
        ConfigKeyKind::Bool
    );
}

#[cfg(feature = "claudecode")]
#[test]
fn claudecode_describes_pkce_and_cookie_login() {
    let descriptor = gproxy_channel::channels::claudecode::Claudecode.descriptor();
    assert_eq!(descriptor.id, "claudecode");
    assert_eq!(
        descriptor.login_modes,
        [LoginMode::AuthorizationCode, LoginMode::Cookie]
    );
    assert!(descriptor.capabilities.services);
    assert!(!descriptor.capabilities.websocket);
    assert_eq!(
        descriptor.config_key("claude_ai_url").unwrap().kind,
        ConfigKeyKind::String
    );
    assert_eq!(
        descriptor.config_key("headers").unwrap().kind,
        ConfigKeyKind::HeaderList
    );
}

#[cfg(feature = "claudeweb")]
#[test]
fn claudeweb_describes_a_cookie_only_session() {
    let descriptor = gproxy_channel::channels::claudeweb::ClaudeWeb::default().descriptor();
    assert_eq!(descriptor.id, "claudeweb");
    assert_eq!(descriptor.login_modes, [LoginMode::Cookie]);
    assert!(descriptor.capabilities.quota_query);
    assert!(
        !descriptor.capabilities.services,
        "claude.ai has no CLI service surface here"
    );
    assert_eq!(
        descriptor.config_key("timezone").unwrap().kind,
        ConfigKeyKind::String
    );
    assert_eq!(
        descriptor.config_key("endpoints").unwrap().kind,
        ConfigKeyKind::Json
    );
}
