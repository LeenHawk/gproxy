#![cfg(feature = "codex")]

mod support;

use base64::Engine;
use gproxy_channel::{
    BaseChannel, ChannelError, OutboundClient,
    channel::{
        AuthorizationRequest, CallerUsage, CallerUsageWindow, ChannelState, CredentialContext,
        CredentialView, DevicePoll, LoginContext, OperationContext, PrepareContext, ProviderView,
        QuotaHeaderContext, QuotaValue, ResponseView, ServiceContext, ServiceView, UsageContext,
        UsageFrame, UsageStreamContext, UsageStreamEnd, UsageTransport,
    },
    channels::codex::{CLI_VERSION, Codex, DEFAULT_CLIENT_ID, KIND_FILE, KIND_PLUGIN, KIND_TASK},
};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    capability::{
        CapabilityError, CapabilityFuture, CapabilityLimits, CasResult, StateEntry, StateWrite,
        UpstreamConnection, Version,
    },
    connection::{Bytes, StreamFraming},
};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime},
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

type Entry = (Bytes, Version, Option<SystemTime>);

/// A tiny CAS store: expired entries read as absent, versions never repeat.
#[derive(Default)]
struct MemoryState {
    entries: Mutex<HashMap<String, Entry>>,
    counter: AtomicU64,
}
impl MemoryState {
    fn text(&self, key: &str) -> Option<String> {
        self.entries
            .lock()
            .unwrap()
            .get(key)
            .map(|(payload, _, _)| String::from_utf8(payload.to_vec()).unwrap())
    }
}
impl ChannelState for MemoryState {
    fn get<'a>(
        &'a self,
        key: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>> {
        Box::pin(async move {
            let entries = self.entries.lock().unwrap();
            Ok(entries
                .get(key)
                .filter(|(_, _, expires)| expires.is_none_or(|at| at > SystemTime::now()))
                .map(|(payload, version, expires_at)| StateEntry {
                    payload: payload.clone(),
                    version: version.clone(),
                    expires_at: *expires_at,
                }))
        })
    }
    fn compare_exchange<'a>(
        &'a self,
        key: &'a str,
        expected: Option<Version>,
        replacement: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>> {
        Box::pin(async move {
            let mut entries = self.entries.lock().unwrap();
            let current = entries.get(key).map(|(_, version, _)| version.clone());
            if current != expected {
                return Ok(CasResult::Conflict);
            }
            match replacement {
                Some(write) => {
                    let version = Version::from_bytes(
                        self.counter
                            .fetch_add(1, Ordering::SeqCst)
                            .to_be_bytes()
                            .to_vec(),
                    );
                    entries.insert(
                        key.to_owned(),
                        (write.payload, version.clone(), write.expires_at),
                    );
                    Ok(CasResult::Applied(Some(version)))
                }
                None => {
                    entries.remove(key);
                    Ok(CasResult::Applied(None))
                }
            }
        })
    }
    fn limits(&self) -> CapabilityLimits {
        CapabilityLimits {
            operation_total: Duration::from_secs(600),
            stream_idle: Duration::from_secs(60),
            read_bytes: 0,
            write_bytes: 0,
            ws_frame_bytes: 0,
        }
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
    assert_eq!(config.backend, Backend::Reqwest);
    assert!(
        config.emulation.is_none(),
        "plain reqwest without an emulation profile"
    );
    assert!(
        !(config.gzip || config.brotli || config.deflate || config.zstd),
        "response decompression remains disabled"
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
                operation: Operation::CreateEmbedding,
                dialect: Dialect::OpenAi,
            },
            request: WireRequest {
                method: Method::GET,
                path: "/v1/embeddings".into(),
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
    assert_eq!(h["oai-product-sku"], "codex");
    assert_eq!(h.get_all("authorization").iter().count(), 1);
}

#[tokio::test]
async fn plugin_product_and_extensions_survive_header_filtering() {
    let config = json!({"allowed_headers": []});
    let secret = secret("at-1");
    let metadata = json!({});
    let response = json!({"plugins":[{"id":"p1","extensions":{"future":true}}]});
    let client = ScriptClient::new(vec![reply(StatusCode::OK, response.clone())]);
    let accounts = [account(&config, &secret, &metadata, &client)];
    let admin = ScriptCaller::admin("a");
    let mut request = service_request(
        Method::GET,
        "/ps/plugins/installed",
        Some("includeExtensions=true&scope=all"),
        "",
    );
    request.headers.insert(
        "oai-product-sku",
        HeaderValue::from_static("custom-product"),
    );
    let result = Codex
        .services()
        .unwrap()
        .call(context(
            &accounts,
            &admin,
            ServiceView::Credential("c".into()),
            request,
        ))
        .await
        .unwrap();
    assert_eq!(body_json(result).await, response);
    let sent = client.sent();
    assert_eq!(
        sent[0].1,
        "https://chatgpt.com/backend-api/ps/plugins/installed?includeExtensions=true&scope=all"
    );
    assert_eq!(sent[0].2["oai-product-sku"], "custom-product");
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
    let mut expected = body("", "");
    expected["stream"] = json!(true);
    expected["store"] = json!(false);
    assert_eq!(
        serde_json::from_slice::<Value>(&with_tokens).unwrap(),
        expected,
        "disabled magic cache preserves client breakpoints while backend shaping still applies"
    );
    let plain = br#"{"model":"gpt-5.3-codex",  "input":"x"}"#.to_vec();
    assert_eq!(
        serde_json::from_slice::<Value>(&shaped(&enabled, Operation::GenerateContent, plain))
            .unwrap(),
        json!({"model":"gpt-5.3-codex","stream":true,"store":false,"input":[{"type":"message","role":"user","content":"x"}]}),
        "backend shaping is independent of magic cache markers"
    );
    let other = format!(r#"{{"input":"{MAGIC_AUTO}"}}"#).into_bytes();
    assert_eq!(
        shaped(&enabled, Operation::SummarizeMemory, other.clone()),
        other,
        "only Responses operations are shaped"
    );
}

// ------------------------------------------------------------ identity

/// The identity headers `identity.rs` synthesizes for a non-CLI request.
const IDENTITY_HEADERS: &[&str] = &[
    "version",
    "session-id",
    "thread-id",
    "x-client-request-id",
    "x-codex-installation-id",
    "x-codex-window-id",
    "x-codex-turn-metadata",
    "x-codex-routing-hint",
];

fn reply_with_turn_state(token: Option<&'static str>) -> WireResponse {
    let mut headers = HeaderMap::new();
    if let Some(token) = token {
        headers.insert("x-codex-turn-state", HeaderValue::from_static(token));
    }
    WireResponse {
        status: StatusCode::OK,
        headers,
        body: HttpBody::Bytes(Bytes::from_static(b"{}")),
    }
}

/// A Responses call the way core issues it for a converted client.
async fn responses_call(
    config: &Value,
    client: &Arc<ScriptClient>,
    state: &Arc<MemoryState>,
    operation: Operation,
    headers: HeaderMap,
    body: Value,
) -> WireResponse {
    let secret = secret("at-1");
    let metadata = json!({"chatgpt_account_id": "acct-1"});
    let ctx = OperationContext {
        provider: provider(config, None),
        credential: credential(&secret, &metadata),
        dialect: Dialect::OpenAi,
        request: WireRequest {
            method: Method::POST,
            path: "/v1/responses".into(),
            query: None,
            headers,
            body: HttpBody::Bytes(Bytes::from(body.to_string())),
        },
        client: client.clone(),
        state: state.clone(),
        instance_id: Arc::from("i"),
        endpoint_override: None,
    };
    match operation {
        Operation::GenerateContent => Codex.generate_content(ctx).await.unwrap(),
        Operation::StreamGenerateContent => Codex.stream_generate_content(ctx).await.unwrap(),
        Operation::CompactContent => Codex.compact_content(ctx).await.unwrap(),
        other => panic!("{other:?}"),
    }
}

fn header(sent: &Sent, name: &str) -> Option<String> {
    sent.2.get(name).map(|v| v.to_str().unwrap().to_owned())
}

fn turn_metadata(sent: &Sent) -> Value {
    serde_json::from_str(&header(sent, "x-codex-turn-metadata").expect("turn metadata")).unwrap()
}

fn prompt(key: &str, input: Value) -> Value {
    json!({"model": "gpt-5.3-codex", "prompt_cache_key": key, "stream": true, "input": input})
}

#[tokio::test]
async fn converted_bodies_get_the_cli_session_window_and_turn_identity() {
    let config = json!({});
    let state = Arc::new(MemoryState::default());
    let client = Arc::new(ScriptClient::new(
        (0..5).map(|_| reply(StatusCode::OK, json!({}))).collect(),
    ));
    let user = |text: &str| json!({"type": "message", "role": "user", "content": text});
    let call = |operation: Operation, body: Value| {
        responses_call(&config, &client, &state, operation, HeaderMap::new(), body)
    };

    call(
        Operation::StreamGenerateContent,
        prompt("conv-1", json!([user("first prompt")])),
    )
    .await;
    let first = &client.sent()[0];
    for name in IDENTITY_HEADERS {
        assert!(first.2.contains_key(*name), "synthesized: {name}");
    }
    assert_eq!(header(first, "version").unwrap(), CLI_VERSION);
    assert_eq!(header(first, "accept").unwrap(), "text/event-stream");
    let session = header(first, "session-id").unwrap();
    let thread = header(first, "thread-id").unwrap();
    assert_eq!(session, thread, "a plain thread shares the session id");
    assert_eq!(
        header(first, "x-client-request-id").unwrap(),
        thread,
        "the CLI's request id is its thread id"
    );
    assert_eq!(
        header(first, "x-codex-window-id").unwrap(),
        format!("{thread}:0")
    );
    assert_eq!(
        header(first, "x-codex-routing-hint").unwrap(),
        "model=gpt-5.3-codex"
    );
    assert!(header(first, "x-codex-turn-state").is_none());
    for absent in [
        "x-openai-subagent",
        "x-codex-parent-thread-id",
        "x-oai-attestation",
        "x-codex-beta-features",
    ] {
        assert!(first.2.get(absent).is_none(), "never synthesized: {absent}");
    }
    let metadata = turn_metadata(first);
    let installation = header(first, "x-codex-installation-id").unwrap();
    assert_eq!(metadata["installation_id"], installation);
    assert_eq!(metadata["session_id"], session);
    assert_eq!(metadata["thread_id"], thread);
    assert_eq!(metadata["agent_name"], "/root");
    assert_eq!(metadata["window_id"], format!("{thread}:0"));
    assert_eq!(metadata["window_number"], 0);
    assert!(metadata["context_window_id"].is_string());
    assert_eq!(metadata["request_kind"], "turn");
    assert_eq!(metadata["thread_source"], "user");
    assert!(metadata.get("compaction").is_none());
    assert!(metadata.get("workspaces").is_none());
    assert!(metadata.get("sandbox").is_none());
    let turn_a = metadata["turn_id"].as_str().unwrap().to_owned();
    assert_eq!(
        state.text("installation_id").as_deref(),
        Some(installation.as_str()),
        "the installation id is persisted"
    );

    // The tool loop of the same prompt keeps the turn.
    call(
        Operation::GenerateContent,
        prompt(
            "conv-1",
            json!([
                user("first prompt"),
                {"type": "function_call", "call_id": "c1", "name": "ls", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "c1", "output": "ok"}
            ]),
        ),
    )
    .await;
    let second = &client.sent()[1];
    assert_eq!(header(second, "session-id").unwrap(), session);
    assert_eq!(
        header(second, "x-codex-installation-id").unwrap(),
        installation
    );
    assert_eq!(turn_metadata(second)["turn_id"], turn_a);
    assert!(
        header(second, "accept").is_none(),
        "a buffered generate does not ask for an event stream"
    );

    // A new user message is a new turn in the same thread.
    call(
        Operation::GenerateContent,
        prompt(
            "conv-1",
            json!([
                user("first prompt"),
                {"type": "message", "role": "assistant", "content": "done"},
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "second prompt"}]}
            ]),
        ),
    )
    .await;
    let third = &client.sent()[2];
    assert_eq!(header(third, "session-id").unwrap(), session);
    assert_ne!(turn_metadata(third)["turn_id"], turn_a);

