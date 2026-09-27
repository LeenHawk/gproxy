use std::{
    collections::{BTreeMap, VecDeque},
    sync::Mutex,
};

mod support;

use futures_util::{StreamExt, stream};
use gproxy_channel::channel::*;
use gproxy_channel::{BaseChannel, ChannelError, OutboundClient};
use gproxy_protocol::capability::{CapabilityError, CapabilityFuture};
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse};
use http::{HeaderMap, Method, StatusCode};
use rust_decimal::Decimal;
use serde_json::{Value, json};

const KEY: OperationKey = OperationKey {
    operation: Operation::StreamGenerateContent,
    dialect: Dialect::OpenAi,
};

struct ScriptClient {
    replies: Mutex<VecDeque<WireResponse>>,
    requests: Mutex<Vec<http::Request<HttpBody>>>,
}
impl ScriptClient {
    fn new(replies: Vec<WireResponse>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
        }
    }
}
impl OutboundClient for ScriptClient {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse, CapabilityError>> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            Ok(self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected call"))
        })
    }
}
fn reply(status: StatusCode, value: Value) -> WireResponse {
    WireResponse {
        status,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&value).unwrap())),
    }
}
fn payload(response: WireResponse) -> Value {
    let HttpBody::Bytes(bytes) = response.body else {
        panic!("expected test JSON")
    };
    serde_json::from_slice(&bytes).unwrap()
}
fn provider(config: &Value) -> ProviderView<'_> {
    ProviderView {
        id: "selected-provider",
        channel: "demo",
        base_url: Some("https://selected.invalid"),
        config,
    }
}
fn credential(secret: &Value) -> CredentialView<'_> {
    CredentialView {
        id: "selected-credential",
        provider_id: "selected-provider",
        auth_kind: "oauth",
        secret,
        metadata: &serde_json::Value::Null,
        version: 7,
        expires_at_ms: None,
    }
}
fn token_json() -> Value {
    json!({"access_token":"test-access", "refresh_token":"test-refresh", "id_token":null,
        "token_type":"Bearer", "scopes":["read"], "expires_at_ms":5000,
        "refresh_expires_at_ms":null, "provider_fields":{"account_id":"test-account"}})
}
async fn login_call(
    ctx: LoginContext<'_>,
    path: &str,
    body: Value,
) -> Result<WireResponse, ChannelError> {
    ctx.client
        .send(
            http::Request::post(format!("{}{path}", ctx.provider.base_url.unwrap()))
                .body(HttpBody::Bytes(Bytes::from(
                    serde_json::to_vec(&body).unwrap(),
                )))
                .unwrap(),
        )
        .await
        .map_err(ChannelError::from)
}
async fn account_call(
    ctx: CredentialContext<'_>,
    path: &str,
    body: Value,
) -> Result<WireResponse, ChannelError> {
    ctx.client
        .send(
            http::Request::post(format!("{}{path}", ctx.provider.base_url.unwrap()))
                .header(
                    "authorization",
                    format!(
                        "Bearer {}",
                        ctx.credential.secret["access_token"].as_str().unwrap()
                    ),
                )
                .body(HttpBody::Bytes(Bytes::from(
                    serde_json::to_vec(&body).unwrap(),
                )))
                .unwrap(),
        )
        .await
        .map_err(ChannelError::from)
}

struct Bare;
impl BaseChannel for Bare {
    fn id(&self) -> &'static str {
        "bare"
    }
}
struct Demo;
impl BaseChannel for Demo {
    fn id(&self) -> &'static str {
        "demo"
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
    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }
    fn quota_reset(&self) -> Option<&dyn QuotaReset> {
        Some(self)
    }
    fn quota_headers(&self) -> Option<&dyn QuotaHeaders> {
        Some(self)
    }
    fn usage_extras(&self) -> Option<&dyn UsageExtras> {
        Some(self)
    }
    fn services(&self) -> Option<&dyn ChannelServices> {
        Some(self)
    }
}

#[test]
fn existing_channels_need_no_optional_implementations() {
    let channel: &dyn BaseChannel = &Bare;
    assert!(channel.oauth_authorization_code().is_none());
    assert!(channel.oauth_device_code().is_none());
    assert!(channel.cookie_login().is_none());
    assert!(channel.credential_refresh().is_none());
    assert!(channel.quota_query().is_none());
    assert!(channel.quota_reset().is_none());
    assert!(channel.quota_headers().is_none());
    assert!(channel.usage_extras().is_none());
    assert!(channel.services().is_none());
}

