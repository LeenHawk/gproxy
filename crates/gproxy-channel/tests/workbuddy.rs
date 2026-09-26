#![cfg(feature = "workbuddy")]
//! WorkBuddy against a scripted client: no real upstream is called.

mod support;

use futures_util::StreamExt as _;
use gproxy_channel::channel::{
    BaseChannel, ChannelError, CredentialContext, CredentialRefresh, CredentialView, DevicePoll,
    LoginContext, NoState, OAuthDeviceCode, OperationContext, PrepareContext, ProviderView,
    QuotaQuery, QuotaValue, ResponseView, UsageContext, UsageExtractor, UsageFrame, UsageStream,
    UsageStreamContext, UsageStreamEnd, UsageTransport,
};
use gproxy_channel::channels::workbuddy::{CLI_VERSION, ENTERPRISE_DIMENSION, WorkBuddy};
use gproxy_channel::{LoginMode, OutboundClient};
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

type Sent = (Method, String, HeaderMap, Vec<u8>);

struct ScriptClient {
    replies: Mutex<VecDeque<WireResponse>>,
    requests: Mutex<Vec<Sent>>,
}

impl ScriptClient {
    fn new(replies: Vec<WireResponse>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
        })
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
    raw_reply(status, serde_json::to_vec(&value).unwrap())
}

fn raw_reply(status: StatusCode, body: impl Into<Vec<u8>>) -> WireResponse {
    WireResponse {
        status,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(body.into())),
    }
}

fn provider<'a>(config: &'a Value, base_url: Option<&'a str>) -> ProviderView<'a> {
    ProviderView {
        id: "w",
        channel: "workbuddy",
        base_url,
        config,
    }
}

fn credential<'a>(secret: &'a Value, metadata: &'a Value) -> CredentialView<'a> {
    CredentialView {
        id: "c",
        provider_id: "w",
        auth_kind: "oauth",
        secret,
        metadata,
        version: 4,
        expires_at_ms: None,
    }
}

fn secret() -> Value {
    json!({"access_token": "wb-access", "refresh_token": "wb-refresh"})
}

/// A personal seat, as the host publishes what the login learned.
fn personal() -> Value {
    json!({"user_id": "uid-1"})
}

/// An enterprise seat additionally announces its tenant and department.
fn enterprise() -> Value {
    json!({"user_id": "uid-1", "enterprise_id": "ent-7",
           "department_full_name": "Infra/Platform", "domain": "corp.example"})
}

fn request(headers: HeaderMap, method: Method, path: &str, body: Value) -> WireRequest<HttpBody> {
    let mut all = headers;
    all.insert("authorization", HeaderValue::from_static("Bearer client"));
    all.insert("host", HeaderValue::from_static("gproxy.local"));
    all.insert("content-length", HeaderValue::from_static("2"));
    WireRequest {
        method,
        path: path.into(),
        query: None,
        headers: all,
        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&body).unwrap())),
    }
}

fn prepare(
    config: &Value,
    base_url: Option<&str>,
    metadata: &Value,
    operation: Operation,
    dialect: Dialect,
    request: WireRequest<HttpBody>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    let secret = secret();
    WorkBuddy.prepare(PrepareContext {
        provider: provider(config, base_url),
        credential: credential(&secret, metadata),
        operation: OperationKey { operation, dialect },
        request,
        endpoint_override: None,
    })
}

fn body_json(request: &http::Request<HttpBody>) -> Value {
    let HttpBody::Bytes(bytes) = request.body() else {
        panic!("a buffered body");
    };
    serde_json::from_slice(bytes).unwrap()
}

async fn collect(body: HttpBody) -> Vec<u8> {
    match body {
        HttpBody::Bytes(bytes) => bytes.to_vec(),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                out.extend_from_slice(&chunk.expect("a chunk"));
            }
            out
        }
    }
}

// ------------------------------------------------------------------ prepare