    // Another session key is another thread; the installation stays.
    call(
        Operation::GenerateContent,
        prompt("conv-2", json!([user("first prompt")])),
    )
    .await;
    let fourth = &client.sent()[3];
    assert_ne!(header(fourth, "session-id").unwrap(), session);
    assert_eq!(
        header(fourth, "x-codex-installation-id").unwrap(),
        installation
    );

    // Without a cache key the first user text identifies the conversation.
    call(
        Operation::GenerateContent,
        json!({"model": "gpt-5.3-codex", "service_tier": "fast", "input": [user("first prompt")]}),
    )
    .await;
    let fifth = &client.sent()[4];
    assert_ne!(header(fifth, "session-id").unwrap(), session);
    assert_eq!(
        header(fifth, "x-codex-routing-hint").unwrap(),
        "model=gpt-5.3-codex;tier=fast"
    );
}

#[tokio::test]
async fn client_supplied_identity_is_kept_verbatim() {
    let config = json!({});
    let state = Arc::new(MemoryState::default());
    let client = Arc::new(ScriptClient::new(vec![
        reply(StatusCode::OK, json!({})),
        reply(StatusCode::OK, json!({})),
    ]));
    let body = prompt("conv-1", json!([{"role": "user", "content": "hi"}]));

    // A real CLI session: its own turn metadata, nothing synthesized.
    let mut headers = HeaderMap::new();
    headers.insert("session-id", HeaderValue::from_static("cli-session"));
    headers.insert("thread-id", HeaderValue::from_static("cli-thread"));
    headers.insert(
        "x-codex-turn-metadata",
        HeaderValue::from_static("{\"request_kind\":\"turn\"}"),
    );
    responses_call(
        &config,
        &client,
        &state,
        Operation::StreamGenerateContent,
        headers,
        body.clone(),
    )
    .await;
    let sent = &client.sent()[0];
    assert_eq!(header(sent, "session-id").unwrap(), "cli-session");
    assert_eq!(header(sent, "thread-id").unwrap(), "cli-thread");
    assert_eq!(
        header(sent, "x-codex-turn-metadata").unwrap(),
        "{\"request_kind\":\"turn\"}"
    );
    for name in ["x-codex-window-id", "x-codex-installation-id", "version"] {
        assert!(sent.2.get(name).is_none(), "{name}: a CLI manages itself");
    }

    // Only a session id: kept, and the rest is derived around it.
    let mut headers = HeaderMap::new();
    headers.insert("session-id", HeaderValue::from_static("client-session"));
    responses_call(
        &config,
        &client,
        &state,
        Operation::StreamGenerateContent,
        headers,
        body,
    )
    .await;
    let sent = &client.sent()[1];
    assert_eq!(header(sent, "session-id").unwrap(), "client-session");
    assert_eq!(header(sent, "thread-id").unwrap(), "client-session");
    let metadata = turn_metadata(sent);
    assert_eq!(metadata["session_id"], "client-session");
    assert_eq!(metadata["window_id"], "client-session:0");
}