impl OAuthAuthorizationCode for Demo {
    fn authorize<'a>(
        &'a self,
        _: LoginContext<'a>,
        req: AuthorizationRequest<'a>,
    ) -> OperationFuture<'a, AuthorizationStart> {
        Box::pin(async move {
            Ok(AuthorizationStart {
                authorize_url: format!(
                    "https://login.invalid/?state={}&code_challenge={}",
                    req.state, req.code_challenge
                ),
                redirect_uri: req.redirect_uri.into(),
                // The pocket the exchange gets back, the same one a device
                // authorization carries.
                provider_state: BTreeMap::from([("registration".into(), json!("client-9"))]),
            })
        })
    }
    fn exchange<'a>(
        &'a self,
        ctx: LoginContext<'a>,
        code: AuthorizationCode<'a>,
    ) -> OperationFuture<'a, OAuthCredential> {
        Box::pin(async move {
            let response = login_call(
                ctx,
                "/token",
                json!({"code":code.code,"state":code.state,
                "redirect_uri":code.redirect_uri,"code_verifier":code.code_verifier,
                "provider_state":code.provider_state}),
            )
            .await?;
            Ok(serde_json::from_value(payload(response)).unwrap())
        })
    }
}
impl OAuthDeviceCode for Demo {
    fn start<'a>(&'a self, ctx: LoginContext<'a>) -> OperationFuture<'a, DeviceAuthorization> {
        Box::pin(async move {
            Ok(
                serde_json::from_value(payload(login_call(ctx, "/device", json!({})).await?))
                    .unwrap(),
            )
        })
    }
    fn poll<'a>(
        &'a self,
        ctx: LoginContext<'a>,
        session: &'a DeviceAuthorization,
    ) -> OperationFuture<'a, DevicePoll> {
        Box::pin(async move {
            let response = login_call(
                ctx,
                "/poll",
                json!({"device_code":session.device_code,"provider_state":session.provider_state}),
            )
            .await?;
            Ok(match response.status {
                StatusCode::ACCEPTED => DevicePoll::Pending,
                StatusCode::TOO_MANY_REQUESTS => DevicePoll::SlowDown { interval_secs: 9 },
                StatusCode::FORBIDDEN => DevicePoll::Denied,
                StatusCode::GONE => DevicePoll::Expired,
                _ => DevicePoll::Ready(serde_json::from_value(payload(response)).unwrap()),
            })
        })
    }
}
impl CookieLogin for Demo {
    fn exchange_cookie<'a>(
        &'a self,
        ctx: LoginContext<'a>,
        cookie: &'a str,
    ) -> OperationFuture<'a, AcquiredCredential> {
        Box::pin(async move {
            let credential: OAuthCredential = serde_json::from_value(payload(
                login_call(ctx, "/cookie", json!({"cookie":cookie})).await?,
            ))
            .unwrap();
            Ok(credential.into())
        })
    }
}

