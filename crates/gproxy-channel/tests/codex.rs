#![cfg(feature = "codex")]

mod support;

use base64::Engine;
use gproxy_channel::{
    BaseChannel, ChannelError, OutboundClient,
    channel::{
        AuthorizationRequest, CallerUsage, CallerUsageWindow, CredentialContext, CredentialView,
        DevicePoll, LoginContext, PrepareContext, ProviderView, QuotaHeaderContext, QuotaValue,
        ResponseView, ServiceContext, ServiceView, UsageContext, UsageFrame, UsageStreamContext,
        UsageStreamEnd, UsageTransport,
    },
    channels::codex::{CLI_VERSION, Codex, DEFAULT_CLIENT_ID, KIND_FILE, KIND_PLUGIN, KIND_TASK},
};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    capability::{CapabilityError, CapabilityFuture, UpstreamConnection},
    connection::{Bytes, StreamFraming},
};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use support::ScriptCaller;

fn jwt(claims: Value) -> String {
    let b64 = |v: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v);
    format!(
        "{}.{}.{}",
        b64(br#"{"alg":"RS256"}"#),
        b64(claims.to_string().as_bytes()),
        b64(b"sig")
    )
}

type Sent = (Method, String, HeaderMap, Vec<u8>);

struct ScriptClient {
    replies: Mutex<VecDeque<WireResponse>>,
    requests: Mutex<Vec<Sent>>,
}
impl ScriptClient {
    fn new(replies: Vec<WireResponse>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
        }
    }
    fn sent(&self) -> Vec<Sent> {
        self.requests.lock().unwrap().clone()
    }
}
impl OutboundClient for ScriptClient {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse, CapabilityError>> {
        Box::pin(async move {
            let (parts, body) = request.into_parts();
            let body = match body {
                HttpBody::Bytes(bytes) => bytes.to_vec(),
                HttpBody::Stream(_) => Vec::new(),
            };
            self.requests.lock().unwrap().push((
                parts.method,
                parts.uri.to_string(),
                parts.headers,
                body,
            ));
            Ok(self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected upstream call"))
        })
    }

    /// Records the handshake as a CONNECT and rejects it, which is enough to
    /// see the URL and headers a WebSocket service would use.
    fn connect<'a>(
        &'a self,
        request: http::Request<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(async move {
            let (parts, ()) = request.into_parts();
            self.requests.lock().unwrap().push((
                Method::CONNECT,
                parts.uri.to_string(),
                parts.headers,
                Vec::new(),
            ));
            Ok(UpstreamConnection::Rejected(WireResponse {
                status: StatusCode::UNAUTHORIZED,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::new()),
            }))
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

fn provider<'a>(config: &'a Value, base_url: Option<&'a str>) -> ProviderView<'a> {
    ProviderView {
        id: "codex",
        channel: "codex",
        base_url,
        config,
    }
}

fn secret(access: &str) -> Value {
    json!({
        "access_token": access,
        "refresh_token": "rt-1",
        "id_token": jwt(json!({"https://api.openai.com/auth": {"chatgpt_account_id": "acct-1", "chatgpt_plan_type": "pro"}})),
        "provider_fields": {"chatgpt_account_id": "acct-1", "chatgpt_plan_type": "pro"}
    })
}

fn credential<'a>(secret: &'a Value, metadata: &'a Value) -> CredentialView<'a> {
    CredentialView {
        id: "c",
        provider_id: "codex",
        auth_kind: "oauth",
        secret,
        metadata,
        version: 3,
        expires_at_ms: None,
    }
}

#[test]
fn default_connection_is_the_cli_transport_identity() {
    use gproxy_client::{Backend, RetryPolicy};
    let config = Codex.default_connection().expect("channel default");
    assert_eq!(config.backend, Backend::ReqwestNative);
    assert!(
        config.emulation.is_none(),
        "native TLS as the CLI's reqwest"
    );
    assert!(
        !(config.gzip || config.brotli || config.deflate || config.zstd),
        "reqwest 0.12 default features decode nothing"
    );
    assert_eq!(config.retry, RetryPolicy::Never);
    assert_eq!(
        config.redirect_max_hops, 10,
        "reqwest's default redirect limit"
    );
}

#[test]
fn prepares_responses_calls_against_the_codex_backend() {
    let config = json!({});
    let secret = secret("at-1");
    let metadata = json!({"chatgpt_account_id": "acct-meta"});
    let mut headers = HeaderMap::new();
    headers.insert("authorization", HeaderValue::from_static("Bearer client"));
    headers.insert("chatgpt-account-id", HeaderValue::from_static("spoof"));
    headers.insert("session-id", HeaderValue::from_static("sess-1"));
    let request = Codex
        .prepare(PrepareContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            operation: OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: Dialect::OpenAi,
            },
            request: WireRequest {
                method: Method::POST,
                path: "/v1/responses".into(),
                query: None,
                headers,
                body: HttpBody::Bytes(Bytes::from_static(b"{}")),
            },
            endpoint_override: None,
        })
        .unwrap();
    assert_eq!(
        request.uri().to_string(),
        "https://chatgpt.com/backend-api/codex/responses",
        "the backend path wins over the client's native path"
    );
    let h = request.headers();
    assert_eq!(h["authorization"], "Bearer at-1");
    assert_eq!(
        h["chatgpt-account-id"], "acct-meta",
        "metadata over secret over client"
    );
    assert_eq!(h["originator"], "codex_cli_rs");
    let agent = h["user-agent"].to_str().unwrap();
    assert!(
        agent.starts_with(&format!("codex_cli_rs/{CLI_VERSION} (")),
        "CLI-shaped user agent: {agent}"
    );
    let (host, terminal) = agent.rsplit_once(") ").unwrap();
    assert!(host.contains("; "), "os version; arch: {host}");
    assert!(!terminal.is_empty() && !terminal.contains(' '));
    assert_eq!(h["session-id"], "sess-1", "vendor headers pass through");
    assert!(h.get("openai-beta").is_none());