#[tokio::test]
async fn turn_state_is_replayed_within_a_turn_only() {
    let config = json!({});
    let state = Arc::new(MemoryState::default());
    let client = Arc::new(ScriptClient::new(vec![
        reply_with_turn_state(Some("tok-1")),
        reply_with_turn_state(Some("tok-1")),
        reply_with_turn_state(None),
        reply_with_turn_state(Some("tok-2")),
    ]));
    let user = |text: &str| json!({"role": "user", "content": text});
    let call = |body: Value| {
        responses_call(
            &config,
            &client,
            &state,
            Operation::StreamGenerateContent,
            HeaderMap::new(),
            body,
        )
    };

    call(prompt("conv-1", json!([user("first")]))).await;
    let first = &client.sent()[0];
    assert!(header(first, "x-codex-turn-state").is_none());
    let thread = header(first, "thread-id").unwrap();
    let turn = turn_metadata(first)["turn_id"].as_str().unwrap().to_owned();
    assert_eq!(
        state
            .text(&format!("thread:{thread}:turn:{turn}:state"))
            .as_deref(),
        Some("tok-1")
    );

    // Same turn, a tool output appended: the token comes back.
    call(prompt(
        "conv-1",
        json!([user("first"), {"type": "function_call_output", "call_id": "c", "output": "o"}]),
    ))
    .await;
    assert_eq!(
        header(&client.sent()[1], "x-codex-turn-state").unwrap(),
        "tok-1"
    );

    // A new turn starts without one.
    call(prompt("conv-1", json!([user("first"), user("second")]))).await;
    let third = &client.sent()[2];
    assert!(header(third, "x-codex-turn-state").is_none());
    assert_ne!(turn_metadata(third)["turn_id"], turn);

    // A client-managed session is never read or written.
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-codex-turn-metadata",
        HeaderValue::from_static("{\"request_kind\":\"turn\"}"),
    );
    let before = state.entries.lock().unwrap().len();
    responses_call(
        &config,
        &client,
        &state,
        Operation::StreamGenerateContent,
        headers,
        prompt("conv-1", json!([user("first")])),
    )
    .await;
    assert_eq!(state.entries.lock().unwrap().len(), before);
}

#[tokio::test]
async fn compaction_opens_the_next_window() {
    let config = json!({});
    let state = Arc::new(MemoryState::default());
    let client = Arc::new(ScriptClient::new(vec![
        reply(StatusCode::OK, json!({})),
        reply(StatusCode::OK, json!({"output": []})),
        reply(StatusCode::OK, json!({})),
        reply(StatusCode::BAD_GATEWAY, json!({})),
        reply(StatusCode::OK, json!({})),
    ]));
    let body = prompt("conv-1", json!([{"role": "user", "content": "hi"}]));
    let call = |operation: Operation| {
        responses_call(
            &config,
            &client,
            &state,
            operation,
            HeaderMap::new(),
            body.clone(),
        )
    };

    call(Operation::GenerateContent).await;
    let first = &client.sent()[0];
    let thread = header(first, "thread-id").unwrap();
    let window_0 = turn_metadata(first)["context_window_id"].clone();

    call(Operation::CompactContent).await;
    let compact = &client.sent()[1];
    assert!(compact.1.ends_with("/responses/compact"));
    assert_eq!(
        header(compact, "x-codex-window-id").unwrap(),
        format!("{thread}:0")
    );
    let metadata = turn_metadata(compact);
    assert_eq!(metadata["request_kind"], "compaction");
    assert_eq!(
        metadata["compaction"],
        json!({
            "trigger": "manual",
            "reason": "user_requested",
            "implementation": "responses_compact",
            "phase": "standalone_turn",
            "strategy": "memento"
        })
    );
    assert_eq!(
        state.text(&format!("thread:{thread}:window")).as_deref(),
        Some("1")
    );

    call(Operation::GenerateContent).await;
    let third = &client.sent()[2];
    assert_eq!(
        header(third, "x-codex-window-id").unwrap(),
        format!("{thread}:1")
    );
    let metadata = turn_metadata(third);
    assert_eq!(metadata["window_number"], 1);
    assert_ne!(metadata["context_window_id"], window_0);

    // A failed compaction leaves the window alone.
    call(Operation::CompactContent).await;
    call(Operation::GenerateContent).await;
    assert_eq!(
        header(&client.sent()[4], "x-codex-window-id").unwrap(),
        format!("{thread}:1")
    );
}