#[tokio::test]
async fn oauth_flows_preserve_pkce_state_tokens_and_host_driven_device_polling() {
    let config = json!({});
    let client = ScriptClient::new(vec![
        reply(StatusCode::OK, token_json()),
        reply(
            StatusCode::OK,
            json!({"device_code":"device","user_code":"USER", "verification_uri":"https://login.invalid/device",
            "verification_uri_complete":null,"expires_at_ms":null,"interval_secs":5,"provider_state":{"device_auth_id":"auth-id"}}),
        ),
        reply(StatusCode::ACCEPTED, json!({})),
        reply(StatusCode::TOO_MANY_REQUESTS, json!({})),
        reply(StatusCode::OK, token_json()),
        reply(StatusCode::FORBIDDEN, json!({})),
        reply(StatusCode::GONE, json!({})),
        reply(StatusCode::OK, token_json()),
    ]);
    let ctx = LoginContext {
        provider: provider(&config),
        client: &client,
    };
    let base: &dyn BaseChannel = &Demo;
    let code = base.oauth_authorization_code().unwrap();
    let started = code
        .authorize(
            ctx,
            AuthorizationRequest {
                redirect_uri: "http://localhost/callback",
                state: "state",
                code_challenge: "challenge",
            },
        )
        .await
        .unwrap();
    assert!(
        started
            .authorize_url
            .contains("state=state&code_challenge=challenge")
    );
    assert!(client.requests.lock().unwrap().is_empty());
    let tokens = code
        .exchange(
            ctx,
            AuthorizationCode {
                code: "code",
                state: "state",
                redirect_uri: &started.redirect_uri,
                code_verifier: "verifier",
                provider_state: &started.provider_state,
            },
        )
        .await
        .unwrap();
    assert_eq!(tokens.refresh_token.as_deref(), Some("test-refresh"));
    assert_eq!(tokens.provider_fields["account_id"], "test-account");
    let device = base.oauth_device_code().unwrap();
    let session = device.start(ctx).await.unwrap();
    let persisted = serde_json::to_vec(&session).unwrap();
    let session: DeviceAuthorization = serde_json::from_slice(&persisted).unwrap();
    assert_eq!(session.provider_state["device_auth_id"], "auth-id");
    assert!(matches!(
        device.poll(ctx, &session).await.unwrap(),
        DevicePoll::Pending
    ));
    assert_eq!(client.requests.lock().unwrap().len(), 3);
    assert!(matches!(
        device.poll(ctx, &session).await.unwrap(),
        DevicePoll::SlowDown { interval_secs: 9 }
    ));
    let DevicePoll::Ready(tokens) = device.poll(ctx, &session).await.unwrap() else {
        panic!("expected tokens")
    };
    assert_eq!(tokens.expires_at_ms, Some(5000));
    assert!(matches!(
        device.poll(ctx, &session).await.unwrap(),
        DevicePoll::Denied
    ));
    assert!(matches!(
        device.poll(ctx, &session).await.unwrap(),
        DevicePoll::Expired
    ));
    assert_eq!(
        base.cookie_login()
            .unwrap()
            .exchange_cookie(ctx, "test-cookie")
            .await
            .unwrap()
            .secret["access_token"],
        "test-access"
    );
    let requests = client.requests.lock().unwrap();
    assert_eq!(requests.len(), 8);
    assert!(
        requests
            .iter()
            .all(|r| r.uri().host() == Some("selected.invalid"))
    );
    let HttpBody::Bytes(body) = requests[0].body() else {
        panic!("expected grant")
    };
    let grant: Value = serde_json::from_slice(body).unwrap();
    assert_eq!(grant["state"], "state");
    assert_eq!(grant["code_verifier"], "verifier");
    assert_eq!(
        grant["provider_state"]["registration"], "client-9",
        "what authorize put in its pocket is what exchange was handed"
    );
}

