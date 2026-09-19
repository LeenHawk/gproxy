use std::{collections::VecDeque, sync::Mutex};

use futures_util::{StreamExt, stream};
use gproxy_channel::channel::*;
use gproxy_channel::{BaseChannel, ChannelError, OutboundClient};
use gproxy_protocol::capability::{CapabilityError, CapabilityFuture};
use gproxy_protocol::connection::{Bytes, StreamFraming, WsFrame};
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
    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        Some(self)
    }
    fn usage_stream(&self) -> Option<&dyn UsageStream> {
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
    assert!(channel.usage_extractor().is_none());
    assert!(channel.usage_stream().is_none());
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
                "redirect_uri":code.redirect_uri,"code_verifier":code.code_verifier}),
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
    ) -> OperationFuture<'a, OAuthCredential> {
        Box::pin(async move {
            Ok(serde_json::from_value(payload(
                login_call(ctx, "/cookie", json!({"cookie":cookie})).await?,
            ))
            .unwrap())
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
            .access_token,
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
                available_count: 2,
                expires_at_ms: Some(8000),
            })
        })
    }
    fn reset<'a>(
        &'a self,
        ctx: CredentialContext<'a>,
        redeem_request_id: &'a str,
    ) -> OperationFuture<'a, QuotaResetResult> {
        Box::pin(async move {
            account_call(
                ctx,
                "/reset",
                json!({"redeem_request_id":redeem_request_id}),
            )
            .await?;
            Ok(QuotaResetResult {
                outcome: QuotaResetOutcome::Reset,
                windows_reset: Some(1),
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
        2
    );
    assert_eq!(
        base.quota_reset()
            .unwrap()
            .reset(ctx, "redeem-1")
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

#[derive(Default)]
struct Observer {
    pending: Vec<u8>,
    latest: Option<NormalizedUsage>,
}
impl Observer {
    fn record(&mut self, bytes: &[u8]) -> Result<(), ChannelError> {
        let record: Value = serde_json::from_slice(bytes)
            .map_err(|e| ChannelError::InvalidResponse(e.to_string()))?;
        let usage = self.latest.get_or_insert_with(NormalizedUsage::default);
        if let Some(value) = record["output_tokens"].as_u64() {
            usage.tokens.output_tokens = Some(value);
        }
        if let Some(value) = record["cached_input_tokens"].as_u64() {
            usage.tokens.cached_input_tokens = Some(value);
        }
        if let Some(tier) = record["service_tier"].as_str() {
            usage.actual_service_tier = Some(tier.into());
        }
        usage.completeness = if record["done"] == true {
            UsageCompleteness::Complete
        } else {
            UsageCompleteness::Partial
        };
        Ok(())
    }
}
impl UsageObserver for Observer {
    fn observe(&mut self, frame: UsageFrame<'_>) -> Result<(), ChannelError> {
        match frame {
            UsageFrame::HttpChunk(chunk) => {
                self.pending.extend_from_slice(chunk);
                while let Some(pos) = self.pending.iter().position(|byte| *byte == b'\n') {
                    let record: Vec<_> = self.pending.drain(..=pos).collect();
                    self.record(&record)?;
                }
            }
            UsageFrame::WebSocket(WsFrame::Text(text)) => self.record(text.as_bytes())?,
            UsageFrame::WebSocket(_) => {}
        }
        Ok(())
    }
    fn snapshot(&self) -> Option<NormalizedUsage> {
        self.latest.clone()
    }
    fn finish(
        mut self: Box<Self>,
        end: UsageStreamEnd,
    ) -> Result<Option<NormalizedUsage>, ChannelError> {
        if end == UsageStreamEnd::Complete && !self.pending.is_empty() {
            let tail = std::mem::take(&mut self.pending);
            self.record(&tail)?;
        }
        if end == UsageStreamEnd::Interrupted
            && let Some(usage) = self.latest.as_mut()
        {
            usage.completeness = UsageCompleteness::Partial;
        }
        Ok(self.latest)
    }
}
impl UsageExtractor for Demo {
    fn extract(&self, ctx: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        if ctx.response.body.is_empty() {
            return Ok(None);
        }
        let mut observer = Observer::default();
        observer.record(ctx.response.body)?;
        Ok(observer.latest)
    }
}
impl UsageStream for Demo {
    fn start(&self, _: UsageStreamContext<'_>) -> Result<Box<dyn UsageObserver>, ChannelError> {
        Ok(Box::<Observer>::default())
    }
}
#[test]
fn metering_keeps_unknown_zero_partial_and_cumulative_usage_distinct() {
    let base: &dyn BaseChannel = &Demo;
    let headers = HeaderMap::new();
    let mut observer = base
        .usage_stream()
        .unwrap()
        .start(UsageStreamContext {
            operation: KEY,
            request_body: None,
            status: StatusCode::OK,
            headers: &headers,
            transport: UsageTransport::Http {
                framing: Some(StreamFraming::NdJson),
            },
        })
        .unwrap();
    assert!(observer.snapshot().is_none());
    observer
        .observe(UsageFrame::HttpChunk(b"{\"output_tokens\":"))
        .unwrap();
    assert!(observer.snapshot().is_none());
    observer
        .observe(UsageFrame::HttpChunk(b"0}\n{\"output_tokens\":3}\n"))
        .unwrap();
    assert_eq!(observer.snapshot().unwrap().tokens.output_tokens, Some(3));
    observer
        .observe(UsageFrame::HttpChunk(
            b"{\"output_tokens\":5,\"cached_input_tokens\":0,\"service_tier\":\"priority\"}\n",
        ))
        .unwrap();
    let usage = observer
        .finish(UsageStreamEnd::Interrupted)
        .unwrap()
        .unwrap();
    assert_eq!(usage.tokens.output_tokens, Some(5));
    assert_eq!(usage.tokens.cached_input_tokens, Some(0));
    assert_eq!(usage.tokens.input_tokens, None);
    assert_eq!(usage.actual_service_tier.as_deref(), Some("priority"));
    assert_eq!(usage.completeness, UsageCompleteness::Partial);
    let mut ws = base
        .usage_stream()
        .unwrap()
        .start(UsageStreamContext {
            operation: KEY,
            request_body: None,
            status: StatusCode::SWITCHING_PROTOCOLS,
            headers: &headers,
            transport: UsageTransport::WebSocket,
        })
        .unwrap();
    ws.observe(UsageFrame::WebSocket(&WsFrame::Text(
        "{\"output_tokens\":2,\"done\":true}".into(),
    )))
    .unwrap();
    assert_eq!(
        ws.finish(UsageStreamEnd::Complete)
            .unwrap()
            .unwrap()
            .completeness,
        UsageCompleteness::Complete
    );
    let extract = base.usage_extractor().unwrap();
    assert!(
        extract
            .extract(UsageContext {
                operation: KEY,
                request_body: None,
                response: ResponseView {
                    status: StatusCode::OK,
                    headers: &headers,
                    body: b""
                }
            })
            .unwrap()
            .is_none()
    );
    assert_eq!(
        extract
            .extract(UsageContext {
                operation: KEY,
                request_body: None,
                response: ResponseView {
                    status: StatusCode::OK,
                    headers: &headers,
                    body: b"{\"output_tokens\":0,\"done\":true}"
                }
            })
            .unwrap()
            .unwrap()
            .tokens
            .output_tokens,
        Some(0)
    );
}

static ROUTES: [ServiceRoute; 1] = [ServiceRoute {
    method: Method::GET,
    path_template: "/profiles/me",
    transport: ServiceTransport::Http,
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
                request: handshake
            })
            .await,
        Err(ChannelError::UnsupportedService)
    ));
}
