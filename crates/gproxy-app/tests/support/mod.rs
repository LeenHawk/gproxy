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
    connection::{Bytes, HeaderMap, HeaderValue},
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
    fn quota_model(&self) -> Option<&dyn gproxy_channel::channel::QuotaModel> {
        Some(self)
    }

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
}

/// Answers each request from a queue and records the URL it was given, so a
/// test can assert which provider was reached — and, when the queue is
/// untouched, that nothing was sent at all.
#[derive(Default)]
pub struct ScriptClient {
    pub authorizations: Mutex<Vec<String>>,
    replies: Mutex<VecDeque<Reply>>,
    seen: Mutex<Vec<String>>,
}

impl ScriptClient {
    pub fn script(&self, replies: Vec<Reply>) {
        *self.replies.lock().unwrap() = replies.into();
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
            self.authorizations.lock().unwrap().push(
                request
                    .headers()
                    .get("authorization")
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
            );
            self.seen.lock().unwrap().push(request.uri().to_string());
            let Reply::Http(status, body) = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("scripted reply");
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
        _: http::Request<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(async move {
            Err(CapabilityError::new(
                CapabilityErrorKind::Unsupported,
                CapabilityErrorStage::Start,
                "no websocket here",
            ))
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

impl gproxy_channel::channel::QuotaModel for TestChannel {
    fn dimensions(
        &self,
        _: ProviderView<'_>,
        credential: gproxy_channel::channel::CredentialView<'_>,
    ) -> Vec<gproxy_channel::channel::QuotaDimension> {
        use gproxy_channel::channel::{
            QuotaDimension, QuotaMetric, QuotaScope, QuotaTracking, QuotaWindow,
        };
        if credential.metadata.get("observed_quota") != Some(&json!(true)) {
            return Vec::new();
        }
        vec![QuotaDimension {
            id: "5h".into(),
            label: None,
            scope: QuotaScope::All,
            operations: None,
            metric: QuotaMetric::Requests,
            window: QuotaWindow::Rolling { seconds: 18000 },
            limit: None,
            tracking: QuotaTracking::Reported,
            blocking: true,
        }]
    }
}