#[test]
fn a_chat_call_carries_the_account_and_the_plugin_identity() {
    let prepared = prepare(
        &json!({}),
        None,
        &enterprise(),
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        request(
            HeaderMap::new(),
            Method::POST,
            "/v1/chat/completions",
            json!({"model": "hunyuan-turbo", "messages": []}),
        ),
    )
    .expect("prepared");
    assert_eq!(
        prepared.uri(),
        "https://copilot.tencent.com/v2/chat/completions"
    );
    let headers = prepared.headers();
    assert_eq!(headers["authorization"], "Bearer wb-access");
    assert_eq!(headers["x-user-id"], "uid-1");
    assert_eq!(headers["x-enterprise-id"], "ent-7");
    assert_eq!(
        headers["x-tenant-id"], "ent-7",
        "the enterprise id is announced under both names"
    );
    assert_eq!(headers["x-department-info"], "Infra/Platform");
    assert_eq!(headers["x-domain"], "corp.example");
    assert_eq!(headers["x-product"], "SaaS");
    assert_eq!(headers["x-ide-type"], "CLI");
    assert_eq!(headers["x-ide-name"], "CLI");
    assert_eq!(headers["x-ide-version"], CLI_VERSION);
    assert_eq!(headers["x-agent-intent"], "craft");
    assert_eq!(headers["user-agent"], format!("WorkBuddy/{CLI_VERSION}"));
    assert_eq!(headers["content-type"], "application/json");
    assert!(headers.get("host").is_none());
    // The three per-request ids share one value and are not the session.
    assert_eq!(
        headers["x-request-id"],
        headers["x-conversation-message-id"]
    );
    assert_eq!(
        headers["x-request-id"],
        headers["x-conversation-request-id"]
    );
    assert_ne!(headers["x-request-id"], headers["x-conversation-id"]);
}

#[test]
fn a_personal_seat_announces_only_what_it_has() {
    let prepared = prepare(
        &json!({}),
        None,
        &personal(),
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        request(
            HeaderMap::new(),
            Method::POST,
            "/v1/chat/completions",
            json!({"messages": []}),
        ),
    )
    .expect("prepared");
    assert_eq!(prepared.headers()["x-user-id"], "uid-1");
    for name in [
        "x-enterprise-id",
        "x-tenant-id",
        "x-department-info",
        "x-domain",
    ] {
        assert!(prepared.headers().get(name).is_none(), "{name}");
    }
}

#[test]
fn the_clients_conversation_id_survives_under_its_own_name() {
    let mut caller = HeaderMap::new();
    caller.insert("x-conversation-id", HeaderValue::from_static("sess-42"));
    caller.insert("x-request-id", HeaderValue::from_static("forged"));
    caller.insert(
        "x-conversation-message-id",
        HeaderValue::from_static("forged"),
    );
    let prepared = prepare(
        &json!({}),
        None,
        &personal(),
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        request(
            caller,
            Method::POST,
            "/v1/chat/completions",
            json!({"messages": []}),
        ),
    )
    .expect("prepared");
    let headers = prepared.headers();
    assert_eq!(
        headers["x-conversation-id"], "sess-42",
        "the session id the gateway's own ladder reads is not renamed or dropped"
    );
    assert_ne!(
        headers["x-request-id"], "forged",
        "a request id is not a session id and is minted per call"
    );
    assert_ne!(headers["x-conversation-message-id"], "forged");
}

#[test]
fn a_client_cannot_forge_the_plugin_identity_but_keeps_its_allow_listed_headers() {
    let mut caller = HeaderMap::new();
    for (name, value) in [
        ("x-user-id", "someone-else"),
        ("x-ide-version", "0.0.1"),
        ("x-product", "Enterprise"),
        ("user-agent", "curl/8"),
        ("x-refresh-token", "stolen"),
        ("cookie", "session=1"),
        ("x-trace", "keep-me"),
    ] {
        caller.insert(name, HeaderValue::from_static(value));
    }
    let prepared = prepare(
        &json!({"allowed_headers": ["x-trace"]}),
        None,
        &personal(),
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        request(
            caller,
            Method::POST,
            "/v1/chat/completions",
            json!({"messages": []}),
        ),
    )
    .expect("prepared");
    let headers = prepared.headers();
    assert_eq!(headers["x-user-id"], "uid-1");
    assert_eq!(headers["x-ide-version"], CLI_VERSION);
    assert_eq!(headers["x-product"], "SaaS");
    assert_eq!(headers["user-agent"], format!("WorkBuddy/{CLI_VERSION}"));
    assert!(headers.get("x-refresh-token").is_none());
    assert!(headers.get("cookie").is_none());
    assert_eq!(headers["x-trace"], "keep-me");
}