    let socket = Codex
        .prepare_connect(PrepareContext {
            provider: provider(&config, Some("https://mirror.example/backend-api/codex/")),
            credential: credential(&secret, &Value::Null),
            operation: OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: Dialect::OpenAiResponsesWebSocket,
            },
            request: WireRequest {
                method: Method::GET,
                path: "/v1/responses".into(),
                query: None,
                headers: HeaderMap::new(),
                body: (),
            },
            endpoint_override: None,
        })
        .unwrap();
    assert_eq!(
        socket.uri().to_string(),
        "wss://mirror.example/backend-api/codex/responses"
    );
    assert_eq!(
        socket.headers()["openai-beta"],
        "responses_websockets=2026-02-06"
    );
    assert_eq!(
        socket.headers()["chatgpt-account-id"],
        "acct-1",
        "account from the secret when metadata has none"
    );

    let compact = Codex
        .prepare(PrepareContext {
            provider: provider(&config, None),
            credential: credential(&secret, &Value::Null),
            operation: OperationKey {
                operation: Operation::CompactContent,
                dialect: Dialect::OpenAi,
            },
            request: WireRequest {
                method: Method::POST,
                path: "/v1/responses/compact".into(),
                query: None,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::new()),
            },
            endpoint_override: None,
        })
        .unwrap();
    assert_eq!(
        compact.uri().to_string(),
        "https://chatgpt.com/backend-api/codex/responses/compact"
    );
    assert!(matches!(
        Codex.prepare(PrepareContext {
            provider: provider(&config, None),
            credential: credential(&secret, &Value::Null),
            operation: OperationKey {
                operation: Operation::ListModels,
                dialect: Dialect::OpenAi,
            },
            request: WireRequest {
                method: Method::GET,
                path: "/v1/models".into(),
                query: None,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::new()),
            },
            endpoint_override: None,
        }),
        Err(ChannelError::UnsupportedOperation(_))
    ));
    assert_eq!(
        Codex.native_dialects(provider(&config, None), Operation::GenerateContent),
        vec![Dialect::OpenAi, Dialect::OpenAiResponsesWebSocket]
    );
}

#[tokio::test]
async fn refresh_rotates_tokens_and_classifies_rejections() {
    let config = json!({});
    let new_access = jwt(json!({"exp": 1_800_000_000}));
    let new_id = jwt(json!({
        "email": "me@example.com",
        "https://api.openai.com/auth": {"chatgpt_account_id": "acct-2", "chatgpt_plan_type": "plus"}
    }));
    let client = ScriptClient::new(vec![
        reply(
            StatusCode::OK,
            json!({"access_token": new_access, "refresh_token": "rt-2", "id_token": new_id}),
        ),
        reply(
            StatusCode::BAD_REQUEST,
            json!({"error": {"code": "invalid_grant", "message": "expired"}}),
        ),
        reply(StatusCode::BAD_GATEWAY, json!({"error": "upstream"})),
        reply(StatusCode::UNAUTHORIZED, json!({})),
    ]);
    let secret = secret("at-old");
    let refresher = Codex.credential_refresh().unwrap();
    let context = || CredentialContext {
        provider: provider(&config, None),
        credential: credential(&secret, &Value::Null),
        client: &client,
    };
    let update = refresher.refresh(context()).await.unwrap();
    assert_eq!(update.expires_at_ms, Some(1_800_000_000_000));
    assert_eq!(update.secret["access_token"], new_access);
    assert_eq!(update.secret["refresh_token"], "rt-2");
    assert_eq!(
        update.secret["provider_fields"]["chatgpt_account_id"],
        "acct-2"
    );
    assert_eq!(
        update.secret["provider_fields"]["chatgpt_plan_type"],
        "plus"
    );
    assert_eq!(update.secret["provider_fields"]["email"], "me@example.com");
    let sent = client.sent();
    assert_eq!(sent[0].0, Method::POST);
    assert_eq!(sent[0].1, "https://auth.openai.com/oauth/token");
    let body: Value = serde_json::from_slice(&sent[0].3).unwrap();
    assert_eq!(body["grant_type"], "refresh_token");
    assert_eq!(body["refresh_token"], "rt-1");
    assert_eq!(body["client_id"], DEFAULT_CLIENT_ID);

    let error = refresher.refresh(context()).await.err().expect("rejected");
    assert!(
        matches!(&error, ChannelError::RefreshRejected(code) if code == "invalid_grant"),
        "{error}"
    );
    let error = refresher.refresh(context()).await.err().expect("transient");
    assert!(
        matches!(error, ChannelError::UpstreamResponse { status, .. } if status == StatusCode::BAD_GATEWAY),
        "a 5xx is transient"
    );
    let error = refresher.refresh(context()).await.err().expect("rejected");
    assert!(
        matches!(error, ChannelError::RefreshRejected(_)),
        "401 is final"
    );
}

