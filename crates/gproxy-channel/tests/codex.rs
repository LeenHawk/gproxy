#![cfg(feature = "codex")]

use base64::Engine;
use gproxy_channel::{
    BaseChannel, ChannelError, OutboundClient,
    channel::{
        AuthorizationRequest, CredentialContext, CredentialView, DevicePoll, LoginContext,
        PrepareContext, ProviderView, QuotaHeaderContext, QuotaValue, ResponseView, UsageContext,
        UsageFrame, UsageStreamContext, UsageStreamEnd, UsageTransport,
    },
    channels::codex::{Codex, DEFAULT_CLIENT_ID},
};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    capability::{CapabilityError, CapabilityFuture},
    connection::{Bytes, StreamFraming},
};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

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
    let config = json!({"allowed_headers": ["session-id"]});
    let secret = secret("at-1");
    let mut headers = HeaderMap::new();
    headers.insert("session-id", HeaderValue::from_static("sess-1"));
    headers.insert("x-client-request-id", HeaderValue::from_static("thread"));
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
    assert_eq!(h["session-id"], "sess-1");
    assert!(h.get("x-client-request-id").is_none());
    assert!(
        h.get("openai-beta").is_none(),
        "channel identity headers are never client-supplied"
    );
    assert_eq!(h["authorization"], "Bearer at-1");
}