#[test]
fn the_operator_states_what_the_plugin_claims_to_be() {
    let prepared = prepare(
        &json!({"ide_version": "5.0.0", "ide_name": "VSCode", "ide_type": "IDE",
                "agent_intent": "chat", "headers": {"x-extra": "1"}}),
        Some("https://copilot.internal/api"),
        &personal(),
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        request(
            HeaderMap::new(),
            Method::POST,
            "/v1/chat/completions",
            json!({"messages": []}),
        ),
    )
    .expect("prepared");
    assert_eq!(
        prepared.uri(),
        "https://copilot.internal/api/v2/chat/completions"
    );
    let headers = prepared.headers();
    assert_eq!(headers["x-ide-version"], "5.0.0");
    assert_eq!(headers["x-ide-name"], "VSCode");
    assert_eq!(headers["x-ide-type"], "IDE");
    assert_eq!(headers["x-agent-intent"], "chat");
    assert_eq!(headers["user-agent"], "WorkBuddy/5.0.0");
    assert_eq!(headers["x-extra"], "1");
}

#[test]
fn a_stream_is_asked_to_end_with_its_usage_chunk() {
    let prepared = prepare(
        &json!({}),
        None,
        &personal(),
        Operation::StreamGenerateContent,
        Dialect::OpenAiChat,
        request(
            HeaderMap::new(),
            Method::POST,
            "/v1/chat/completions",
            json!({"model": "hunyuan-turbo", "messages": [], "stream": true}),
        ),
    )
    .expect("prepared");
    assert_eq!(
        body_json(&prepared)["stream_options"]["include_usage"],
        true
    );
}

#[test]
fn an_image_request_is_narrowed_to_what_hunyuan_takes() {
    let prepared = prepare(
        &json!({}),
        None,
        &personal(),
        Operation::EditImage,
        Dialect::OpenAi,
        request(
            HeaderMap::new(),
            Method::POST,
            "/v1/images/edits",
            json!({"prompt": "brighter", "moderation": "low",
                   "images": ["data:image/png;base64,QUJD"]}),
        ),
    )
    .expect("prepared");
    assert_eq!(
        prepared.uri(),
        "https://copilot.tencent.com/v2/images/edits"
    );
    let body = body_json(&prepared);
    assert_eq!(body["image"], json!(["image/png;base64,QUJD"]));
    assert_eq!(body["response_format"], "b64_json");
    assert!(body.get("moderation").is_none());
}

#[test]
fn a_credential_without_the_account_the_upstream_matches_is_refused() {
    assert!(matches!(
        prepare(
            &json!({}),
            None,
            &Value::Null,
            Operation::GenerateContent,
            Dialect::OpenAiChat,
            request(
                HeaderMap::new(),
                Method::POST,
                "/v1/chat/completions",
                json!({"messages": []}),
            ),
        ),
        Err(ChannelError::InvalidCredential),
    ));
    // The flat v3 layout still satisfies it.
    let secret = json!({"access_token": "a", "user_id": "uid-legacy"});
    let prepared = WorkBuddy
        .prepare(PrepareContext {
            provider: provider(&json!({}), None),
            credential: credential(&secret, &Value::Null),
            operation: OperationKey {
                operation: Operation::GenerateContent,
                dialect: Dialect::OpenAiChat,
            },
            request: request(
                HeaderMap::new(),
                Method::POST,
                "/v1/chat/completions",
                json!({"messages": []}),
            ),
            endpoint_override: None,
        })
        .expect("prepared");
    assert_eq!(prepared.headers()["x-user-id"], "uid-legacy");
}