#[tokio::test]
async fn identity_synthesis_can_be_switched_off() {
    let config = json!({"synthesize_cli_identity": false});
    let state = Arc::new(MemoryState::default());
    let client = Arc::new(ScriptClient::new(vec![reply_with_turn_state(Some("tok"))]));
    let mut headers = HeaderMap::new();
    headers.insert("session-id", HeaderValue::from_static("mine"));
    responses_call(
        &config,
        &client,
        &state,
        Operation::StreamGenerateContent,
        headers,
        prompt("conv-1", json!([{"role": "user", "content": "hi"}])),
    )
    .await;
    let sent = &client.sent()[0];
    assert_eq!(
        header(sent, "session-id").unwrap(),
        "mine",
        "passthrough is unaffected"
    );
    for name in IDENTITY_HEADERS
        .iter()
        .filter(|name| **name != "session-id")
    {
        assert!(sent.2.get(*name).is_none(), "{name}");
    }
    assert!(sent.2.get("accept").is_none());
    assert!(
        state.entries.lock().unwrap().is_empty(),
        "nothing remembered"
    );
}

fn prepared_body(config: Value, operation: Operation, body: Value) -> Value {
    let request = prepare_body_bytes(
        config,
        operation,
        HeaderMap::new(),
        Bytes::from(body.to_string()),
    );
    let HttpBody::Bytes(bytes) = request.into_body() else {
        panic!("buffered")
    };
    serde_json::from_slice(&bytes).unwrap()
}

fn prepare_body_bytes(
    config: Value,
    operation: Operation,
    headers: HeaderMap,
    body: Bytes,
) -> http::Request<HttpBody> {
    Codex
        .prepare(PrepareContext {
            provider: provider(&config, None),
            credential: credential(&secret("at"), &Value::Null),
            operation: OperationKey {
                operation,
                dialect: Dialect::OpenAi,
            },
            request: WireRequest {
                method: Method::POST,
                path: "/ignored".into(),
                query: None,
                headers,
                body: HttpBody::Bytes(body),
            },
            endpoint_override: None,
        })
        .unwrap()
}

#[test]
fn shapes_converted_responses_and_avoids_cli_tool_name_collisions() {
    let input = json!({
        "model":"gpt-5.4", "instructions":"first", "stream":false, "store":true,
        "max_output_tokens":100, "metadata":{}, "prompt_cache_options":{"mode":"implicit"},
        "temperature":1,"top_p":1,"top_logprobs":2,"safety_identifier":"x","truncation":"auto",
        "input":[
            {"role":"system","content":"policy"},
            {"type":"reasoning","id":"r1","summary":[],"status":"completed","future_item":1},
            {"type":"shell_call","id":"shell_old","call_id":"call_old","action":{"commands":["pwd"]}},
            {"type":"local_shell_call_output","id":"out1","call_id":"call-real","output":"ok","status":"completed"}
        ],
        "tools":[
            {"type":"function","name":"shell_command","strict":false,"parameters":{"type":"object"}},
            {"type":"shell"}, {"type":"apply_patch"}, {"type":"tool_search","execution":"client"}
        ], "tool_choice":{"type":"shell"}, "future_request":true
    });
    let value = prepared_body(json!({}), Operation::StreamGenerateContent, input);
    assert_eq!(value["stream"], true);
    assert_eq!(value["store"], false);
    for field in [
        "max_output_tokens",
        "metadata",
        "prompt_cache_options",
        "temperature",
        "top_p",
        "top_logprobs",
        "safety_identifier",
        "truncation",
    ] {
        assert!(value.get(field).is_none(), "{field}");
    }
    assert_eq!(value["instructions"], "first\npolicy");
    assert!(value["input"][0].get("status").is_none());
    assert_eq!(value["input"][0]["future_item"], 1);
    assert_eq!(value["input"][1]["type"], "function_call");
    assert_eq!(value["input"][1]["name"], "shell_command_1");
    assert!(value["input"][1]["id"].as_str().unwrap().starts_with("fc_"));
    assert_eq!(value["input"][1]["call_id"], "call_old");
    assert_eq!(value["input"][2]["call_id"], "call-real");
    assert_eq!(value["tools"][0]["name"], "shell_command");
    assert_eq!(value["tools"][1]["name"], "shell_command_1");
    assert_eq!(value["tools"][2]["type"], "custom");
    assert!(value["tools"][3].get("parameters").is_some());
    assert_eq!(
        value["tool_choice"],
        json!({"type":"function","name":"shell_command_1"})
    );
    assert_eq!(value["future_request"], true);
    let text = prepared_body(
        json!({}),
        Operation::GenerateContent,
        json!({"input":"hello"}),
    );
    assert_eq!(
        text["input"],
        json!([{"type":"message","role":"user","content":"hello"}])
    );
}