#[tokio::test]
async fn device_code_login_polls_then_exchanges_the_code() {
    let config = json!({"issuer": "https://auth.example/"});
    let id_token = jwt(json!({"https://api.openai.com/auth": {"chatgpt_account_id": "acct-9"}}));
    let access = jwt(json!({"exp": 1_700_000_000}));
    let client = ScriptClient::new(vec![
        reply(
            StatusCode::OK,
            json!({"device_auth_id": "dev-1", "usercode": "ABCD-EFGH", "interval": "7"}),
        ),
        reply(StatusCode::FORBIDDEN, json!({})),
        reply(
            StatusCode::OK,
            json!({"authorization_code": "code-1", "code_challenge": "cc", "code_verifier": "cv"}),
        ),
        reply(
            StatusCode::OK,
            json!({"access_token": access, "refresh_token": "rt-9", "id_token": id_token}),
        ),
    ]);
    let login = Codex.oauth_device_code().unwrap();
    let context = LoginContext {
        provider: provider(&config, None),
        client: &client,
    };
    let start = login.start(context).await.unwrap();
    assert_eq!(start.device_code, "dev-1");
    assert_eq!(start.user_code, "ABCD-EFGH");
    assert_eq!(start.verification_uri, "https://auth.example/codex/device");
    assert_eq!(start.interval_secs, 7);
    assert!(matches!(
        login.poll(context, &start).await.unwrap(),
        DevicePoll::Pending
    ));
    let DevicePoll::Ready(credential) = login.poll(context, &start).await.unwrap() else {
        panic!("ready");
    };
    assert_eq!(credential.access_token, access);
    assert_eq!(credential.refresh_token.as_deref(), Some("rt-9"));
    assert_eq!(credential.expires_at_ms, Some(1_700_000_000_000));
    assert_eq!(credential.provider_fields["chatgpt_account_id"], "acct-9");
    let sent = client.sent();
    assert_eq!(
        sent[0].1,
        "https://auth.example/api/accounts/deviceauth/usercode"
    );
    assert_eq!(
        sent[2].1,
        "https://auth.example/api/accounts/deviceauth/token"
    );
    assert_eq!(sent[3].1, "https://auth.example/oauth/token");
    assert_eq!(
        sent[3].2["content-type"],
        "application/x-www-form-urlencoded"
    );
    let form = String::from_utf8(sent[3].3.clone()).unwrap();
    assert!(form.contains("grant_type=authorization_code"), "{form}");
    assert!(form.contains("code=code-1"), "{form}");
    assert!(form.contains("code_verifier=cv"), "{form}");
    assert!(
        form.contains("redirect_uri=https%3A%2F%2Fauth.example%2Fdeviceauth%2Fcallback"),
        "{form}"
    );

    let authorize = Codex
        .oauth_authorization_code()
        .unwrap()
        .authorize(
            context,
            AuthorizationRequest {
                redirect_uri: "http://localhost:1455/auth/callback",
                state: "st",
                code_challenge: "ch",
            },
        )
        .await
        .unwrap();
    assert!(
        authorize
            .authorize_url
            .starts_with("https://auth.example/oauth/authorize?")
    );
    for expected in [
        "response_type=code",
        "code_challenge=ch",
        "code_challenge_method=S256",
        "codex_cli_simplified_flow=true",
        "state=st",
        "originator=codex_cli_rs",
    ] {
        assert!(authorize.authorize_url.contains(expected), "{expected}");
    }
}

#[test]
fn rate_limit_headers_become_quota_entries_per_family() {
    let mut headers = HeaderMap::new();
    for (name, value) in [
        ("x-codex-primary-used-percent", "12.5"),
        ("x-codex-primary-window-minutes", "300"),
        ("x-codex-primary-reset-at", "1700000000"),
        ("x-codex-secondary-used-percent", "100"),
        ("x-codex-secondary-window-minutes", "10080"),
        ("x-codex-secondary-reset-at", "1700400000"),
        ("x-codex-bengalfox-primary-used-percent", "3"),
        ("x-codex-bengalfox-limit-name", "GPT-5.3-Codex-Spark"),
        ("x-codex-credits-has-credits", "true"),
        ("x-codex-credits-unlimited", "false"),
        ("x-codex-credits-balance", "42.5"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    let entries = Codex
        .quota_headers()
        .unwrap()
        .observe(QuotaHeaderContext {
            operation: OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: Dialect::OpenAi,
            },
            upstream_model: "gpt-5-codex",
            status: StatusCode::OK,
            headers: &headers,
        })
        .unwrap();
    let ids: Vec<&str> = entries.iter().map(|e| e.source_id.as_str()).collect();
    assert_eq!(
        ids,
        vec![
            "codex_primary",
            "codex_secondary",
            "codex_bengalfox_primary",
            "codex_credits"
        ]
    );
    let QuotaValue::Window(primary) = &entries[0].value else {
        panic!("window");
    };
    assert_eq!(primary.used_percent, Some("12.5".parse().unwrap()));
    assert_eq!(primary.period_end_ms, Some(1_700_000_000_000));
    assert_eq!(
        primary.period_start_ms,
        Some(1_700_000_000_000 - 300 * 60 * 1000)
    );
    let QuotaValue::Window(secondary) = &entries[1].value else {
        panic!("window");
    };
    assert_eq!(secondary.remaining, Some(0.into()), "exhausted");
    assert_eq!(entries[2].label.as_deref(), Some("GPT-5.3-Codex-Spark"));
    let QuotaValue::Balance(credits) = &entries[3].value else {
        panic!("balance");
    };
    assert_eq!(credits.remaining, Some("42.5".parse().unwrap()));
    let dims = Codex.quota_model().unwrap().dimensions(
        provider(&json!({}), None),
        credential(&secret("a"), &json!({"plan_type": "pro"})),
    );
    assert_eq!(dims.len(), 2);
    assert_eq!(dims[0].id, "codex_primary");
    assert_eq!(dims[0].label.as_deref(), Some("pro 5h window"));
    assert_eq!(dims[1].id, "codex_secondary");
}

#[tokio::test]
async fn wham_usage_is_queried_on_the_backend_and_parsed() {
    let config = json!({});
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({
            "plan_type": "pro",
            "rate_limit": {"allowed": true, "limit_reached": false,
                "primary_window": null,
                "secondary_window": {"used_percent": 40, "limit_window_seconds": 604800, "reset_after_seconds": 1000, "reset_at": 1700400000}},
            "additional_rate_limits": [{"limit_name": "GPT-5.3-Codex-Spark", "metered_feature": "codex_bengalfox",
                "rate_limit": {"allowed": true, "limit_reached": true,
                    "primary_window": {"used_percent": 100, "limit_window_seconds": 18000, "reset_after_seconds": 10, "reset_at": 1700001000},
                    "secondary_window": null}}],
            "credits": {"has_credits": false, "unlimited": false, "balance": null},
            "code_review_rate_limit": null
        }),
    )]);
    let s = secret("at");
    let snapshot = Codex
        .quota_query()
        .unwrap()
        .query(CredentialContext {
            provider: provider(&config, Some("https://chatgpt.com/backend-api/codex")),
            credential: credential(&s, &Value::Null),
            client: &client,
        })
        .await
        .unwrap();
    let sent = client.sent();
    assert_eq!(sent[0].0, Method::GET);
    assert_eq!(sent[0].1, "https://chatgpt.com/backend-api/wham/usage");
    assert_eq!(sent[0].2["authorization"], "Bearer at");
    assert_eq!(sent[0].2["chatgpt-account-id"], "acct-1");
    let ids: Vec<&str> = snapshot
        .entries
        .iter()
        .map(|e| e.source_id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec![
            "codex_secondary",
            "codex_bengalfox_primary",
            "codex_credits"
        ]
    );
    assert_eq!(
        snapshot.entries[1].label.as_deref(),
        Some("GPT-5.3-Codex-Spark")
    );
    let QuotaValue::Window(spark) = &snapshot.entries[1].value else {
        panic!("window");
    };
    assert_eq!(spark.used_percent, Some(100.into()));
    assert_eq!(spark.period_end_ms, Some(1_700_001_000_000));
    let QuotaValue::Balance(credits) = &snapshot.entries[2].value else {
        panic!("balance");
    };
    assert_eq!(credits.remaining, Some(0.into()));
}