#[test]
fn the_declared_dialects_follow_the_surface() {
    let config = json!({});
    assert_eq!(
        WorkBuddy.native_dialects(provider(&config, None), Operation::GenerateContent),
        [Dialect::OpenAiChat]
    );
    assert_eq!(
        WorkBuddy.native_dialects(provider(&config, None), Operation::ListModels),
        [Dialect::OpenAi]
    );
    assert_eq!(
        WorkBuddy.native_dialects(provider(&config, None), Operation::CreateImage),
        [Dialect::OpenAi]
    );
    assert!(
        WorkBuddy
            .native_dialects(provider(&config, None), Operation::CountTokens)
            .is_empty()
    );
}

// ----------------------------------------------------------------- envelope

#[tokio::test]
async fn the_configuration_document_becomes_an_openai_model_list() {
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"code": 0, "msg": "ok", "data": {
            "models": [{"id": "hunyuan-turbo"}, {"id": "hunyuan-pro"}],
            "features": {"agent": true},
        }}),
    )]);
    let config = json!({});
    let secret = secret();
    let metadata = personal();
    let response = WorkBuddy
        .list_models(OperationContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            dialect: Dialect::OpenAi,
            request: request(HeaderMap::new(), Method::GET, "/v1/models", json!({})),
            client: client.clone(),
            state: Arc::new(NoState::default()),
            instance_id: Arc::from("i"),
            endpoint_override: None,
        })
        .await
        .expect("a response");
    let body: Value = serde_json::from_slice(&collect(response.body).await).unwrap();
    assert_eq!(body["object"], "list");
    assert_eq!(body["data"][0]["id"], "hunyuan-turbo");
    assert!(body.get("features").is_none());
    let (method, url, _, _) = client.sent().into_iter().next().unwrap();
    assert_eq!(method, Method::GET);
    assert_eq!(url, "https://copilot.tencent.com/v3/config");
}

#[tokio::test]
async fn an_image_reply_is_unwrapped_and_a_failure_is_not() {
    let config = json!({});
    let secret = secret();
    let metadata = personal();
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"code": 0, "msg": "ok", "data": {"data": [{"b64_json": "QUJD"}]}}),
    )]);
    let response = WorkBuddy
        .create_image(OperationContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            dialect: Dialect::OpenAi,
            request: request(
                HeaderMap::new(),
                Method::POST,
                "/v1/images/generations",
                json!({"prompt": "a cat"}),
            ),
            client,
            state: Arc::new(NoState::default()),
            instance_id: Arc::from("i"),
            endpoint_override: None,
        })
        .await
        .expect("a response");
    let body: Value = serde_json::from_slice(&collect(response.body).await).unwrap();
    assert_eq!(body["data"][0]["b64_json"], "QUJD");

    let failed = ScriptClient::new(vec![reply(
        StatusCode::FORBIDDEN,
        json!({"code": 40003, "msg": "no seat"}),
    )]);
    let response = WorkBuddy
        .create_image(OperationContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            dialect: Dialect::OpenAi,
            request: request(
                HeaderMap::new(),
                Method::POST,
                "/v1/images/generations",
                json!({"prompt": "a cat"}),
            ),
            client: failed,
            state: Arc::new(NoState::default()),
            instance_id: Arc::from("i"),
            endpoint_override: None,
        })
        .await
        .expect("a response");
    assert_eq!(response.status, StatusCode::FORBIDDEN);
    let body: Value = serde_json::from_slice(&collect(response.body).await).unwrap();
    assert_eq!(body["code"], 40003, "an upstream error reaches the client");
}

// -------------------------------------------------------------------- usage

#[test]
fn chat_usage_is_the_shared_openai_reading() {
    let body = json!({"usage": {"prompt_tokens": 500, "completion_tokens": 8,
                                "prompt_tokens_details": {"cached_tokens": 480}}})
    .to_string();
    let headers = HeaderMap::new();
    let usage = WorkBuddy
        .extract(UsageContext {
            operation: OperationKey {
                operation: Operation::GenerateContent,
                dialect: Dialect::OpenAiChat,
            },
            request_body: None,
            response: ResponseView {
                status: StatusCode::OK,
                headers: &headers,
                body: body.as_bytes(),
            },
        })
        .expect("read")
        .expect("usage");
    assert_eq!(usage.tokens.input_tokens, Some(20));
    assert_eq!(usage.tokens.cached_input_tokens, Some(480));
}