fn window(percent: Option<Decimal>) -> QuotaEntry {
    QuotaEntry {
        id: "weekly-models".into(),
        source_id: "subscription".into(),
        label: None,
        subject: QuotaSubject::Account,
        model_scope: QuotaScope::ModelPrefixes(vec!["model-family".into()]),
        value: QuotaValue::Window(QuotaAllowance {
            used_percent: percent,
            period_end_ms: Some(9000),
            ..Default::default()
        }),
    }
}
impl QuotaQuery for Demo {
    fn query<'a>(&'a self, ctx: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            account_call(ctx, "/usage", json!({})).await?;
            Ok(QuotaSnapshot {
                observed_at_ms: 1000,
                entries: vec![window(Some(Decimal::ZERO))],
            })
        })
    }
}
impl QuotaReset for Demo {
    fn credits<'a>(&'a self, ctx: CredentialContext<'a>) -> OperationFuture<'a, QuotaResetCredits> {
        Box::pin(async move {
            account_call(ctx, "/credits", json!({})).await?;
            Ok(QuotaResetCredits {
                credit_expirations_ms: Vec::new(),
                available_count: Some(2),
                options: Vec::new(),
                expires_at_ms: Some(8000),
            })
        })
    }
    fn reset<'a>(
        &'a self,
        ctx: CredentialContext<'a>,
        request: gproxy_channel::channel::QuotaResetRequest<'a>,
    ) -> OperationFuture<'a, QuotaResetResult> {
        Box::pin(async move {
            account_call(
                ctx,
                "/reset",
                json!({"redeem_request_id":request.redeem_request_id}),
            )
            .await?;
            Ok(QuotaResetResult {
                reason: None,
                outcome: QuotaResetOutcome::Reset,
                windows_reset: Some(1),
                clears: Vec::new(),
            })
        })
    }
}
impl QuotaHeaders for Demo {
    fn observe(&self, ctx: QuotaHeaderContext<'_>) -> Result<Vec<QuotaEntry>, ChannelError> {
        Ok(ctx
            .headers
            .get("x-used-percent")
            .map(|value| vec![window(Some(value.to_str().unwrap().parse().unwrap()))])
            .unwrap_or_default())
    }
}
#[tokio::test]
async fn quota_query_reset_and_response_observation_are_independent() {
    let config = json!({});
    let secret = token_json();
    let client = ScriptClient::new((0..3).map(|_| reply(StatusCode::OK, json!({}))).collect());
    let ctx = CredentialContext {
        provider: provider(&config),
        credential: credential(&secret),
        client: &client,
    };
    let base: &dyn BaseChannel = &Demo;
    let snapshot = base.quota_query().unwrap().query(ctx).await.unwrap();
    let QuotaValue::Window(window) = &snapshot.entries[0].value else {
        panic!("expected window")
    };
    assert_eq!(window.used_percent, Some(Decimal::ZERO));
    assert_eq!(window.remaining, None);
    assert_eq!(window.period_start_ms, None);
    assert_eq!(client.requests.lock().unwrap().len(), 1);
    assert_eq!(
        base.quota_reset()
            .unwrap()
            .credits(ctx)
            .await
            .unwrap()
            .available_count,
        Some(2)
    );
    assert_eq!(
        base.quota_reset()
            .unwrap()
            .reset(
                ctx,
                gproxy_channel::channel::QuotaResetRequest {
                    redeem_request_id: "redeem-1",
                    program: None,
                    grant_id: None
                }
            )
            .await
            .unwrap()
            .outcome,
        QuotaResetOutcome::Reset
    );
    let mut headers = HeaderMap::new();
    assert!(
        base.quota_headers()
            .unwrap()
            .observe(QuotaHeaderContext {
                operation: KEY,
                upstream_model: "model-family-x",
                status: StatusCode::OK,
                headers: &headers
            })
            .unwrap()
            .is_empty()
    );
    headers.insert("x-used-percent", "2.5".parse().unwrap());
    let observed = base
        .quota_headers()
        .unwrap()
        .observe(QuotaHeaderContext {
            operation: KEY,
            upstream_model: "model-family-x",
            status: StatusCode::OK,
            headers: &headers,
        })
        .unwrap();
    let QuotaValue::Window(window) = &observed[0].value else {
        panic!("expected window")
    };
    assert_eq!(window.used_percent, Some(Decimal::new(25, 1)));
    let requests = client.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert!(
        requests
            .iter()
            .all(|r| r.headers()["authorization"] == "Bearer test-access")
    );
    let HttpBody::Bytes(body) = requests[2].body() else {
        panic!("expected reset request")
    };
    assert_eq!(
        serde_json::from_slice::<Value>(body).unwrap()["redeem_request_id"],
        "redeem-1"
    );
}

/// A vendor that states the price it charged beside the usage object.
impl UsageExtras for Demo {
    fn read(&self, source: UsageSource<'_>, usage: &mut NormalizedUsage) {
        if let Some(cost) = usage_object(source.root)
            .and_then(|usage| usage.get("cost"))
            .and_then(Value::as_u64)
        {
            usage
                .metrics
                .insert("upstream_cost_usd".into(), cost.into());
        }
        if let Some(tier) = source
            .headers
            .get("x-tier")
            .and_then(|value| value.to_str().ok())
        {
            usage.actual_service_tier = Some(tier.into());
        }
    }
}
#[test]
fn extras_add_to_the_standard_reading_and_never_invent_one() {
    let base: &dyn BaseChannel = &Demo;
    let mut headers = HeaderMap::new();
    let settle = |headers: &HeaderMap, root: &Value, standard: Option<NormalizedUsage>| {
        with_extras(
            base.usage_extras(),
            UsageSource {
                operation: KEY,
                headers,
                root,
            },
            standard,
        )
    };
    let mut standard = NormalizedUsage::default();
    standard.tokens.output_tokens = Some(0);
    standard.completeness = UsageCompleteness::Complete;

    // Nothing the extras recognize: the standard reading, explicit zero kept.
    let plain = settle(&headers, &json!({"usage": {}}), Some(standard.clone())).unwrap();
    assert_eq!(plain, standard);
    // No standard usage and nothing of the vendor's: still no reading.
    assert!(settle(&headers, &Value::Null, None).is_none());

    // The vendor's own fields, from the usage object and the headers.
    headers.insert("x-tier", "priority".parse().unwrap());
    let priced = settle(
        &headers,
        &json!({"response": {"usage": {"cost": 3}}}),
        Some(standard.clone()),
    )
    .unwrap();
    assert_eq!(priced.tokens.output_tokens, Some(0));
    assert_eq!(priced.metrics["upstream_cost_usd"], Decimal::from(3));
    assert_eq!(priced.actual_service_tier.as_deref(), Some("priority"));

    // A reply with no standard usage keeps what the vendor alone reported.
    let alone = settle(&HeaderMap::new(), &json!({"usage": {"cost": 1}}), None).unwrap();
    assert_eq!(alone.metrics["upstream_cost_usd"], Decimal::from(1));
    assert_eq!(alone.tokens.output_tokens, None);

    // A channel without extras settles with the standard reading as is.
    assert!(Bare.usage_extras().is_none());
    assert_eq!(
        with_extras(
            Bare.usage_extras(),
            UsageSource {
                operation: KEY,
                headers: &headers,
                root: &json!({"usage": {"cost": 1}}),
            },
            None,
        ),
        None
    );
}