#[test]
fn responses_usage_is_read_from_the_terminal_event_and_from_json() {
    let mut observer = Codex
        .usage_stream()
        .unwrap()
        .start(UsageStreamContext {
            operation: OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: Dialect::OpenAi,
            },
            request_body: None,
            status: StatusCode::OK,
            headers: &HeaderMap::new(),
            transport: UsageTransport::Http {
                framing: Some(StreamFraming::Sse),
            },
        })
        .unwrap();
    observer
        .observe(UsageFrame::HttpChunk(
            b"event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\nevent: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":30,\"input_tokens_details\":{\"cached_tokens\":10},\"output_tokens\":7,\"output_tokens_details\":{\"reasoning_tokens\":2}}}}\n\n",
        ))
        .unwrap();
    let usage = observer.finish(UsageStreamEnd::Complete).unwrap().unwrap();
    assert_eq!(
        usage.tokens.input_tokens,
        Some(20),
        "cached tokens are excluded"
    );
    assert_eq!(usage.tokens.cached_input_tokens, Some(10));
    assert_eq!(usage.tokens.output_tokens, Some(7));
    assert_eq!(usage.tokens.reasoning_tokens, Some(2));

    let body = json!({"id": "resp", "usage": {"input_tokens": 5, "output_tokens": 1}}).to_string();
    let extracted = Codex
        .usage_extractor()
        .unwrap()
        .extract(UsageContext {
            operation: OperationKey {
                operation: Operation::GenerateContent,
                dialect: Dialect::OpenAi,
            },
            request_body: None,
            response: ResponseView {
                status: StatusCode::OK,
                headers: &HeaderMap::new(),
                body: body.as_bytes(),
            },
        })
        .unwrap()
        .unwrap();
    assert_eq!(extracted.tokens.input_tokens, Some(5));
    let _ = Arc::new(());
}

#[test]
fn allowed_headers_applies_to_codex_too() {
    let config = json!({"allowed_headers": ["x-request-id"]});
    let secret = secret("at-1");
    let mut headers = HeaderMap::new();
    headers.insert("x-request-id", HeaderValue::from_static("req-1"));
    headers.insert("x-custom", HeaderValue::from_static("dropped"));
    // The CLI's own headers pass regardless of the provider list.
    headers.insert("session-id", HeaderValue::from_static("sess-1"));
    headers.insert("thread-id", HeaderValue::from_static("thread-1"));
    headers.insert("x-client-request-id", HeaderValue::from_static("thread"));
    headers.insert(
        "x-codex-turn-metadata",
        HeaderValue::from_static("{\"request_kind\":\"turn\"}"),
    );
    headers.insert("x-codex-turn-state", HeaderValue::from_static("sticky"));
    headers.insert("version", HeaderValue::from_static("0.155.1"));
    headers.insert("openai-beta", HeaderValue::from_static("spoof"));
    let request = Codex
        .prepare(PrepareContext {
            provider: provider(&config, None),
            credential: credential(&secret, &Value::Null),
            operation: OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: Dialect::OpenAi,
            },
            request: WireRequest {
                method: Method::POST,
                path: "/v1/responses".into(),
                query: None,
                headers,
                body: HttpBody::Bytes(Bytes::new()),
            },
            endpoint_override: None,
        })
        .unwrap();
    let h = request.headers();
    assert_eq!(h["x-request-id"], "req-1");
    assert!(h.get("x-custom").is_none());
    assert_eq!(h["session-id"], "sess-1");
    assert_eq!(h["thread-id"], "thread-1");
    assert_eq!(h["x-client-request-id"], "thread");
    assert_eq!(h["x-codex-turn-metadata"], "{\"request_kind\":\"turn\"}");
    assert_eq!(h["x-codex-turn-state"], "sticky");
    assert_eq!(h["version"], "0.155.1");
    assert!(
        h.get("openai-beta").is_none(),
        "channel identity headers are never client-supplied"
    );
    assert_eq!(h["authorization"], "Bearer at-1");
}