#[test]
fn an_image_reply_that_states_no_tokens_still_states_how_many_images() {
    let body = json!({"data": [{"b64_json": "A"}, {"b64_json": "B"}]}).to_string();
    let headers = HeaderMap::new();
    let usage = WorkBuddy
        .extract(UsageContext {
            operation: OperationKey {
                operation: Operation::CreateImage,
                dialect: Dialect::OpenAi,
            },
            request_body: None,
            response: ResponseView {
                status: StatusCode::OK,
                headers: &headers,
                body: body.as_bytes(),
            },
        })
        .expect("read")
        .expect("usage");
    assert_eq!(usage.metrics["image_outputs"], 2.into());
}

#[test]
fn a_chat_stream_reports_the_usage_its_last_chunk_carries() {
    let headers = HeaderMap::new();
    let mut observer = UsageStream::start(
        &WorkBuddy,
        UsageStreamContext {
            operation: OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: Dialect::OpenAiChat,
            },
            request_body: None,
            status: StatusCode::OK,
            headers: &headers,
            transport: UsageTransport::Http {
                framing: Some(StreamFraming::Sse),
            },
        },
    )
    .expect("an observer");
    for chunk in [
        "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n",
        "data: {\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":3}}\n\n",
        "data: [DONE]\n\n",
    ] {
        observer
            .observe(UsageFrame::HttpChunk(chunk.as_bytes()))
            .expect("observed");
    }
    let usage = observer
        .finish(UsageStreamEnd::Complete)
        .expect("finished")
        .expect("usage");
    assert_eq!(usage.tokens.input_tokens, Some(12));
    assert_eq!(usage.tokens.output_tokens, Some(3));
}

// -------------------------------------------------------------------- login

#[tokio::test]
async fn the_device_login_is_a_browser_redirect_polled_to_a_credential() {
    let config = json!({});
    let start = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"code": 0, "msg": "ok", "data": {"state": "st-1",
               "authUrl": "https://copilot.tencent.com/login?state=st-1"}}),
    )]);
    let started = OAuthDeviceCode::start(
        &WorkBuddy,
        LoginContext {
            provider: provider(&config, None),
            client: start.as_ref(),
        },
    )
    .await
    .expect("a device authorization");
    let (method, url, headers, _) = start.sent().into_iter().next().unwrap();
    assert_eq!(method, Method::POST);
    assert_eq!(
        url,
        "https://copilot.tencent.com/v2/plugin/auth/state?platform=workbuddy"
    );
    assert_eq!(headers["x-product"], "SaaS");
    assert_eq!(started.device_code, "st-1");
    assert_eq!(
        started.verification_uri,
        "https://copilot.tencent.com/login?state=st-1"
    );
    assert_eq!(started.interval_secs, 1);

    // The browser has not finished yet.
    let pending = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"code": 11217, "msg": "pending"}),
    )]);
    assert!(matches!(
        WorkBuddy
            .poll(
                LoginContext {
                    provider: provider(&config, None),
                    client: pending.as_ref(),
                },
                &started,
            )
            .await
            .unwrap(),
        DevicePoll::Pending
    ));

    // Tokens are there but the account is not readable yet.
    let half = ScriptClient::new(vec![
        reply(
            StatusCode::OK,
            json!({"code": 0, "data": {"accessToken": "at", "refreshToken": "rt",
                                       "expiresIn": 3600, "domain": "corp.example"}}),
        ),
        reply(StatusCode::OK, json!({"code": 12151, "msg": "pending"})),
    ]);
    assert!(matches!(
        WorkBuddy
            .poll(
                LoginContext {
                    provider: provider(&config, None),
                    client: half.as_ref(),
                },
                &started,
            )
            .await
            .unwrap(),
        DevicePoll::Pending
    ));

    let granted = ScriptClient::new(vec![
        reply(
            StatusCode::OK,
            json!({"code": 0, "data": {"accessToken": "at", "refreshToken": "rt",
                                       "expiresIn": 3600, "domain": "corp.example"}}),
        ),
        reply(
            StatusCode::OK,
            json!({"code": 0, "data": {"uid": "uid-9", "nickname": "n",
                                       "enterpriseId": "ent-7", "enterpriseName": "Corp",
                                       "departmentFullName": "Infra/Platform"}}),
        ),
    ]);
    let DevicePoll::Ready(acquired) = WorkBuddy
        .poll(
            LoginContext {
                provider: provider(&config, None),
                client: granted.as_ref(),
            },
            &started,
        )
        .await
        .unwrap()
    else {
        panic!("a credential");
    };
    assert_eq!(acquired.access_token, "at");
    assert_eq!(acquired.refresh_token.as_deref(), Some("rt"));
    // Everything the request path must announce is a login fact.
    assert_eq!(acquired.provider_fields["user_id"], "uid-9");
    assert_eq!(acquired.provider_fields["enterprise_id"], "ent-7");
    assert_eq!(
        acquired.provider_fields["department_full_name"],
        "Infra/Platform"
    );
    assert_eq!(acquired.provider_fields["domain"], "corp.example");
    let calls = granted.sent();
    assert_eq!(
        calls[1].1,
        "https://copilot.tencent.com/v2/plugin/login/account?state=st-1"
    );
    assert_eq!(calls[1].2["x-domain"], "corp.example");
}