static ROUTES: [ServiceRoute; 1] = [ServiceRoute {
    method: Method::GET,
    path_template: "/profiles/me",
    transport: ServiceTransport::Http,
    idempotent: true,
    class: ServiceClass::Catalog,
}];
impl ChannelServices for Demo {
    fn routes(&self) -> &[ServiceRoute] {
        &ROUTES
    }
    fn call<'a>(&'a self, ctx: ServiceContext<'a>) -> OperationFuture<'a, WireResponse> {
        Box::pin(async move {
            ctx.account
                .client
                .send(
                    http::Request::builder()
                        .method(ctx.request.method)
                        .uri(format!(
                            "{}{}",
                            ctx.account.provider.base_url.unwrap(),
                            ctx.request.path
                        ))
                        .header(
                            "authorization",
                            format!(
                                "Bearer {}",
                                ctx.account.credential.secret["access_token"]
                                    .as_str()
                                    .unwrap()
                            ),
                        )
                        .body(ctx.request.body)
                        .unwrap(),
                )
                .await
                .map_err(ChannelError::from)
        })
    }
}
impl ChannelServices for Bare {}
#[tokio::test]
async fn cli_service_uses_assigned_client_and_keeps_response_streaming() {
    let config = json!({});
    let secret = token_json();
    let client = ScriptClient::new(vec![WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        body: HttpBody::Stream(Box::pin(stream::iter([Ok(Bytes::from_static(b"profile"))]))),
    }]);
    let ctx = CredentialContext {
        provider: provider(&config),
        credential: credential(&secret),
        client: &client,
    };
    let caller = support::ScriptCaller::member("m");
    let request = || WireRequest {
        method: Method::GET,
        path: "/vendor-specific".into(),
        query: None,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::new()),
    };
    assert!(matches!(
        ChannelServices::call(
            &Bare,
            ServiceContext {
                account: ctx,
                accounts: &[],
                caller: &caller,
                view: ServiceView::Caller,
                request: request()
            }
        )
        .await,
        Err(ChannelError::UnsupportedService)
    ));
    let base: &dyn BaseChannel = &Demo;
    let services = base.services().unwrap();
    assert_eq!(services.routes()[0].path_template, "/profiles/me");
    let response = services
        .call(ServiceContext {
            account: ctx,
            accounts: &[],
            caller: &caller,
            view: ServiceView::Caller,
            request: request(),
        })
        .await
        .unwrap();
    let HttpBody::Stream(mut body) = response.body else {
        panic!("buffered service response")
    };
    assert_eq!(body.next().await.unwrap().unwrap(), "profile");
    assert_eq!(client.requests.lock().unwrap().len(), 1);
    let handshake = WireRequest {
        method: Method::GET,
        path: "/remote".into(),
        query: None,
        headers: HeaderMap::new(),
        body: (),
    };
    assert!(matches!(
        services
            .connect(ServiceContext {
                account: ctx,
                accounts: &[],
                caller: &caller,
                view: ServiceView::Caller,
                request: handshake
            })
            .await,
        Err(ChannelError::UnsupportedService)
    ));
}