#[test]
fn cli_shaped_requests_and_client_managed_sessions_are_identity_transforms() {
    let cli = json!({"model":"gpt-5.4","stream":true,"store":false,"instructions":"policy",
        "input":[{"role":"user","content":"hello"}],
        "tools":[{"type":"function","name":"shell_command","parameters":{"type":"object"},"strict":false}],
        "include":["reasoning.encrypted_content"]});
    assert_eq!(
        prepared_body(json!({}), Operation::StreamGenerateContent, cli.clone()),
        cli
    );
    let mut headers = HeaderMap::new();
    headers.insert("x-codex-turn-metadata", HeaderValue::from_static("{}"));
    let bytes =
        Bytes::from_static(br#"{"future_cli_field": true, "tools":[{"type":"future_tool"}]}"#);
    let request = prepare_body_bytes(
        json!({}),
        Operation::StreamGenerateContent,
        headers,
        bytes.clone(),
    );
    let HttpBody::Bytes(actual) = request.into_body() else {
        panic!("buffered")
    };
    assert_eq!(actual, bytes);
}

#[test]
fn codex_image_endpoints_shape_json_and_binary_multipart() {
    for operation in [Operation::CreateImage, Operation::EditImage] {
        assert_eq!(
            Codex.native_dialects(provider(&json!({}), None), operation),
            vec![Dialect::OpenAi]
        );
    }
    let create = prepare_body_bytes(json!({}), Operation::CreateImage, HeaderMap::new(), Bytes::from(json!({
        "model":"gpt-image-1","prompt":"draw","moderation":"low","future_image_option":{"x":1}
    }).to_string()));
    assert!(create.uri().path().ends_with("/images/generations"));
    assert_eq!(create.headers()["content-type"], "application/json");
    let HttpBody::Bytes(bytes) = create.into_body() else {
        panic!("buffered")
    };
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["moderation"], "low");
    assert_eq!(value["future_image_option"]["x"], 1);
    let edited = prepared_body(
        json!({}),
        Operation::EditImage,
        json!({"model":"gpt-image-1","prompt":"edit","images":[{"image_url":"https://example.com/a.png"}],"mask":{"image_url":"mask"},"input_fidelity":"high"}),
    );
    assert_eq!(edited["mask"], json!({"image_url":"mask"}));
    assert_eq!(edited["input_fidelity"], "high");
    let image = b"\x89PNG\0\xff--edge-not-a-boundary\r\n";
    let mut body = b"--edge\r\nContent-Disposition: form-data; name=\"prompt\"\r\n\r\nedit\r\n--edge\r\nContent-Disposition: form-data; name=\"image[]\"; filename=\"x.png\"\r\nContent-Type: image/png\r\n\r\n".to_vec();
    body.extend_from_slice(image);
    body.extend_from_slice(b"\r\n--edge--\r\n");
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        HeaderValue::from_static("multipart/form-data; boundary=\"edge\""),
    );
    let edit = prepare_body_bytes(json!({}), Operation::EditImage, headers, Bytes::from(body));
    assert!(edit.uri().path().ends_with("/images/edits"));
    assert_eq!(edit.headers()["content-type"], "application/json");
    let HttpBody::Bytes(bytes) = edit.into_body() else {
        panic!("buffered")
    };
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        value["images"][0]["image_url"],
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(image)
        )
    );
}

fn sse_response(text: &str) -> WireResponse {
    let chunks: Vec<_> = text
        .as_bytes()
        .chunks(7)
        .map(|chunk| Ok(Bytes::copy_from_slice(chunk)))
        .collect();
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        HeaderValue::from_static("text/event-stream"),
    );
    WireResponse {
        status: StatusCode::OK,
        headers,
        body: HttpBody::Stream(Box::pin(futures_util::stream::iter(chunks))),
    }
}

#[tokio::test]
async fn converted_stream_restores_native_tools_and_preserves_usage() {
    use futures_util::StreamExt;
    use gproxy_protocol::codec::{CodecLimits, SseDecoder, SseFrame};
    let body = json!({"model":"gpt-5.4","input":[{"type":"shell_call","id":"client_shell","call_id":"history","action":{"commands":["pwd"]}}],
        "tools":[{"type":"shell"},{"type":"function","name":"shell_command","strict":false,"parameters":{"type":"object"}}]});
    let shaped = prepared_body(json!({}), Operation::StreamGenerateContent, body.clone());
    let mapped = shaped["input"][0]["id"].clone();
    let input = [
        json!({"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":mapped,"call_id":"call_new","name":"shell_command_1","arguments":""}}),
        json!({"type":"response.function_call_arguments.delta","output_index":0,"item_id":mapped,"delta":"{\"command\":\"pwd\"}"}),
        json!({"type":"response.completed","response":{"id":"resp_native","output":[],"usage":{"input_tokens":30,"input_tokens_details":{"cached_tokens":10},"output_tokens":7,"output_tokens_details":{"reasoning_tokens":2}}}}),
    ].iter().map(|event| format!("data: {event}\n\n")).collect::<String>();
    let client = Arc::new(ScriptClient::new(vec![sse_response(&input)]));
    let response = responses_call(
        &json!({}),
        &client,
        &Arc::new(MemoryState::default()),
        Operation::StreamGenerateContent,
        HeaderMap::new(),
        body,
    )
    .await;
    let mut observer = Codex
        .usage_stream()
        .unwrap()
        .start(UsageStreamContext {
            operation: OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: Dialect::OpenAi,
            },
            request_body: None,
            status: response.status,
            headers: &response.headers,
            transport: UsageTransport::Http {
                framing: Some(StreamFraming::Sse),
            },
        })
        .unwrap();
    let HttpBody::Stream(mut stream) = response.body else {
        panic!("stream")
    };
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.unwrap();
        observer.observe(UsageFrame::HttpChunk(&chunk)).unwrap();
        bytes.extend_from_slice(&chunk);
    }
    let usage = observer.finish(UsageStreamEnd::Complete).unwrap().unwrap();
    assert_eq!(usage.tokens.input_tokens, Some(20));
    assert_eq!(usage.tokens.cached_input_tokens, Some(10));
    assert_eq!(usage.tokens.output_tokens, Some(7));
    assert_eq!(usage.tokens.reasoning_tokens, Some(2));
    let mut decoder = SseDecoder::new(CodecLimits {
        max_buffer_bytes: 1 << 20,
        max_value_bytes: 1 << 20,
        max_body_bytes: 1 << 20,
        max_line_bytes: 1 << 20,
        max_part_bytes: 0,
        max_parts: 0,
    });
    let events: Vec<Value> = decoder
        .push(&bytes)
        .unwrap()
        .into_iter()
        .filter_map(|frame| match frame {
            SseFrame::Event(event) => Some(serde_json::from_str(&event.data).unwrap()),
            _ => None,
        })
        .collect();
    assert_eq!(events[0]["type"], "response.created");
    assert_eq!(events[0]["response"]["id"], "resp_native");
    let call = &events.last().unwrap()["response"]["output"][0];
    assert_eq!(call["type"], "shell_call");
    assert_eq!(call["id"], "client_shell");
    assert_eq!(call["call_id"], "call_new");
    assert_eq!(call["action"]["commands"], json!(["pwd"]));
    assert_eq!(events[1]["item"], *call);
    assert_eq!(events[2]["item"], *call);
    let sent: Value = serde_json::from_slice(&client.sent()[0].3).unwrap();
    assert_eq!(sent["tools"][0]["name"], "shell_command_1");
    assert_eq!(sent["tools"][1]["name"], "shell_command");
}