#[tokio::test]
async fn the_refresh_rotates_through_a_header_and_a_refusal_is_definitive() {
    let config = json!({});
    let secret = secret();
    let metadata = enterprise();

    let rotated = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"code": 0, "data": {"accessToken": "at2", "refreshToken": "rt2",
                                   "expiresIn": 3600}}),
    )]);
    let update = CredentialRefresh::refresh(
        &WorkBuddy,
        CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            client: rotated.as_ref(),
        },
    )
    .await
    .expect("a rotation");
    let (method, url, headers, body) = rotated.sent().into_iter().next().unwrap();
    assert_eq!(method, Method::POST);
    assert_eq!(
        url,
        "https://copilot.tencent.com/v2/plugin/auth/token/refresh"
    );
    assert_eq!(
        headers["x-refresh-token"], "wb-refresh",
        "the token travels in a header, not a body"
    );
    assert_eq!(headers["x-auth-refresh-source"], "plugin");
    assert_eq!(headers["x-domain"], "corp.example");
    assert_eq!(body, b"{}");
    assert_eq!(update.secret["access_token"], "at2");
    assert_eq!(update.secret["refresh_token"], "rt2");
    assert_eq!(
        update.expires_at_ms,
        update.secret["expires_at_ms"].as_i64()
    );

    // An expiry the gateway did not state is recorded as unknown, not invented.
    let silent = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"code": 0, "data": {"accessToken": "at3"}}),
    )]);
    let update = CredentialRefresh::refresh(
        &WorkBuddy,
        CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            client: silent.as_ref(),
        },
    )
    .await
    .expect("a rotation");
    assert_eq!(update.expires_at_ms, None);
    assert_eq!(update.secret["expiry_unknown"], true);
    assert_eq!(
        update.secret["refresh_token"], "wb-refresh",
        "a full replacement that keeps what the rotation did not name"
    );

    let rejected = ScriptClient::new(vec![reply(
        StatusCode::UNAUTHORIZED,
        json!({"code": 40001, "msg": "expired"}),
    )]);
    assert!(matches!(
        CredentialRefresh::refresh(
            &WorkBuddy,
            CredentialContext {
                provider: provider(&config, None),
                credential: credential(&secret, &metadata),
                client: rejected.as_ref(),
            },
        )
        .await,
        Err(ChannelError::RefreshRejected(_))
    ));

    let transient = ScriptClient::new(vec![raw_reply(StatusCode::BAD_GATEWAY, "down")]);
    assert!(
        matches!(
            CredentialRefresh::refresh(
                &WorkBuddy,
                CredentialContext {
                    provider: provider(&config, None),
                    credential: credential(&secret, &metadata),
                    client: transient.as_ref(),
                },
            )
            .await,
            Err(ChannelError::UpstreamResponse { .. })
        ),
        "a gateway failure is not a dead credential"
    );

    let no_token = json!({"access_token": "a"});
    assert!(matches!(
        CredentialRefresh::refresh(
            &WorkBuddy,
            CredentialContext {
                provider: provider(&config, None),
                credential: credential(&no_token, &metadata),
                client: ScriptClient::new(Vec::new()).as_ref(),
            },
        )
        .await,
        Err(ChannelError::RefreshRejected(_))
    ));
}