// ------------------------------------------------------------ services

fn service_request(method: Method, path: &str, query: Option<&str>, body: &str) -> WireRequest {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", HeaderValue::from_static("Bearer client"));
    headers.insert("chatgpt-account-id", HeaderValue::from_static("spoof"));
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    headers.insert("accept", HeaderValue::from_static("text/event-stream"));
    headers.insert("mcp-session-id", HeaderValue::from_static("mcp-1"));
    headers.insert("x-codex-foo", HeaderValue::from_static("bar"));
    WireRequest {
        method,
        path: path.into(),
        query: query.map(str::to_owned),
        headers,
        body: HttpBody::Bytes(Bytes::from(body.to_owned())),
    }
}

fn account<'a>(
    config: &'a Value,
    secret: &'a Value,
    metadata: &'a Value,
    client: &'a ScriptClient,
) -> CredentialContext<'a> {
    CredentialContext {
        provider: provider(config, None),
        credential: credential(secret, metadata),
        client,
    }
}

fn context<'a>(
    accounts: &'a [CredentialContext<'a>],
    caller: &'a ScriptCaller,
    view: ServiceView,
    request: WireRequest,
) -> ServiceContext<'a> {
    ServiceContext {
        account: accounts[0],
        accounts,
        caller,
        view,
        request,
    }
}

async fn body_json(response: WireResponse) -> Value {
    let HttpBody::Bytes(bytes) = response.body else {
        panic!("local answers are buffered");
    };
    serde_json::from_slice(&bytes).unwrap()
}

fn views() -> [ServiceView; 3] {
    [
        ServiceView::Caller,
        ServiceView::Pool,
        ServiceView::Credential("c".into()),
    ]
}

#[tokio::test]
async fn catalog_routes_forward_under_every_view_with_the_credential_identity() {
    let config = json!({});
    let secret = secret("at-1");
    let metadata = json!({"chatgpt_account_id": "acct-meta"});
    let client = ScriptClient::new(vec![
        reply(StatusCode::OK, json!({})),
        reply(StatusCode::OK, json!({})),
        reply(
            StatusCode::TOO_MANY_REQUESTS,
            json!({"detail": "slow down"}),
        ),
    ]);
    let accounts = [account(&config, &secret, &metadata, &client)];
    let member = ScriptCaller::member("m");
    let admin = ScriptCaller::admin("a");
    let services = Codex.services().expect("codex exposes services");
    for (view, caller) in views().into_iter().zip([&member, &admin, &admin]) {
        let response = services
            .call(context(
                &accounts,
                caller,
                view,
                service_request(
                    Method::GET,
                    "/ps/plugins/search",
                    Some("key=leaked&q=deploy"),
                    "",
                ),
            ))
            .await
            .unwrap();
        assert!(
            response.status == StatusCode::OK || response.status == StatusCode::TOO_MANY_REQUESTS
        );
    }
    let sent = client.sent();
    assert_eq!(sent.len(), 3, "forwarded under Caller, Pool and Credential");
    assert_eq!(
        sent[0].1, "https://chatgpt.com/backend-api/ps/plugins/search?q=deploy",
        "the bare mount maps onto the backend, `key` is stripped"
    );
    let h = &sent[0].2;
    assert_eq!(h["authorization"], "Bearer at-1");
    assert_eq!(h["chatgpt-account-id"], "acct-meta");
    assert_eq!(h["originator"], "codex_cli_rs");
    assert_eq!(h["content-type"], "application/json");
    assert_eq!(h["accept"], "text/event-stream");
    assert_eq!(h["mcp-session-id"], "mcp-1");
    assert_eq!(h["x-codex-foo"], "bar");
    assert_eq!(h.get_all("authorization").iter().count(), 1);
}