#[tokio::test]
async fn real_cli_and_upstream_error_streams_remain_byte_exact() {
    use futures_util::StreamExt;
    let text = ": keepalive\r\ndata: {\"type\":\"future.cli.event\",\"value\":1}\r\n\r\n";
    for cli in [true, false] {
        let mut reply = sse_response(text);
        if !cli {
            reply.status = StatusCode::BAD_REQUEST;
        }
        let client = Arc::new(ScriptClient::new(vec![reply]));
        let mut headers = HeaderMap::new();
        if cli {
            headers.insert("x-codex-turn-metadata", HeaderValue::from_static("{}"));
        }
        let response = responses_call(
            &json!({}),
            &client,
            &Arc::new(MemoryState::default()),
            Operation::StreamGenerateContent,
            headers,
            json!({"input":"hi"}),
        )
        .await;
        let HttpBody::Stream(mut stream) = response.body else {
            panic!("stream")
        };
        let mut actual = Vec::new();
        while let Some(bytes) = stream.next().await {
            actual.extend_from_slice(&bytes.unwrap());
        }
        assert_eq!(actual, text.as_bytes());
    }
}

#[tokio::test]
async fn image_stream_parameters_and_upstream_sse_pass_through() {
    use futures_util::StreamExt;
    for operation in [Operation::CreateImage, Operation::EditImage] {
        let event = if operation == Operation::CreateImage {
            "image_generation.completed"
        } else {
            "image_edit.completed"
        };
        let wire = format!(
            "event: {event}\r\ndata: {{\"type\":\"{event}\",\"b64_json\":\"aW1hZ2U=\"}}\r\n\r\n"
        );
        let client = Arc::new(ScriptClient::new(vec![sse_response(&wire)]));
        let mut body = json!({"model":"gpt-image-2.5","prompt":"blue circle","stream":true,"partial_images":2, "moderation":"low", "output_compression":80, "output_format":"jpeg", "response_format":"b64_json", "style":"natural", "user":"gproxy-image-probe"});
        if operation == Operation::EditImage {
            body["images"] = json!([{"image_url":"data:image/png;base64,aW1hZ2U="}]);
            body["input_fidelity"] = json!("high");
            body["mask"] = json!({"image_url":"data:image/png;base64,bWFzaw=="});
        }
        let config = json!({});
        let secret = secret("at");
        let context = OperationContext {
            provider: provider(&config, None),
            credential: credential(&secret, &Value::Null),
            dialect: Dialect::OpenAi,
            request: WireRequest {
                method: Method::POST,
                path: "/ignored".into(),
                query: None,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::from(body.to_string())),
            },
            client: client.clone(),
            state: Arc::new(MemoryState::default()),
            instance_id: Arc::from("i"),
            endpoint_override: None,
        };
        let response = match operation {
            Operation::CreateImage => Codex.create_image(context).await.unwrap(),
            Operation::EditImage => Codex.edit_image(context).await.unwrap(),
            _ => unreachable!(),
        };
        let sent: Value = serde_json::from_slice(&client.sent()[0].3).unwrap();
        assert_eq!(
            sent, body,
            "image request parameters must survive preparation"
        );
        assert_eq!(response.headers["content-type"], "text/event-stream");
        let HttpBody::Stream(mut stream) = response.body else {
            panic!("stream")
        };
        let mut actual = Vec::new();
        while let Some(chunk) = stream.next().await {
            actual.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(actual, wire.as_bytes());
    }
}

#[test]
fn multipart_image_edit_preserves_stream_parameters() {
    let body = "--edge\r\nContent-Disposition: form-data; name=\"prompt\"\r\n\r\nedit\r\n--edge\r\nContent-Disposition: form-data; name=\"stream\"\r\n\r\ntrue\r\n--edge\r\nContent-Disposition: form-data; name=\"partial_images\"\r\n\r\n2\r\n--edge\r\nContent-Disposition: form-data; name=\"image\"\r\n\r\nhttps://example.com/a.png\r\n--edge--\r\n";
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        HeaderValue::from_static("multipart/form-data; boundary=edge"),
    );
    let request = prepare_body_bytes(json!({}), Operation::EditImage, headers, Bytes::from(body));
    let HttpBody::Bytes(bytes) = request.into_body() else {
        panic!("buffered")
    };
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["stream"], true);
    assert_eq!(value["partial_images"], 2);
}

#[tokio::test]
async fn alpha_search_preserves_search_parameters_and_returns_json() {
    let config = json!({});
    let secret = secret("at");
    let input = json!({"id":"session-1","model":"gpt-6-astra","input":"recent context",
        "commands":{"search_query":[{"q":"OpenAI","domains":["openai.com"]}],"response_length":"short"},
        "settings":{"external_web_access":"live"},"max_output_tokens":2500,"future":true});
    let answer =
        json!({"output":"result","encrypted_output":"opaque","results":[{"future_result":1}]});
    let client = Arc::new(ScriptClient::new(vec![reply(
        StatusCode::OK,
        answer.clone(),
    )]));
    assert_eq!(
        Codex.native_dialects(provider(&config, None), Operation::WebSearch),
        vec![Dialect::OpenAi]
    );
    let response = Codex
        .web_search(OperationContext {
            provider: provider(&config, None),
            credential: credential(&secret, &Value::Null),
            dialect: Dialect::OpenAi,
            request: WireRequest {
                method: Method::POST,
                path: "/v1/alpha/search".into(),
                query: None,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::from(input.to_string())),
            },
            client: client.clone(),
            state: Arc::new(MemoryState::default()),
            instance_id: Arc::from("i"),
            endpoint_override: None,
        })
        .await
        .unwrap();
    assert!(
        client.sent()[0]
            .1
            .ends_with("/backend-api/codex/alpha/search")
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&client.sent()[0].3).unwrap(),
        input
    );
    assert_eq!(body_json(response).await, answer);
}

fn realtime_multipart() -> Bytes {
    Bytes::from_static(b"--rtc\r\nContent-Disposition: form-data; name=\"sdp\"\r\n\r\nv=0\r\ns=offer\r\n\r\n--rtc\r\nContent-Disposition: form-data; name=\"session\"\r\n\r\n{\"type\":\"realtime\",\"model\":\"gpt-realtime\",\"future_session\":true}\r\n--rtc--\r\n")
}