// -------------------------------------------------------------------- quota

/// No `QuotaModel`: the enterprise meter and each personal package are
/// observed, never charged.
const OBSERVE_ONLY: &[&str] = &[ENTERPRISE_DIMENSION, "pkg_*"];

#[tokio::test]
async fn the_meter_a_credential_reads_follows_its_seat() {
    let config = json!({});
    let secret = secret();

    let enterprise_meta = enterprise();
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"data": {"data": {"limitNum": 500, "credit": "120",
                                 "cycleResetTime": "2026-09-01T00:00:00Z"}}}),
    )]);
    let snapshot = WorkBuddy
        .query(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &enterprise_meta),
            client: client.as_ref(),
        })
        .await
        .expect("a snapshot");
    let (_, url, headers, body) = client.sent().into_iter().next().unwrap();
    assert_eq!(
        url,
        "https://copilot.tencent.com/v2/billing/meter/get-enterprise-user-usage"
    );
    assert_eq!(body, b"{}");
    assert_eq!(
        headers["x-user-id"], "uid-1",
        "the probe is the same account as the traffic"
    );
    assert_eq!(snapshot.entries[0].id, ENTERPRISE_DIMENSION);
    let QuotaValue::Window(window) = &snapshot.entries[0].value else {
        panic!("a window");
    };
    assert_eq!(window.used_percent, Some(24.into()));
    assert_eq!(window.period_end_ms, Some(1_788_220_800_000));
    support::assert_quota_contract(None, &[], &snapshot.entries, OBSERVE_ONLY);

    let personal_meta = personal();
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"data": {"Response": {"Data": {"Accounts": [
            {"PackageCode": "pkg_basic", "CycleCapacitySizePrecise": 1000,
             "CycleCapacityRemainPrecise": "250.5"},
        ]}}}}),
    )]);
    let snapshot = WorkBuddy
        .query(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &personal_meta),
            client: client.as_ref(),
        })
        .await
        .expect("a snapshot");
    let (_, url, _, body) = client.sent().into_iter().next().unwrap();
    assert_eq!(
        url,
        "https://copilot.tencent.com/v2/billing/meter/get-user-resource"
    );
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["ProductCode"], "p_tcaca");
    assert_eq!(body["Status"], json!([0, 3]));
    assert_eq!(snapshot.entries[0].id, "pkg_basic");
    support::assert_quota_contract(None, &[], &snapshot.entries, OBSERVE_ONLY);
}

#[tokio::test]
async fn a_meter_that_reports_nothing_is_an_error_rather_than_an_empty_snapshot() {
    let config = json!({});
    let secret = secret();
    let metadata = personal();
    let client = ScriptClient::new(vec![reply(StatusCode::OK, json!({"data": {}}))]);
    assert!(matches!(
        WorkBuddy
            .query(CredentialContext {
                provider: provider(&config, None),
                credential: credential(&secret, &metadata),
                client: client.as_ref(),
            })
            .await,
        Err(ChannelError::InvalidResponse(_))
    ));
}

// --------------------------------------------------------------- descriptor

#[test]
fn the_descriptor_names_the_one_way_in_and_the_keys_a_form_needs() {
    let descriptor = WorkBuddy.descriptor();
    assert_eq!(descriptor.id, "workbuddy");
    assert_eq!(descriptor.login_modes, [LoginMode::DeviceCode]);
    assert!(descriptor.capabilities.refresh);
    assert!(descriptor.capabilities.quota_query);
    assert!(!descriptor.capabilities.services);
    for key in [
        "ide_version",
        "ide_name",
        "agent_intent",
        "headers",
        "allowed_headers",
    ] {
        assert!(descriptor.config_key(key).is_some(), "{key}");
    }
}

#[test]
fn there_is_no_plugin_fingerprint_to_reproduce() {
    assert!(WorkBuddy.default_connection().is_none());
}