#[tokio::test]
async fn identity_is_synthesized_for_caller_and_pool_and_real_for_credential() {
    let config = json!({});
    let secret = secret("at-1");
    let metadata = json!({"chatgpt_account_id": "acct-meta", "plan_type": "team"});
    let client = ScriptClient::new(Vec::new());
    let accounts = [account(&config, &secret, &metadata, &client)];
    let member = ScriptCaller::member("m");
    let pool_admin = ScriptCaller::admin("pool:codex");
    let services = Codex.services().unwrap();
    let whoami = |caller, view| {
        services.call(context(
            &accounts,
            caller,
            view,
            service_request(Method::GET, "/v1/user-auth-credential/whoami", None, ""),
        ))
    };
    let caller = body_json(whoami(&member, ServiceView::Caller).await.unwrap()).await;
    assert_eq!(caller["email"], "m@gproxy.invalid");
    assert_eq!(
        caller["chatgpt_plan_type"], "team",
        "the tier is not identifying"
    );
    assert_eq!(caller["chatgpt_account_is_fedramp"], false);
    let account_id = caller["chatgpt_account_id"].as_str().unwrap().to_owned();
    assert!(account_id.starts_with("gproxy-account-") && account_id != "acct-meta");
    assert_ne!(caller["chatgpt_user_id"], account_id);
    let again = body_json(whoami(&member, ServiceView::Caller).await.unwrap()).await;
    assert_eq!(
        again["chatgpt_account_id"], account_id,
        "stable across calls"
    );

    let pool = body_json(whoami(&pool_admin, ServiceView::Pool).await.unwrap()).await;
    assert_ne!(
        pool["chatgpt_account_id"], account_id,
        "the pool identity core supplies renders as a different account"
    );

    let real = body_json(
        whoami(&pool_admin, ServiceView::Credential("c".into()))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(real["chatgpt_account_id"], "acct-meta");
    assert_eq!(real["email"], Value::Null, "unknown facts stay null");
    assert!(client.sent().is_empty(), "whoami never reaches the backend");

    let check = body_json(
        services
            .call(context(
                &accounts,
                &member,
                ServiceView::Caller,
                service_request(Method::GET, "/api/codex/accounts/check", None, ""),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(check["default_account_id"], account_id);
    assert_eq!(check["accounts"][0]["name"], "m");
}

#[tokio::test]
async fn usage_is_synthesized_from_caller_usage_and_raw_for_credential() {
    let config = json!({});
    let secret = secret("at-1");
    let metadata = Value::Null;
    let client = ScriptClient::new(vec![reply(StatusCode::OK, json!({"plan_type": "pro"}))]);
    let accounts = [account(&config, &secret, &metadata, &client)];
    let allotted = ScriptCaller::member("m").with_usage(CallerUsage {
        input_tokens: 10,
        output_tokens: 5,
        cost: Some("0.25".into()),
        windows: vec![CallerUsageWindow {
            key: "primary".into(),
            used_percent: Some(50.0),
            period_start_ms: Some(0),
            reset_at_ms: Some(18_000_000),
        }],
    });
    let bare = ScriptCaller::admin("a");
    let services = Codex.services().unwrap();
    let usage = |caller, view| {
        services.call(context(
            &accounts,
            caller,
            view,
            service_request(Method::GET, "/backend-api/wham/usage", None, ""),
        ))
    };
    let value = body_json(usage(&allotted, ServiceView::Caller).await.unwrap()).await;
    assert_eq!(value["local_usage"]["input_tokens"], 10);
    assert_eq!(value["local_usage"]["cost"], "0.25");
    assert_eq!(value["rate_limit"]["primary_window"]["used_percent"], 50);
    assert_eq!(value["rate_limit"]["primary_window"]["reset_at"], 18_000);
    assert_eq!(
        value["rate_limit"]["primary_window"]["limit_window_seconds"],
        18_000
    );
    assert!(value["rate_limit"].get("secondary_window").is_none());
    assert_eq!(value["rate_limit_reset_credits"]["available_count"], 0);
    assert_eq!(value["plan_type"], "pro");

    let value = body_json(usage(&bare, ServiceView::Pool).await.unwrap()).await;
    assert!(
        value.get("rate_limit").is_none(),
        "no windows allotted: no window fields"
    );
    assert!(client.sent().is_empty());

    let credits = body_json(
        services
            .call(context(
                &accounts,
                &allotted,
                ServiceView::Caller,
                service_request(
                    Method::POST,
                    "/backend-api/wham/rate-limit-reset-credits/consume",
                    None,
                    "{}",
                ),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(credits["code"], "no_credit");

    usage(&bare, ServiceView::Credential("c".into()))
        .await
        .unwrap();
    assert_eq!(
        client.sent()[0].1,
        "https://chatgpt.com/backend-api/wham/usage"
    );
}

#[tokio::test]
async fn settings_are_neutral_defaults_overridable_from_config() {
    let config = json!({"codex_virtual_settings": {"commit_attribution_enabled": true}});
    let plain = json!({});
    let secret = secret("at-1");
    let metadata = Value::Null;
    let client = ScriptClient::new(vec![reply(StatusCode::OK, json!({}))]);
    let member = ScriptCaller::member("m");
    let services = Codex.services().unwrap();
    let request = || service_request(Method::GET, "/api/codex/settings/user", None, "");
    let accounts = [account(&plain, &secret, &metadata, &client)];
    let value = body_json(
        services
            .call(context(&accounts, &member, ServiceView::Caller, request()))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(value, json!({"commit_attribution_enabled": false}));
    let accounts = [account(&config, &secret, &metadata, &client)];
    let value = body_json(
        services
            .call(context(&accounts, &member, ServiceView::Pool, request()))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(value["commit_attribution_enabled"], true);
    services
        .call(context(
            &accounts,
            &member,
            ServiceView::Credential("c".into()),
            request(),
        ))
        .await
        .unwrap();
    assert_eq!(
        client.sent()[0].1,
        "https://chatgpt.com/backend-api/wham/settings/user",
        "the Codex-API spelling maps onto wham"
    );
}

#[tokio::test]
async fn resources_are_bound_on_creation_and_gated_by_bindings() {
    let config = json!({});
    let secret_main = secret("at-1");
    let other_secret = secret("at-2");
    let metadata = Value::Null;
    let client = ScriptClient::new(vec![
        reply(StatusCode::OK, json!({"file_id": "f1", "bytes": 3})),
        reply(StatusCode::OK, json!({"ok": true})),
        reply(StatusCode::OK, json!({})),
    ]);
    let other = ScriptClient::new(Vec::new());
    let mut second = credential(&other_secret, &metadata);
    second.id = "c2";
    let accounts = [
        account(&config, &secret_main, &metadata, &client),
        CredentialContext {
            provider: provider(&config, None),
            credential: second,
            client: &other,
        },
    ];
    let caller = ScriptCaller::member("m")
        .with_binding(KIND_TASK, "t1", "c", json!({"id": "t1", "title": "mine"}))
        .with_binding(KIND_PLUGIN, "p1", "c", json!({"id": "p1"}));
    let services = Codex.services().unwrap();

    // Create: forwarded with the selected credential, the returned id bound.
    let created = services
        .call(context(
            &accounts,
            &caller,
            ServiceView::Caller,
            service_request(Method::POST, "/backend-api/files", None, "bin"),
        ))
        .await
        .unwrap();
    assert_eq!(created.status, StatusCode::OK);
    let bound = caller.bound();
    let file = bound.iter().find(|b| b.kind == KIND_FILE).unwrap();
    assert_eq!(
        (file.upstream_id.as_str(), file.credential_id.as_str()),
        ("f1", "c")
    );
    assert_eq!(file.summary["bytes"], 3);

    // List: the caller's bindings in the vendor envelope, no upstream call.
    let tasks = body_json(
        services
            .call(context(
                &accounts,
                &caller,
                ServiceView::Pool,
                service_request(Method::GET, "/backend-api/wham/tasks/list", None, ""),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        tasks,
        json!({"tasks": [{"id": "t1", "title": "mine"}], "cursor": null})
    );

    // Item: forwarded with the binding's credential; a foreign id is 404.
    services
        .call(context(
            &accounts,
            &caller,
            ServiceView::Caller,
            service_request(Method::POST, "/backend-api/files/f1/uploaded", None, ""),
        ))
        .await
        .unwrap();
    assert_eq!(
        client.sent()[1].1,
        "https://chatgpt.com/backend-api/files/f1/uploaded"
    );
    let foreign = services
        .call(context(
            &accounts,
            &caller,
            ServiceView::Caller,
            service_request(Method::GET, "/backend-api/wham/tasks/t9", None, ""),
        ))
        .await
        .unwrap();
    assert_eq!(foreign.status, StatusCode::NOT_FOUND);
    assert_eq!(body_json(foreign).await, json!({"detail": "Not found"}));
    assert_eq!(
        client.sent().len(),
        2,
        "the foreign id never reaches upstream"
    );

    // Delete: forwarded, then the binding is gone.
    services
        .call(context(
            &accounts,
            &caller,
            ServiceView::Caller,
            service_request(
                Method::POST,
                "/backend-api/ps/plugins/p1/uninstall",
                None,
                "",
            ),
        ))
        .await
        .unwrap();
    assert!(!caller.bound().iter().any(|b| b.kind == KIND_PLUGIN));
    assert!(
        other.sent().is_empty(),
        "the second credential was never used"
    );
}

#[tokio::test]
async fn item_routes_use_the_credential_named_by_the_binding() {
    let config = json!({});
    let secret_main = secret("at-1");
    let other_secret = secret("at-2");
    let metadata = Value::Null;
    let client = ScriptClient::new(Vec::new());
    let other = ScriptClient::new(vec![reply(StatusCode::OK, json!({}))]);
    let mut second = credential(&other_secret, &metadata);
    second.id = "c2";
    let accounts = [
        account(&config, &secret_main, &metadata, &client),
        CredentialContext {
            provider: provider(&config, None),
            credential: second,
            client: &other,
        },
    ];
    let caller = ScriptCaller::admin("a").with_binding(KIND_TASK, "t2", "c2", json!({"id": "t2"}));
    Codex
        .services()
        .unwrap()
        .call(context(
            &accounts,
            &caller,
            ServiceView::Pool,
            service_request(Method::GET, "/backend-api/wham/tasks/t2", None, ""),
        ))
        .await
        .unwrap();
    assert!(client.sent().is_empty());
    assert_eq!(other.sent()[0].2["authorization"], "Bearer at-2");
}

#[tokio::test]
async fn restricted_telemetry_and_unknown_routes_depend_on_the_view() {
    let config = json!({});
    let secret = secret("at-1");
    let metadata = Value::Null;
    let client = ScriptClient::new(vec![
        reply(StatusCode::OK, json!({})),
        reply(StatusCode::OK, json!({})),
        reply(StatusCode::OK, json!({})),
    ]);
    let accounts = [account(&config, &secret, &metadata, &client)];
    let member = ScriptCaller::member("m");
    let services = Codex.services().unwrap();
    let routes = [
        (
            Method::POST,
            "/backend-api/wham/accounts/send_add_credits_nudge_email",
            StatusCode::FORBIDDEN,
        ),
        (
            Method::POST,
            "/backend-api/codex/analytics-events/events",
            StatusCode::OK,
        ),
        (
            Method::GET,
            "/backend-api/wham/brand-new/endpoint",
            StatusCode::NOT_FOUND,
        ),
    ];
    for (method, path, expected) in &routes {
        for view in [ServiceView::Caller, ServiceView::Pool] {
            let response = services
                .call(context(
                    &accounts,
                    &member,
                    view,
                    service_request(method.clone(), path, None, "{}"),
                ))
                .await
                .unwrap();
            assert_eq!(response.status, *expected, "{path}");
        }
    }
    assert!(client.sent().is_empty(), "nothing left the gateway");
    for (method, path, _) in &routes {
        services
            .call(context(
                &accounts,
                &member,
                ServiceView::Credential("c".into()),
                service_request(method.clone(), path, None, "{}"),
            ))
            .await
            .unwrap();
    }
    assert_eq!(client.sent().len(), 3, "the Credential view forwards them");

    let error = services
        .call(context(
            &accounts,
            &member,
            ServiceView::Credential("c".into()),
            service_request(Method::POST, "/v1/responses", None, "{}"),
        ))
        .await
        .unwrap_err();
    assert!(matches!(error, ChannelError::UnsupportedService));
}

#[tokio::test]
async fn remote_control_is_credential_only() {
    let config = json!({});
    let secret = secret("at-1");
    let metadata = Value::Null;
    let client = ScriptClient::new(Vec::new());
    let accounts = [account(&config, &secret, &metadata, &client)];
    let admin = ScriptCaller::admin("a");
    let services = Codex.services().unwrap();
    let path = "/backend-api/wham/remote/control/server";
    let response = services
        .call(context(
            &accounts,
            &admin,
            ServiceView::Pool,
            service_request(Method::GET, path, None, ""),
        ))
        .await
        .unwrap();
    assert_eq!(response.status, StatusCode::FORBIDDEN);
    let response = services
        .call(context(
            &accounts,
            &admin,
            ServiceView::Credential("c".into()),
            service_request(Method::GET, path, None, ""),
        ))
        .await
        .unwrap();
    assert_eq!(response.status, StatusCode::UPGRADE_REQUIRED);
    assert_eq!(response.headers["upgrade"], "websocket");

    let handshake = |view| ServiceContext {
        account: accounts[0],
        accounts: &accounts,
        caller: &admin,
        view,
        request: WireRequest {
            method: Method::GET,
            path: path.into(),
            query: None,
            headers: HeaderMap::new(),
            body: (),
        },
    };
    let rejected = services
        .connect(handshake(ServiceView::Pool))
        .await
        .unwrap();
    assert!(
        matches!(rejected, UpstreamConnection::Rejected(r) if r.status == StatusCode::FORBIDDEN)
    );
    assert!(client.sent().is_empty());
    let connection = services
        .connect(handshake(ServiceView::Credential("c".into())))
        .await
        .unwrap();
    assert!(matches!(connection, UpstreamConnection::Rejected(_)));
    let sent = client.sent();
    assert_eq!(
        sent[0].0,
        Method::CONNECT,
        "recorded by the scripted connect"
    );
    assert_eq!(
        sent[0].1,
        "wss://chatgpt.com/backend-api/wham/remote/control/server"
    );
    assert_eq!(sent[0].2["authorization"], "Bearer at-1");
}

// ----------------------------------------------------------- magic cache

const MAGIC_AUTO: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH";
const MAGIC_1H: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY";
const MAGIC_PREFIX: &str = "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_";

#[test]
fn magic_cache_strings_shape_responses_bodies_only_when_enabled() {
    let secret = secret("at-1");
    let shaped = |config: &Value, operation: Operation, body: Vec<u8>| -> Vec<u8> {
        let request = Codex
            .prepare(PrepareContext {
                provider: provider(config, None),
                credential: credential(&secret, &Value::Null),
                operation: OperationKey {
                    operation,
                    dialect: Dialect::OpenAi,
                },
                request: WireRequest {
                    method: Method::POST,
                    path: "/v1/responses".into(),
                    query: None,
                    headers: HeaderMap::new(),
                    body: HttpBody::Bytes(Bytes::from(body)),
                },
                endpoint_override: None,
            })
            .unwrap();
        let HttpBody::Bytes(bytes) = request.into_body() else {
            panic!("buffered");
        };
        bytes.to_vec()
    };
    let body = |instruction_token: &str, user_token: &str| {
        json!({
            "model": "gpt-5.3-codex",
            "instructions": format!("rules {instruction_token}"),
            "input": [
                {"type": "message", "role": "user", "content": format!("hi {user_token}")},
                {"type": "message", "role": "user", "content": [
                    {"type": "input_text", "text": "pinned", "prompt_cache_breakpoint": {"mode": "explicit"}}
                ]}
            ]
        })
    };
    let enabled = json!({"enable_openai_magic_cache": true});
    let disabled = json!({});

    for operation in [
        Operation::GenerateContent,
        Operation::StreamGenerateContent,
        Operation::CompactContent,
    ] {
        let bytes = shaped(
            &enabled,
            operation,
            body(MAGIC_1H, MAGIC_AUTO).to_string().into_bytes(),
        );
        assert!(!String::from_utf8_lossy(&bytes).contains(MAGIC_PREFIX));
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["instructions"], "rules ");
        let input = value["input"].as_array().unwrap();
        assert_eq!(input.len(), 3, "{operation:?}: anchor prepended");
        assert_eq!(
            input[0],
            json!({"type": "message", "role": "developer", "content": [
                {"type": "input_text", "text": " ", "prompt_cache_breakpoint": {"mode": "explicit"}}
            ]})
        );
        assert_eq!(
            input[1]["content"],
            json!([{"type": "input_text", "text": "hi ", "prompt_cache_breakpoint": {"mode": "explicit"}}])
        );
        assert_eq!(input[2], body("", "")["input"][1]);
    }

    let with_tokens = shaped(
        &disabled,
        Operation::GenerateContent,
        body(MAGIC_1H, MAGIC_AUTO).to_string().into_bytes(),
    );
    assert!(!String::from_utf8_lossy(&with_tokens).contains(MAGIC_PREFIX));
    assert_eq!(
        with_tokens,
        serde_json::to_vec(&body("", "")).unwrap(),
        "disabled: tokens stripped, the client's breakpoint and everything else untouched"
    );
    let plain = br#"{"model":"gpt-5.3-codex",  "input":"x"}"#.to_vec();
    assert_eq!(
        shaped(&enabled, Operation::GenerateContent, plain.clone()),
        plain,
        "no token, no rewrite"
    );
    let other = format!(r#"{{"input":"{MAGIC_AUTO}"}}"#).into_bytes();
    assert_eq!(
        shaped(&enabled, Operation::SummarizeMemory, other.clone()),
        other,
        "only Responses operations are shaped"
    );
}