#[tokio::test]
async fn realtime_call_adapts_buffered_and_streamed_multipart_and_keeps_answer() {
    use futures_util::StreamExt;
    for streamed in [false, true] {
        let config = json!({});
        let secret = secret("at");
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            HeaderValue::from_static("multipart/form-data; boundary=rtc"),
        );
        let source = realtime_multipart();
        let body = if streamed {
            HttpBody::Stream(Box::pin(futures_util::stream::iter(
                source
                    .chunks(3)
                    .map(|chunk| Ok(Bytes::copy_from_slice(chunk)))
                    .collect::<Vec<_>>(),
            )))
        } else {
            HttpBody::Bytes(source)
        };
        let mut response_headers = HeaderMap::new();
        response_headers.insert("content-type", HeaderValue::from_static("application/sdp"));
        response_headers.insert(
            "location",
            HeaderValue::from_static("/v1/realtime/calls/rtc_test"),
        );
        let client = Arc::new(ScriptClient::new(vec![WireResponse {
            status: StatusCode::CREATED,
            headers: response_headers.clone(),
            body: HttpBody::Bytes(Bytes::from_static(b"v=0\r\ns=answer\r\n")),
        }]));
        let response = Codex
            .create_realtime_call(OperationContext {
                provider: provider(&config, None),
                credential: credential(&secret, &Value::Null),
                dialect: Dialect::OpenAi,
                request: WireRequest {
                    method: Method::POST,
                    path: "/v1/realtime/calls".into(),
                    query: Some("intent=quicksilver&architecture=avas".into()),
                    headers,
                    body,
                },
                client: client.clone(),
                state: Arc::new(MemoryState::default()),
                instance_id: Arc::from("i"),
                endpoint_override: None,
            })
            .await
            .unwrap();
        let sent = &client.sent()[0];
        assert_eq!(
            sent.1,
            "https://chatgpt.com/backend-api/codex/realtime/calls?intent=quicksilver&architecture=avas"
        );
        assert_eq!(sent.2["content-type"], "application/json");
        assert_eq!(
            serde_json::from_slice::<Value>(&sent.3).unwrap(),
            json!({"sdp":"v=0\r\ns=offer\r\n","session":{"type":"realtime","model":"gpt-realtime","future_session":true}})
        );
        assert_eq!(response.status, StatusCode::CREATED);
        assert_eq!(response.headers, response_headers);
        let bytes = match response.body {
            HttpBody::Bytes(b) => b,
            HttpBody::Stream(mut s) => s.next().await.unwrap().unwrap(),
        };
        assert_eq!(bytes.as_ref(), b"v=0\r\ns=answer\r\n");
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        HeaderValue::from_static("multipart/form-data; boundary=rtc"),
    );
    let prepared = prepare_body_bytes(
        json!({}),
        Operation::CreateRealtimeCall,
        headers,
        realtime_multipart(),
    );
    assert_eq!(prepared.headers()["content-type"], "application/json");
}

#[tokio::test]
async fn realtime_websocket_uses_api_routes_without_responses_identity() {
    let config = json!({"allowed_headers":[]});
    let secret = secret("at");
    for (path, query, endpoint, expected) in [
        (
            "/v1/realtime",
            Some("model=gpt-realtime&intent=quicksilver"),
            None,
            "wss://api.openai.com/v1/realtime?model=gpt-realtime&intent=quicksilver",
        ),
        (
            "/v1/realtime",
            Some("call_id=rtc_test"),
            None,
            "wss://api.openai.com/v1/realtime?call_id=rtc_test",
        ),
        (
            "/v1/live/rtc_test",
            None,
            None,
            "wss://api.openai.com/v1/live/rtc_test",
        ),
        ("/v1/live", None, None, "wss://api.openai.com/v1/live"),
        (
            "/v1/realtime",
            Some("call_id=rtc_test"),
            Some("https://example.test/control?deployment=x"),
            "wss://example.test/control?deployment=x&call_id=rtc_test",
        ),
    ] {
        let client = Arc::new(ScriptClient::new(vec![]));
        let mut headers = HeaderMap::new();
        headers.insert("openai-beta", HeaderValue::from_static("realtime=v1"));
        let _ = Codex
            .connect_realtime(OperationContext {
                provider: provider(&config, None),
                credential: credential(&secret, &Value::Null),
                dialect: Dialect::OpenAi,
                request: WireRequest {
                    method: Method::GET,
                    path: path.into(),
                    query: query.map(str::to_owned),
                    headers,
                    body: (),
                },
                client: client.clone(),
                state: Arc::new(MemoryState::default()),
                instance_id: Arc::from("i"),
                endpoint_override: endpoint,
            })
            .await
            .unwrap();
        let sent = &client.sent()[0];
        assert_eq!(sent.1, expected);
        assert_eq!(sent.2["authorization"], "Bearer at");
        assert_eq!(sent.2["openai-beta"], "realtime=v1");
        assert!(!sent.2.contains_key("x-codex-turn-metadata"));
        assert!(!sent.2.contains_key("x-codex-turn-state"));
    }
}

#[test]
fn live_endpoint_override_keeps_call_id_and_backend_detection_uses_url_path() {
    let config = json!({});
    let secret = secret("at");
    let prepared = Codex
        .prepare_connect(PrepareContext {
            provider: provider(&config, None),
            credential: credential(&secret, &Value::Null),
            operation: OperationKey {
                operation: Operation::ConnectRealtime,
                dialect: Dialect::OpenAi,
            },
            request: WireRequest {
                method: Method::GET,
                path: "/v1/live/rtc_bound".into(),
                query: None,
                headers: HeaderMap::new(),
                body: (),
            },
            endpoint_override: Some("https://example.test/custom/live?deployment=x"),
        })
        .unwrap();
    assert_eq!(
        prepared.uri(),
        "wss://example.test/custom/live/rtc_bound?deployment=x"
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        HeaderValue::from_static("multipart/form-data; boundary=rtc"),
    );
    let original = realtime_multipart();
    let prepared = Codex
        .prepare(PrepareContext {
            provider: provider(&config, None),
            credential: credential(&secret, &Value::Null),
            operation: OperationKey {
                operation: Operation::CreateRealtimeCall,
                dialect: Dialect::OpenAi,
            },
            request: WireRequest {
                method: Method::POST,
                path: "/v1/realtime/calls".into(),
                query: None,
                headers,
                body: HttpBody::Bytes(original.clone()),
            },
            endpoint_override: Some("https://example.test/v1/realtime/calls?label=/backend-api"),
        })
        .unwrap();
    let HttpBody::Bytes(actual) = prepared.into_body() else {
        panic!("buffered")
    };
    assert_eq!(actual, original);
}

#[tokio::test]
async fn realtime_multipart_read_failures_are_transport_errors_with_sources() {
    let config = json!({});
    let secret = secret("at");
    let client = Arc::new(ScriptClient::new(vec![]));
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        HeaderValue::from_static("multipart/form-data; boundary=rtc"),
    );
    let error = Codex
        .create_realtime_call(OperationContext {
            provider: provider(&config, None),
            credential: credential(&secret, &Value::Null),
            dialect: Dialect::OpenAi,
            request: WireRequest {
                method: Method::POST,
                path: "/v1/realtime/calls".into(),
                query: None,
                headers,
                body: HttpBody::Stream(Box::pin(futures_util::stream::once(async {
                    Err(std::io::Error::other("socket interrupted").into())
                }))),
            },
            client: client.clone(),
            state: Arc::new(MemoryState::default()),
            instance_id: Arc::from("i"),
            endpoint_override: None,
        })
        .await
        .unwrap_err();
    let ChannelError::Transport(error) = error else {
        panic!("transport error: {error}")
    };
    assert_eq!(
        error.kind(),
        gproxy_protocol::capability::CapabilityErrorKind::Transport
    );
    assert!(std::error::Error::source(&error).is_some());
    assert!(client.sent().is_empty());
}

#[test]
fn image_sse_usage_observer_handles_split_frames() {
    let headers = HeaderMap::new();
    for (operation, kind) in [
        (Operation::CreateImage, "image_generation.completed"),
        (Operation::EditImage, "image_edit.completed"),
    ] {
        let mut observer = Codex
            .usage_stream()
            .unwrap()
            .start(UsageStreamContext {
                operation: OperationKey {
                    operation,
                    dialect: Dialect::OpenAi,
                },
                request_body: None,
                status: StatusCode::OK,
                headers: &headers,
                transport: UsageTransport::Http {
                    framing: Some(StreamFraming::Sse),
                },
            })
            .unwrap();
        let value = json!({"type":kind,"generation_id":"g","quality":"high","size":"1024x1024","usage":{"input_tokens":15,"output_tokens":10,"input_tokens_details":{"image_tokens":12,"text_tokens":3}}});
        let wire = format!("event: {kind}\ndata: {value}\n\n");
        for chunk in wire.as_bytes().chunks(7) {
            observer.observe(UsageFrame::HttpChunk(chunk)).unwrap();
        }
        let usage = observer.finish(UsageStreamEnd::Complete).unwrap().unwrap();
        assert_eq!(usage.tokens.input_tokens, Some(15));
        assert_eq!(usage.tokens.output_tokens, Some(10));
        assert_eq!(usage.metrics["image_outputs"], 1.into());
        assert_eq!(usage.metrics["image_input_tokens"], 12.into());
        assert_eq!(usage.dimensions["quality"], "high");
    }
}

async fn model_call(
    operation: Operation,
    path: &str,
    query: Option<&str>,
    headers: HeaderMap,
    reply: WireResponse,
) -> (WireResponse, Vec<Sent>) {
    let config = json!({});
    let secret = secret("at");
    let client = Arc::new(ScriptClient::new(vec![reply]));
    let ctx = OperationContext {
        provider: provider(&config, None),
        credential: credential(&secret, &Value::Null),
        dialect: Dialect::OpenAi,
        request: WireRequest {
            method: Method::GET,
            path: path.into(),
            query: query.map(str::to_owned),
            headers,
            body: HttpBody::Bytes(Bytes::new()),
        },
        client: client.clone(),
        state: Arc::new(MemoryState::default()),
        instance_id: Arc::from("i"),
        endpoint_override: None,
    };
    let response = if operation == Operation::ListModels {
        Codex.list_models(ctx).await.unwrap()
    } else {
        Codex.get_model(ctx).await.unwrap()
    };
    (response, client.sent())
}
#[tokio::test]
async fn model_catalog_is_standard_for_every_user_agent_and_keeps_v3_extensions() {
    let catalog = json!({"models":[{"slug":"gpt-6-astra","display_name":"Astra","base_instructions":"rules","context_window":272000,"max_context_window":872000,
        "supported_reasoning_levels":["high",{"effort":"xhigh","description":"More reasoning"}],"service_tiers":[{"id":"priority","name":"Fast","description":"Fast tier"}],"future_metadata":{"value":1}}],"future_catalog":true});
    for cli in [false, true] {
        let mut headers = HeaderMap::new();
        if cli {
            headers.insert(
                "user-agent",
                HeaderValue::from_static("codex_cli_rs/0.155.1"),
            );
        }
        let (response, sent) = model_call(
            Operation::ListModels,
            "/v1/models",
            None,
            headers,
            reply(StatusCode::OK, catalog.clone()),
        )
        .await;
        assert!(
            sent[0]
                .1
                .ends_with(&format!("/models?client_version={CLI_VERSION}"))
        );
        let value = body_json(response).await;
        assert_eq!(value["object"], "list");
        assert!(value.get("models").is_none());
        assert_eq!(value["future_catalog"], true);
        let model = &value["data"][0];
        assert_eq!(model["id"], "gpt-6-astra");
        assert_eq!(model["instructions"], "rules");
        assert_eq!(model["context_window"], 872000);
        assert_eq!(model["max_context_window"], 872000);
        assert_eq!(
            model["supported_reasoning_levels"][0],
            json!({"effort":"high","description":""})
        );
        assert_eq!(model["future_metadata"], json!({"value":1}));
        assert!(model.get("created").is_none());
        assert!(model.get("owned_by").is_none());
    }
}
#[tokio::test]
async fn model_lookup_selects_exact_id_and_preserves_errors_and_client_version() {
    let catalog = json!({"models":[{"slug":"wrong"},{"slug":"vendor/model+v1"}]});
    let (response, sent) = model_call(
        Operation::GetModel,
        "/v1/models/vendor%2Fmodel+v1",
        Some("client_version=9.9.9&extra=1"),
        HeaderMap::new(),
        reply(StatusCode::OK, catalog.clone()),
    )
    .await;
    assert!(sent[0].1.ends_with("/models?client_version=9.9.9&extra=1"));
    assert_eq!(body_json(response).await["id"], "vendor/model+v1");
    let (response, _) = model_call(
        Operation::GetModel,
        "/v1/models/missing",
        None,
        HeaderMap::new(),
        reply(StatusCode::OK, catalog),
    )
    .await;
    assert_eq!(response.status, StatusCode::NOT_FOUND);
    let (response, _) = model_call(
        Operation::ListModels,
        "/v1/models",
        None,
        HeaderMap::new(),
        reply(StatusCode::FORBIDDEN, json!({"error":"denied"})),
    )
    .await;
    assert_eq!(response.status, StatusCode::FORBIDDEN);
    assert_eq!(body_json(response).await, json!({"error":"denied"}));
}
