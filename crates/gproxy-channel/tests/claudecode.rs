#![cfg(feature = "claudecode")]

mod support;

use gproxy_channel::{
    BaseChannel, ChannelError, OperationContext, OutboundClient,
    channel::{
        AuthorizationCode, AuthorizationRequest, CallerUsage, CallerUsageWindow, ChannelState,
        CredentialContext, CredentialView, LoginContext, NoState, PrepareContext, ProviderView,
        QuotaHeaderContext, QuotaScope, QuotaValue, QuotaWindow, ServiceContext, ServiceView,
    },
    channels::claudecode::{
        CLI_USER_AGENT, Claudecode, DEFAULT_CLIENT_ID, DEFAULT_REDIRECT_URI, KIND_FILE, KIND_SKILL,
    },
};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    capability::{
        CapabilityError, CapabilityFuture, CapabilityLimits, CasResult, StateEntry, StateWrite,
        Version,
    },
    connection::Bytes,
};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use support::ScriptCaller;

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
        id: "cc",
        channel: "claudecode",
        base_url,
        config,
    }
}

fn secret(access: &str) -> Value {
    json!({
        "access_token": access,
        "refresh_token": "rt-1",
        "scopes": ["user:inference", "user:projects:read", "user:plugins"],
        "provider_fields": {"device_id": "device-1", "account_uuid": "account-1"}
    })
}

fn credential<'a>(secret: &'a Value, metadata: &'a Value) -> CredentialView<'a> {
    CredentialView {
        id: "c",
        provider_id: "cc",
        auth_kind: "oauth",
        secret,
        metadata,
        version: 1,
        expires_at_ms: None,
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn key(operation: Operation) -> OperationKey {
    OperationKey {
        operation,
        dialect: Dialect::Claude,
    }
}

fn messages_request(headers: HeaderMap, body: Value) -> WireRequest {
    WireRequest {
        method: Method::POST,
        path: "/v1/messages".into(),
        query: None,
        headers,
        body: HttpBody::Bytes(Bytes::from(body.to_string())),
    }
}

#[test]
fn default_connections_separate_api_requests_and_cookie_login() {
    use gproxy_channel::channel::ConnectionPurpose;
    use gproxy_client::{Backend, EmulationConfig};
    let api = Claudecode.default_connection().expect("channel default");
    assert_eq!(api.backend, Backend::Wreq);
    assert!(api.emulation.is_none());
    let cookie = Claudecode
        .default_connection_for(ConnectionPurpose::CookieLogin)
        .unwrap();
    assert_eq!(cookie.backend, Backend::Wreq);
    assert!(matches!(
        cookie.emulation,
        Some(EmulationConfig::Preset { .. })
    ));
}

#[test]
fn prepares_messages_with_cli_identity_and_request_hygiene() {
    let config = json!({});
    let secret = secret("at-1");
    let metadata = json!({"device_id": "device-meta", "account_uuid": "account-meta"});
    let mut headers = HeaderMap::new();
    headers.insert("authorization", HeaderValue::from_static("Bearer client"));
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_static("feature-x,context-1m-2025-08-07,oauth-2025-04-20"),
    );
    headers.insert(
        "x-claude-code-session-id",
        HeaderValue::from_static("session-1"),
    );
    headers.insert(
        "user-agent",
        HeaderValue::from_static("claude-cli/2.1.280 (external, sdk-cli)"),
    );
    headers.insert("x-request-id", HeaderValue::from_static("req-1"));
    let body = json!({
        "model": "claude-opus-4-8",
        "speed": "fast",
        "temperature": 0.7,
        "top_p": 0.9,
        "top_k": 40,
        "system": [
            {"type":"text", "text":"x-anthropic-billing-header: cc_version=2.1.280.abc; cc_entrypoint=sdk-cli;"},
            {"type":"text", "text":" policy "},
            {"type":"text", "text":" ", "cache_control":{"type":"ephemeral"}}
        ],
        "messages": [
            {"role":"user", "content":"aaaa😀 reply with exactly: ok"},
            {"role":"assistant", "content":"prefix"}
        ]
    });
    let request = Claudecode
        .prepare(PrepareContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            operation: key(Operation::GenerateContent),
            request: messages_request(headers, body),
            endpoint_override: None,
        })
        .unwrap();
    assert_eq!(
        request.uri().to_string(),
        "https://api.anthropic.com/v1/messages?beta=true"
    );
    let h = request.headers();
    assert_eq!(h["authorization"], "Bearer at-1");
    assert_eq!(h["anthropic-version"], "2023-06-01");
    assert_eq!(
        h["anthropic-beta"], "oauth-2025-04-20,feature-x,fast-mode-2026-02-01",
        "oauth beta first, client betas kept, context-1m stripped, fast mode derived"
    );
    assert_eq!(h["x-app"], "cli");
    assert_eq!(h["user-agent"], "claude-cli/2.1.280 (external, sdk-cli)");
    assert_eq!(h["x-claude-code-session-id"], "session-1");
    assert_eq!(h["x-stainless-package-version"], "0.112.1");
    assert_eq!(h["anthropic-dangerous-direct-browser-access"], "true");
    assert_eq!(h["x-request-id"], "req-1", "other client headers pass");
    let HttpBody::Bytes(bytes) = request.into_body() else {
        panic!("buffered");
    };
    let shaped: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(shaped.get("temperature").is_none());
    assert!(shaped.get("top_p").is_none());
    assert!(shaped.get("top_k").is_none());
    assert_eq!(shaped["messages"][1]["role"], "user", "prefill coerced");
    assert_eq!(shaped["system"][1]["text"], "policy");
    assert_eq!(shaped["system"][1]["cache_control"]["type"], "ephemeral");
    let billing = shaped["system"][0]["text"].as_str().unwrap();
    let (fixed, prompt) = billing.split_once(" cc_prompt_id=").unwrap();
    assert_eq!(
        fixed,
        "x-anthropic-billing-header: cc_version=2.1.280.b74; cc_entrypoint=sdk-cli; cch=00000;"
    );
    assert_eq!(prompt.len(), 37, "a derived prompt UUID: {prompt}");
    assert!(prompt.ends_with(';'));
    let ids: Value = serde_json::from_str(shaped["metadata"]["user_id"].as_str().unwrap()).unwrap();
    assert_eq!(ids["device_id"], "device-meta", "metadata over secret");
    assert_eq!(ids["account_uuid"], "account-meta");
    assert_eq!(ids["session_id"], "session-1");

    let get = Claudecode
        .prepare(PrepareContext {
            provider: provider(&config, Some("https://mirror.example/")),
            credential: credential(&secret, &Value::Null),
            operation: key(Operation::GetModel),
            request: WireRequest {
                method: Method::GET,
                path: "/v1/models/claude-sonnet-4-6".into(),
                query: Some("key=downstream&foo=1".into()),
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::new()),
            },
            endpoint_override: None,
        })
        .unwrap();
    assert_eq!(
        get.uri().to_string(),
        "https://mirror.example/v1/models/claude-sonnet-4-6?foo=1"
    );
    assert_eq!(get.headers()["user-agent"], CLI_USER_AGENT);
    let session = get.headers()["x-claude-code-session-id"].to_str().unwrap();
    assert_eq!(session.len(), 36, "derived session id is a UUID: {session}");

    let count = Claudecode
        .prepare(PrepareContext {
            provider: provider(&config, None),
            credential: credential(&secret, &Value::Null),
            operation: key(Operation::CountTokens),
            request: WireRequest {
                method: Method::POST,
                path: "/v1/messages/count_tokens".into(),
                query: None,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::from_static(
                    br#"{"model":"claude-sonnet-4-6","speed":"fast","messages":[],"metadata":{"kept":true}}"#,
                )),
            },
            endpoint_override: Some("https://relay.example/count?fixed=1"),
        })
        .unwrap();
    assert_eq!(
        count.uri().to_string(),
        "https://relay.example/count?fixed=1&beta=true"
    );
    assert_eq!(
        count.headers()["anthropic-beta"],
        "oauth-2025-04-20,fast-mode-2026-02-01"
    );
    let HttpBody::Bytes(bytes) = count.into_body() else {
        panic!("buffered");
    };
    let count_body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        count_body["metadata"],
        json!({"kept": true}),
        "count_tokens bodies are not rewritten"
    );

    assert!(matches!(
        Claudecode.prepare(PrepareContext {
            provider: provider(&config, None),
            credential: credential(&secret, &Value::Null),
            operation: key(Operation::CreateEmbedding),
            request: WireRequest {
                method: Method::POST,
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
        Claudecode.native_dialects(provider(&config, None), Operation::StreamGenerateContent),
        vec![Dialect::Claude]
    );
    assert!(
        Claudecode
            .native_dialects(provider(&config, None), Operation::CreateImage)
            .is_empty()
    );
}

#[test]
fn billing_block_keeps_valid_client_fragments_in_cli_order() {
    let config = json!({});
    let secret = secret("at-1");
    let metadata = json!({"device_id": "device-1", "account_uuid": "account-1"});
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-claude-code-session-id",
        HeaderValue::from_static("session-1"),
    );
    let billing = |text: &str| {
        let body = json!({
            "model": "claude-opus-4-8",
            "system": [{"type": "text", "text": text}],
            "messages": [{"role": "user", "content": "aaaa😀 reply with exactly: ok"}]
        });
        let request = Claudecode
            .prepare(PrepareContext {
                provider: provider(&config, None),
                credential: credential(&secret, &metadata),
                operation: key(Operation::GenerateContent),
                request: messages_request(headers.clone(), body),
                endpoint_override: None,
            })
            .unwrap();
        let HttpBody::Bytes(bytes) = request.into_body() else {
            panic!("buffered");
        };
        let shaped: Value = serde_json::from_slice(&bytes).unwrap();
        shaped["system"][0]["text"].as_str().unwrap().to_owned()
    };

    // Every fragment present, out of order, with one that fails the CLI's
    // regex and one the CLI never emits.
    assert_eq!(
        billing(concat!(
            "x-anthropic-billing-header: cc_prompt_id=0B1C2D3E-4F50-4A6B-8C7D-8E9F0A1B2C3D; ",
            "cc_version=2.1.200.zzz; cc_prev_req=req_0123abc-XYZ_; cc_is_subagent=true; ",
            "cc_workload=agent_run; cc_entrypoint=sdk-ts; cc_secret=leak; cch=11111; cc_turn_origin=user;"
        )),
        concat!(
            "x-anthropic-billing-header: cc_version=2.1.280.b74; cc_entrypoint=sdk-ts; cch=00000; ",
            "cc_workload=agent_run; cc_is_subagent=true; cc_prev_req=req_0123abc-XYZ_; ",
            "cc_prompt_id=0B1C2D3E-4F50-4A6B-8C7D-8E9F0A1B2C3D; cc_turn_origin=user;"
        )
    );
    // Invalid values are dropped rather than forwarded: a prev_req without
    // the CLI shape, a subagent flag that is not `true`, a workload with a
    // space, an entrypoint with a slash, and a prompt id that is not a UUID
    // (which is then derived instead).
    let derived = billing(concat!(
        "x-anthropic-billing-header: cc_version=2.1.200.zzz; cc_entrypoint=sdk/ts; ",
        "cc_workload=agent run; cc_is_subagent=false; cc_prev_req=nope; cc_prompt_id=prompt-1;"
    ));
    let (fixed, prompt) = derived.split_once(" cc_prompt_id=").unwrap();
    assert_eq!(
        fixed,
        "x-anthropic-billing-header: cc_version=2.1.280.b74; cc_entrypoint=cli; cch=00000;"
    );
    assert_eq!(prompt.len(), 37);
    for invalid in [
        "User",
        "_user",
        "user-1",
        "user1",
        "",
        "abcdefghijklmnopqrstuvwxyzabcdefg",
        "用户",
    ] {
        let shaped = billing(&format!(
            "x-anthropic-billing-header: cc_turn_origin={invalid};"
        ));
        assert!(!shaped.contains("cc_turn_origin="), "{invalid}");
    }
    for valid in ["u", "tool_result", "abcdefghijklmnopqrstuvwxyzabcdef"] {
        let shaped = billing(&format!(
            "x-anthropic-billing-header: cc_turn_origin={valid};"
        ));
        assert!(shaped.ends_with(&format!(" cc_turn_origin={valid};")));
    }
    // The same device, session and prompt derive the same id.
    assert_eq!(
        billing("x-anthropic-billing-header: cc_version=2.1.200.zzz; cc_entrypoint=cli;"),
        derived
    );
}

type Entry = (Bytes, Version, Option<SystemTime>);

/// A tiny CAS store: expired entries read as absent, versions never repeat.
#[derive(Default)]
struct MemoryState {
    entries: Mutex<HashMap<String, Entry>>,
    counter: AtomicU64,
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
        NoState::default().limits
    }
}

#[tokio::test]
async fn prev_req_follows_the_session_and_prompt_id_follows_user_text() {
    let config = json!({});
    let secret = secret("at-1");
    let metadata = json!({"device_id": "device-1", "account_uuid": "account-1"});
    let state = Arc::new(MemoryState::default());
    let reply_with_id = |id: &'static str| {
        let mut headers = HeaderMap::new();
        headers.insert("request-id", HeaderValue::from_static(id));
        WireResponse {
            status: StatusCode::OK,
            headers,
            body: HttpBody::Bytes(Bytes::from_static(b"{}")),
        }
    };
    let client = Arc::new(ScriptClient::new(vec![
        reply_with_id("req_first"),
        reply_with_id("req_second"),
        reply_with_id("req_third"),
        reply_with_id("req_other"),
    ]));
    let call = |session: &'static str, messages: Value| {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-claude-code-session-id",
            HeaderValue::from_static(session),
        );
        Claudecode.generate_content(OperationContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            dialect: Dialect::Claude,
            request: messages_request(
                headers,
                json!({"model": "claude-opus-4-8", "messages": messages}),
            ),
            client: client.clone(),
            state: state.clone(),
            instance_id: Arc::from("i"),
            endpoint_override: None,
        })
    };
    let billing = |index: usize| -> (Option<String>, String) {
        let sent = client.sent();
        let body: Value = serde_json::from_slice(&sent[index].3).unwrap();
        let text = body["system"][0]["text"].as_str().unwrap();
        let fields: Vec<(&str, &str)> = text
            .trim_start_matches("x-anthropic-billing-header:")
            .split(';')
            .filter_map(|field| field.trim().split_once('='))
            .collect();
        let get = |name: &str| {
            fields
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_owned())
        };
        (get("cc_prev_req"), get("cc_prompt_id").unwrap())
    };

    let prompt = json!([{"role": "user", "content": "first prompt"}]);
    call("session-a", prompt.clone()).await.unwrap();
    let (prev, prompt_a) = billing(0);
    assert_eq!(
        prev, None,
        "the first call of a session has no previous request"
    );

    // The tool loop of the same prompt: a tool_result continuation.
    let continued = json!([
        {"role": "user", "content": "first prompt"},
        {"role": "assistant", "content": [{"type": "tool_use", "id": "t1", "name": "ls", "input": {}}]},
        {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]}
    ]);
    call("session-a", continued).await.unwrap();
    let (prev, prompt_b) = billing(1);
    assert_eq!(prev.as_deref(), Some("req_first"));
    assert_eq!(prompt_b, prompt_a, "one prompt id across the tool loop");

    // A new user message is a new prompt.
    let next = json!([
        {"role": "user", "content": "first prompt"},
        {"role": "assistant", "content": "done"},
        {"role": "user", "content": [{"type": "text", "text": "second prompt"}]}
    ]);
    call("session-a", next).await.unwrap();
    let (prev, prompt_c) = billing(2);
    assert_eq!(prev.as_deref(), Some("req_second"));
    assert_ne!(prompt_c, prompt_a);

    // Another session knows nothing about the first.
    call("session-b", prompt).await.unwrap();
    let (prev, prompt_d) = billing(3);
    assert_eq!(prev, None);
    assert_ne!(prompt_d, prompt_a, "prompt ids are per session");
    assert_eq!(
        String::from_utf8(
            state
                .get("session:session-a:prev_req")
                .await
                .unwrap()
                .unwrap()
                .payload
                .to_vec()
        )
        .unwrap(),
        "req_third"
    );
}

#[test]
fn allowed_headers_narrows_forwarding_and_identity_cannot_be_spoofed() {
    let config = json!({"allowed_headers": ["x-request-id"]});
    let secret = secret("at-1");
    let mut headers = HeaderMap::new();
    headers.insert("x-request-id", HeaderValue::from_static("req-1"));
    headers.insert("x-custom", HeaderValue::from_static("dropped"));
    headers.insert("x-app", HeaderValue::from_static("spoof"));
    headers.insert("cookie", HeaderValue::from_static("sessionKey=spoof"));
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_static("interleaved-thinking-2025-05-14"),
    );
    headers.insert(
        "x-claude-code-session-id",
        HeaderValue::from_static("client-session"),
    );
    headers.insert("user-agent", HeaderValue::from_static("curl/8"));
    headers.insert("x-stainless-runtime", HeaderValue::from_static("browser"));
    let request = Claudecode
        .prepare(PrepareContext {
            provider: provider(&config, None),
            credential: credential(&secret, &Value::Null),
            operation: key(Operation::StreamGenerateContent),
            request: messages_request(
                headers,
                json!({"model": "claude-sonnet-4-6", "messages": [{"role": "user", "content": "hi"}]}),
            ),
            endpoint_override: None,
        })
        .unwrap();
    let h = request.headers();
    assert_eq!(h["x-request-id"], "req-1");
    assert!(h.get("x-custom").is_none());
    assert!(h.get("cookie").is_none());
    assert_eq!(h["x-app"], "cli");
    assert_eq!(h["x-stainless-runtime"], "node");
    assert_eq!(
        h["user-agent"], CLI_USER_AGENT,
        "a foreign user agent is replaced"
    );
    // The CLI's own hints are read even though the provider list omits them.
    let betas = h["anthropic-beta"].to_str().unwrap();
    assert!(
        betas.contains("oauth-2025-04-20") && betas.contains("interleaved-thinking-2025-05-14"),
        "{betas}"
    );
    assert_eq!(
        h["x-claude-code-session-id"], "client-session",
        "the CLI's session id is honoured under an allow-list"
    );
    assert_eq!(h["authorization"], "Bearer at-1");
}

#[tokio::test]
async fn authorize_url_and_code_exchange_follow_the_cli() {
    let config = json!({});
    let client = ScriptClient::new(vec![
        reply(
            StatusCode::OK,
            json!({"access_token": "at-new", "refresh_token": "rt-new", "expires_in": 3600,
                   "scope": "user:profile user:inference"}),
        ),
        reply(
            StatusCode::OK,
            json!({"account": {"uuid": "account-1", "email": "user@example.com"},
                   "organization": {"uuid": "org-1", "organization_type": "claude_max",
                                    "rate_limit_tier": "default_claude_max_20x",
                                    "has_extra_usage_enabled": true}}),
        ),
    ]);
    let context = LoginContext {
        provider: provider(&config, None),
        client: &client,
    };
    let login = Claudecode.oauth_authorization_code().unwrap();
    let start = login
        .authorize(
            context,
            AuthorizationRequest {
                redirect_uri: "",
                state: "st",
                code_challenge: "ch",
            },
        )
        .await
        .unwrap();
    assert_eq!(start.redirect_uri, DEFAULT_REDIRECT_URI);
    assert!(
        start
            .authorize_url
            .starts_with("https://claude.com/cai/oauth/authorize?code=true&"),
        "{}",
        start.authorize_url
    );
    for expected in [
        &format!("client_id={DEFAULT_CLIENT_ID}"),
        "response_type=code",
        "redirect_uri=https%3A%2F%2Fplatform.claude.com%2Foauth%2Fcode%2Fcallback",
        "scope=org%3Acreate_api_key%20user%3Aprofile%20user%3Ainference",
        "user%3Aplugins",
        "code_challenge=ch",
        "code_challenge_method=S256",
        "state=st",
    ] {
        assert!(start.authorize_url.contains(expected), "{expected}");
    }

    let before = now_ms();
    let credential = login
        .exchange(
            context,
            AuthorizationCode {
                code: "code-1",
                redirect_uri: DEFAULT_REDIRECT_URI,
                code_verifier: "ver",
                state: "st",
                provider_state: &BTreeMap::new(),
            },
        )
        .await
        .unwrap();
    assert_eq!(credential.access_token, "at-new");
    assert_eq!(credential.refresh_token.as_deref(), Some("rt-new"));
    assert_eq!(credential.scopes, vec!["user:profile", "user:inference"]);
    let expires = credential.expires_at_ms.unwrap();
    assert!(expires >= before + 3_600_000 && expires <= now_ms() + 3_600_000);
    assert_eq!(credential.provider_fields["account_uuid"], "account-1");
    assert_eq!(credential.provider_fields["user_email"], "user@example.com");
    assert_eq!(credential.provider_fields["organization_uuid"], "org-1");
    assert_eq!(
        credential.provider_fields["rate_limit_tier"],
        "default_claude_max_20x"
    );
    assert_eq!(credential.provider_fields["has_extra_usage_enabled"], true);
    assert_eq!(
        credential.provider_fields["device_id"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    let sent = client.sent();
    assert_eq!(sent[0].0, Method::POST);
    assert_eq!(sent[0].1, "https://platform.claude.com/v1/oauth/token");
    assert_eq!(sent[0].2["content-type"], "application/json");
    let body: Value = serde_json::from_slice(&sent[0].3).unwrap();
    assert_eq!(body["grant_type"], "authorization_code");
    assert_eq!(body["client_id"], DEFAULT_CLIENT_ID);
    assert_eq!(body["code"], "code-1");
    assert_eq!(body["code_verifier"], "ver");
    assert_eq!(body["state"], "st");
    assert_eq!(sent[1].1, "https://api.anthropic.com/api/oauth/profile");
    assert_eq!(sent[1].2["authorization"], "Bearer at-new");
}

#[tokio::test]
async fn cookie_login_bootstraps_authorizes_and_exchanges() {
    let config = json!({});
    let client = ScriptClient::new(vec![
        raw_reply(StatusCode::FORBIDDEN, "<title>Just a moment...</title>"),
        raw_reply(
            StatusCode::OK,
            r#"{"usage":{}} {"account":{"memberships":[{"organization":{"uuid":"org-api","capabilities":["api"]}},{"organization":{"uuid":"org-sub","capabilities":["claude_max"]}}]}}"#,
        ),
        reply(
            StatusCode::OK,
            json!({"redirect_uri": "https://platform.claude.com/oauth/code/callback?code=code-1&state=state"}),
        ),
        reply(
            StatusCode::OK,
            json!({"access_token": "fresh", "expires_in": 3600, "scope": "user:inference user:file_upload"}),
        ),
        reply(
            StatusCode::OK,
            json!({"account": {"uuid": "account-1", "email": "user@example.com"},
                   "organization": {"uuid": "org-sub", "organization_type": "claude_max"}}),
        ),
    ]);
    let acquired = Claudecode
        .cookie_login()
        .unwrap()
        .exchange_cookie(
            LoginContext {
                provider: provider(&config, None),
                client: &client,
            },
            "Cookie: cf_clearance=clear; sessionKey=sk-ant-sid01-example",
        )
        .await
        .unwrap();
    assert_eq!(acquired.secret["access_token"], "fresh");
    assert!(acquired.secret.get("refresh_token").unwrap().is_null());
    assert_eq!(
        acquired.secret["cookie"],
        "cf_clearance=clear; sessionKey=sk-ant-sid01-example"
    );
    assert!(acquired.expires_at_ms.unwrap() > now_ms());
    assert_eq!(acquired.metadata["account_uuid"], "account-1");
    assert_eq!(acquired.metadata["organization_uuid"], "org-sub");
    assert_eq!(acquired.metadata["user_email"], "user@example.com");
    assert!(
        acquired.metadata.get("cookie").is_none(),
        "the cookie is secret"
    );
    let sent = client.sent();
    assert_eq!(sent.len(), 5);
    assert_eq!(sent[0].1, "https://claude.ai/api/bootstrap");
    assert_eq!(
        sent[1].1, "https://claude.ai/api/bootstrap",
        "challenge retried"
    );
    assert_eq!(
        sent[1].2["cookie"],
        "cf_clearance=clear; sessionKey=sk-ant-sid01-example"
    );
    assert_eq!(sent[1].2["origin"], "https://claude.ai");
    assert_eq!(
        sent[2].1,
        "https://api.anthropic.com/v1/oauth/org-sub/authorize"
    );
    let authorize: Value = serde_json::from_slice(&sent[2].3).unwrap();
    assert_eq!(authorize["organization_uuid"], "org-sub");
    assert_eq!(authorize["code_challenge_method"], "S256");
    assert_eq!(sent[2].2["anthropic-beta"], "oauth-2025-04-20");
    assert_eq!(sent[3].1, "https://api.anthropic.com/v1/oauth/token");
    assert_eq!(
        sent[3].2["content-type"],
        "application/x-www-form-urlencoded"
    );
    let form = String::from_utf8(sent[3].3.clone()).unwrap();
    assert!(form.contains("grant_type=authorization_code"), "{form}");
    assert!(form.contains("code=code-1"), "{form}");
    assert!(form.contains("state="), "{form}");
    assert_eq!(sent[4].1, "https://api.anthropic.com/api/oauth/profile");

    // A cookie-only credential refreshes by minting again and keeps the
    // facts the earlier login recorded.
    let client = ScriptClient::new(vec![
        raw_reply(
            StatusCode::OK,
            r#"{"account":{"memberships":[{"organization":{"uuid":"org-sub","capabilities":["claude_pro"]}}]}}"#,
        ),
        reply(
            StatusCode::OK,
            json!({"redirect_uri": "https://platform.claude.com/oauth/code/callback?code=code-2"}),
        ),
        reply(
            StatusCode::OK,
            json!({"access_token": "fresher", "expires_in": 60}),
        ),
        reply(StatusCode::NOT_FOUND, json!({})),
    ]);
    let metadata = Value::Null;
    let update = Claudecode
        .credential_refresh()
        .unwrap()
        .refresh(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&acquired.secret, &metadata),
            client: &client,
        })
        .await
        .unwrap();
    assert_eq!(update.secret["access_token"], "fresher");
    assert_eq!(
        update.secret["cookie"],
        "cf_clearance=clear; sessionKey=sk-ant-sid01-example"
    );
    assert_eq!(
        update.secret["provider_fields"]["user_email"],
        "user@example.com"
    );
    assert_eq!(
        update.secret["scopes"],
        json!(["user:inference", "user:file_upload"])
    );
    assert!(update.expires_at_ms.unwrap() > now_ms());
    assert_eq!(client.sent().len(), 4);

    assert!(matches!(
        Claudecode
            .cookie_login()
            .unwrap()
            .exchange_cookie(
                LoginContext {
                    provider: provider(&config, None),
                    client: &client,
                },
                "not a cookie",
            )
            .await,
        Err(ChannelError::InvalidCredential)
    ));
}

#[tokio::test]
async fn refresh_rotates_tokens_and_classifies_rejections() {
    let config = json!({"token_url": "https://token.example/oauth"});
    let client = ScriptClient::new(vec![
        reply(
            StatusCode::OK,
            json!({"access_token": "fresh", "refresh_token": "rotated", "expires_in": 1800,
                   "refresh_token_expires_in": 86400, "scope": "user:inference user:projects:read"}),
        ),
        reply(StatusCode::OK, json!({"access_token": "fresh-2"})),
        reply(
            StatusCode::BAD_REQUEST,
            json!({"error": "invalid_grant", "error_description": "expired"}),
        ),
        reply(StatusCode::BAD_GATEWAY, json!({"error": "upstream"})),
        reply(
            StatusCode::UNAUTHORIZED,
            json!({"error": {"type": "authentication_error", "message": "bad"}}),
        ),
        reply(
            StatusCode::TOO_MANY_REQUESTS,
            json!({"error": {"type": "rate_limit_error"}}),
        ),
    ]);
    let mut secret = secret("stale");
    secret["scopes"] = json!(["user:inference", "user:projects:read"]);
    let metadata = json!({"account_uuid": "account-meta"});
    let refresher = Claudecode.credential_refresh().unwrap();
    let context = || CredentialContext {
        provider: provider(&config, None),
        credential: credential(&secret, &metadata),
        client: &client,
    };
    let before = now_ms();
    let update = refresher.refresh(context()).await.unwrap();
    assert_eq!(update.secret["access_token"], "fresh");
    assert_eq!(update.secret["refresh_token"], "rotated");
    assert_eq!(
        update.secret["scopes"],
        json!(["user:inference", "user:projects:read"])
    );
    let expires = update.expires_at_ms.unwrap();
    assert!(expires >= before + 1_800_000 && expires <= now_ms() + 1_800_000);
    assert!(update.secret["refresh_expires_at_ms"].as_i64().unwrap() >= before + 86_400_000);
    assert_eq!(update.secret["provider_fields"]["device_id"], "device-1");
    assert_eq!(
        update.secret["provider_fields"]["account_uuid"],
        "account-1"
    );
    assert!(update.secret.get("cookie").is_none());
    let sent = client.sent();
    assert_eq!(sent[0].0, Method::POST);
    assert_eq!(sent[0].1, "https://token.example/oauth");
    assert_eq!(sent[0].2["content-type"], "application/json");
    assert!(sent[0].2.get("anthropic-beta").is_none());
    let body: Value = serde_json::from_slice(&sent[0].3).unwrap();
    assert_eq!(body["grant_type"], "refresh_token");
    assert_eq!(body["refresh_token"], "rt-1");
    assert_eq!(body["client_id"], DEFAULT_CLIENT_ID);
    assert_eq!(
        body["scope"],
        "user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload user:plugins user:projects:read"
    );

    let update = refresher.refresh(context()).await.unwrap();
    assert_eq!(update.secret["access_token"], "fresh-2");
    assert_eq!(
        update.secret["refresh_token"], "rt-1",
        "an unrotated refresh token is kept"
    );
    assert_eq!(
        update.secret["scopes"],
        json!(["user:inference", "user:projects:read"]),
        "scopes are kept when the response names none"
    );
    let expires = update.expires_at_ms.unwrap();
    assert!(
        expires >= before + 3_600_000,
        "expires_in defaults to an hour"
    );

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
        matches!(&error, ChannelError::RefreshRejected(code) if code == "authentication_error"),
        "401 is final: {error}"
    );
    let error = refresher.refresh(context()).await.err().expect("transient");
    assert!(
        matches!(error, ChannelError::UpstreamResponse { status, .. } if status == StatusCode::TOO_MANY_REQUESTS),
        "a 429 is transient"
    );

    let no_refresh = json!({"access_token": "only"});
    let error = refresher
        .refresh(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&no_refresh, &Value::Null),
            client: &client,
        })
        .await
        .err()
        .expect("rejected");
    assert!(matches!(error, ChannelError::RefreshRejected(_)));
}

#[test]
fn quota_model_declares_account_and_family_windows() {
    let dims = Claudecode.quota_model().unwrap().dimensions(
        provider(&json!({}), None),
        credential(
            &secret("a"),
            &json!({"rate_limit_tier": "default_claude_max_20x"}),
        ),
    );
    let ids: Vec<&str> = dims.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids, vec!["five_hour", "seven_day", "seven_day_fable"]);
    assert_eq!(
        dims[0].label.as_deref(),
        Some("default_claude_max_20x 5h window")
    );
    assert_eq!(dims[0].scope, QuotaScope::All);
    assert_eq!(
        dims[0].window,
        QuotaWindow::Rolling {
            seconds: 5 * 60 * 60
        }
    );
    assert_eq!(
        dims[1].window,
        QuotaWindow::Rolling {
            seconds: 7 * 24 * 60 * 60
        }
    );
    assert_eq!(
        dims[2].scope,
        QuotaScope::ModelPrefixes(vec!["claude-fable".into()])
    );
    assert!(dims[2].scope.matches("claude-fable-5-1"));
    assert!(!dims[2].scope.matches("claude-opus-4-8"));
    assert_eq!(dims[2].window, dims[1].window);
    let dims = Claudecode.quota_model().unwrap().dimensions(
        provider(&json!({}), None),
        credential(&secret("a"), &Value::Null),
    );
    assert_eq!(dims[1].label.as_deref(), Some("unknown 7d window"));
}

#[test]
fn low_priority_marks_messages_and_frees_only_the_five_hour_window() {
    let secret = secret("at-1");
    let marked = |config: &Value, operation: Operation, path: &str| {
        Claudecode
            .prepare(PrepareContext {
                provider: provider(config, None),
                credential: credential(&secret, &Value::Null),
                operation: key(operation),
                request: WireRequest {
                    path: path.into(),
                    ..messages_request(
                        HeaderMap::new(),
                        json!({"model": "claude-fable-5-1", "messages": []}),
                    )
                },
                endpoint_override: None,
            })
            .unwrap()
            .headers()
            .get("anthropic-usage-limit")
            .map(|value| value.to_str().unwrap().to_owned())
    };
    let on = json!({"low_priority": true});
    let off = json!({});
    for operation in [Operation::GenerateContent, Operation::StreamGenerateContent] {
        assert_eq!(
            marked(&on, operation, "/v1/messages").as_deref(),
            Some("slow")
        );
        assert_eq!(marked(&off, operation, "/v1/messages"), None);
    }
    assert_eq!(
        marked(&on, Operation::CountTokens, "/v1/messages/count_tokens"),
        None,
        "the CLI marks only Messages calls"
    );

    let blocking = |config: &Value| -> Vec<(String, bool)> {
        Claudecode
            .quota_model()
            .unwrap()
            .dimensions(provider(config, None), credential(&secret, &Value::Null))
            .into_iter()
            .map(|d| (d.id, d.blocking))
            .collect()
    };
    assert!(blocking(&off).iter().all(|(_, blocks)| *blocks));
    assert_eq!(
        blocking(&on),
        vec![
            ("five_hour".into(), false),
            ("seven_day".into(), true),
            ("seven_day_fable".into(), true),
        ],
        "the server refuses low priority past the weekly limit"
    );
}

#[tokio::test]
async fn oauth_usage_is_queried_with_the_cli_identity_and_parsed() {
    let config = json!({});
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({
            "five_hour": { "utilization": 34.5, "resets_at": "2026-08-31T15:00:00Z" },
            "seven_day": { "utilization": 61.0, "resets_at": "2026-09-03T00:00:00+00:00" },
            "seven_day_opus": { "utilization": 22.0, "resets_at": "2026-09-03T00:00:00Z" },
            "seven_day_oauth_apps": { "utilization": 5, "resets_at": "2026-09-03T00:00:00Z" },
            "cinder_cove": { "utilization": null, "resets_at": null },
            "extra_usage": { "is_enabled": false },
            "limits": [
                { "kind": "weekly_all", "percent": 99.0, "resets_at": "2026-09-03T00:00:00Z" },
                { "kind": "weekly_scoped", "percent": 12.0, "resets_at": "2026-09-03T00:00:00Z",
                  "scope": { "model": { "id": "claude-opus-5", "display_name": "Claude Opus 5" } } },
                { "kind": "weekly_scoped", "percent": 3.0, "resets_at": "2026-09-03T00:00:00Z",
                  "scope": { "model": { "display_name": "Claude Sonnet" } } }
            ]
        }),
    )]);
    let s = secret("at");
    let snapshot = Claudecode
        .quota_query()
        .unwrap()
        .query(CredentialContext {
            provider: provider(&config, Some("https://mirror.example")),
            credential: credential(&s, &Value::Null),
            client: &client,
        })
        .await
        .unwrap();
    let sent = client.sent();
    assert_eq!(sent[0].0, Method::GET);
    assert_eq!(sent[0].1, "https://mirror.example/api/oauth/usage");
    assert_eq!(sent[0].2["authorization"], "Bearer at");
    assert_eq!(sent[0].2["user-agent"], CLI_USER_AGENT);
    assert_eq!(sent[0].2["anthropic-beta"], "oauth-2025-04-20");
    let ids: Vec<&str> = snapshot
        .entries
        .iter()
        .map(|e| e.source_id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec![
            "five_hour",
            "seven_day",
            "seven_day_oauth_apps",
            "seven_day_opus",
            "weekly_model:claude_opus_5",
            "seven_day_sonnet",
        ],
        "explicit nulls are not windows; weekly_all duplicates seven_day; \
         a family limit takes the family window id"
    );
    let QuotaValue::Window(five) = &snapshot.entries[0].value else {
        panic!("window");
    };
    assert_eq!(five.used_percent, Some("34.5".parse().unwrap()));
    assert_eq!(five.remaining, Some("65.5".parse().unwrap()));
    assert_eq!(five.period_end_ms, Some(1_788_188_400_000));
    assert_eq!(
        five.period_start_ms,
        Some(1_788_188_400_000 - 5 * 60 * 60 * 1000)
    );
    assert_eq!(snapshot.entries[1].model_scope, QuotaScope::All);
    let QuotaValue::Window(seven) = &snapshot.entries[1].value else {
        panic!("window");
    };
    assert_eq!(seven.used_percent, Some(61.into()));
    assert_eq!(snapshot.entries[2].model_scope, QuotaScope::Unknown);
    assert_eq!(
        snapshot.entries[3].model_scope,
        QuotaScope::ModelPrefixes(vec!["claude-opus".into()])
    );
    assert_eq!(
        snapshot.entries[4].model_scope,
        QuotaScope::Models(vec!["claude-opus-5".into()]),
        "a concrete model id does not widen to its family"
    );
    assert_eq!(snapshot.entries[4].label.as_deref(), Some("Claude Opus 5"));
    assert_eq!(
        snapshot.entries[5].model_scope,
        QuotaScope::ModelPrefixes(vec!["claude-sonnet".into()])
    );
}

#[test]
fn unified_rate_limit_headers_become_window_entries() {
    let mut headers = HeaderMap::new();
    for (name, value) in [
        ("anthropic-ratelimit-unified-status", "allowed_warning"),
        ("anthropic-ratelimit-unified-5h-utilization", "0.425"),
        ("anthropic-ratelimit-unified-5h-reset", "1700000000"),
        ("anthropic-ratelimit-unified-7d-utilization", "1.2"),
        ("anthropic-ratelimit-unified-reset", "1700400000"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    let entries = Claudecode
        .quota_headers()
        .unwrap()
        .observe(QuotaHeaderContext {
            operation: key(Operation::StreamGenerateContent),
            upstream_model: "claude-sonnet-4-6",
            status: StatusCode::OK,
            headers: &headers,
        })
        .unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].source_id, "five_hour");
    let QuotaValue::Window(five) = &entries[0].value else {
        panic!("window");
    };
    assert_eq!(five.used_percent, Some("42.5".parse().unwrap()));
    assert_eq!(five.period_end_ms, Some(1_700_000_000_000));
    assert_eq!(
        five.period_start_ms,
        Some(1_700_000_000_000 - 5 * 60 * 60 * 1000)
    );
    assert_eq!(entries[1].source_id, "seven_day");
    let QuotaValue::Window(seven) = &entries[1].value else {
        panic!("window");
    };
    assert_eq!(seven.used_percent, Some(100.into()), "clamped like the CLI");
    assert_eq!(seven.remaining, Some(0.into()));
    assert_eq!(seven.period_end_ms, None, "no 7d reset header");

    let empty = Claudecode
        .quota_headers()
        .unwrap()
        .observe(QuotaHeaderContext {
            operation: key(Operation::GenerateContent),
            upstream_model: "claude-sonnet-4-6",
            status: StatusCode::OK,
            headers: &HeaderMap::new(),
        })
        .unwrap();
    assert!(empty.is_empty());
}

// Live captures from a claude_max 5x account (2026-09-26), sanitized.
const USAGE_FIXTURE: &str = include_str!("fixtures/quota/claudecode_usage.json");
const FABLE_HEADERS: &str = include_str!("fixtures/quota/claudecode_messages_fable.headers");
const HAIKU_HEADERS: &str = include_str!("fixtures/quota/claudecode_messages_haiku.headers");
/// Observed without a declared dimension: no cost accrues to them.
const OBSERVE_ONLY: &[&str] = &["seven_day_breakdown"];

async fn captured_usage() -> Vec<gproxy_channel::channel::QuotaEntry> {
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        serde_json::from_str(USAGE_FIXTURE).unwrap(),
    )]);
    let config = json!({});
    let s = secret("at");
    Claudecode
        .quota_query()
        .unwrap()
        .query(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&s, &Value::Null),
            client: &client,
        })
        .await
        .unwrap()
        .entries
}

fn captured_headers(text: &str, model: &str) -> Vec<gproxy_channel::channel::QuotaEntry> {
    Claudecode
        .quota_headers()
        .unwrap()
        .observe(QuotaHeaderContext {
            operation: key(Operation::GenerateContent),
            upstream_model: model,
            status: StatusCode::OK,
            headers: &support::header_fixture(text),
        })
        .unwrap()
}

#[tokio::test]
async fn captured_quota_replies_keep_the_channel_contract() {
    let model = Claudecode.quota_model().unwrap();
    let declared = model.dimensions(
        provider(&json!({}), None),
        credential(
            &secret("a"),
            &json!({"rate_limit_tier": "default_claude_max_5x"}),
        ),
    );
    let usage = captured_usage().await;
    let fable = captured_headers(FABLE_HEADERS, "claude-fable-5-1");
    let haiku = captured_headers(HAIKU_HEADERS, "claude-haiku-4-5-20251001");
    for entries in [&usage, &fable, &haiku] {
        support::assert_quota_contract(Some(model), &declared, entries, OBSERVE_ONLY);
    }
    let ids = |entries: &[gproxy_channel::channel::QuotaEntry]| {
        entries.iter().map(|e| e.id.clone()).collect::<Vec<_>>()
    };
    // Null codenamed keys and `nimbus_quill` without a reset are no windows;
    // Fable appears only under `limits[]`.
    assert_eq!(
        ids(&usage),
        [
            "five_hour",
            "seven_day",
            "seven_day_breakdown",
            "seven_day_fable"
        ]
    );
    // The `7d_oi` claim only rides Fable requests.
    assert_eq!(ids(&fable), ["five_hour", "seven_day", "seven_day_fable"]);
    assert_eq!(ids(&haiku), ["five_hour", "seven_day"]);
}

#[tokio::test]
async fn the_fable_window_is_one_window_on_both_paths() {
    let usage = captured_usage().await;
    let headers = captured_headers(FABLE_HEADERS, "claude-fable-5-1");
    let find = |entries: &[gproxy_channel::channel::QuotaEntry]| {
        let entry = entries
            .iter()
            .find(|e| e.id == "seven_day_fable")
            .cloned()
            .unwrap();
        let QuotaValue::Window(window) = entry.value else {
            panic!("window");
        };
        (entry.model_scope, window.used_percent, window.period_end_ms)
    };
    let (query_scope, query_used, query_end) = find(&usage);
    let (header_scope, header_used, header_end) = find(&headers);
    assert_eq!(
        query_scope,
        QuotaScope::ModelPrefixes(vec!["claude-fable".into()])
    );
    assert_eq!(header_scope, query_scope);
    assert_eq!(query_used, Some(0.into()));
    assert_eq!(header_used, query_used);
    // 2026-09-26T11:00:00Z on both paths.
    assert_eq!(query_end, Some(1_790_420_400_000));
    assert_eq!(header_end, query_end);
}

/// The channel forwards Messages as the upstream wrote them, so a reply
/// settles with the standard reading: a server-side fallback becomes two
/// attempts, the one that fell back unbillable.
#[test]
fn messages_settle_from_sse_and_from_json() {
    let stream = concat!(
        "event: message_start\r\ndata: {\"type\":\"message_start\",\"message\":{\"model\":\"claude-fable-5\",\"usage\":{\"input_tokens\":25,\"output_tokens\":1,\"cache_read_input_tokens\":10,\"cache_creation\":{\"ephemeral_5m_input_tokens\":0,\"ephemeral_1h_input_tokens\":20}}}}\r\n\r\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"fallback\",\"from\":{\"model\":\"claude-fable-5\"},\"to\":{\"model\":\"claude-opus-4-8\"}}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":5}}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":12,\"cache_creation_input_tokens\":20,\"output_tokens_details\":{\"thinking_tokens\":4},\"iterations\":[{\"type\":\"fallback_message\",\"model\":\"claude-fable-5\",\"input_tokens\":25,\"output_tokens\":0},{\"type\":\"message\",\"input_tokens\":25,\"output_tokens\":12}]}}\n\n"
    );
    let usage = support::settled_stream(
        &Claudecode,
        Operation::StreamGenerateContent,
        Dialect::Claude,
        &HeaderMap::new(),
        stream.as_bytes(),
    )
    .unwrap();
    assert_eq!(usage.tokens.input_tokens, Some(25));
    assert_eq!(usage.tokens.output_tokens, Some(12));
    assert_eq!(usage.tokens.cached_input_tokens, Some(10));
    assert_eq!(
        usage.tokens.cache_creation_5m_tokens,
        Some(0),
        "message_start's breakdown wins over the flat delta count"
    );
    assert_eq!(usage.tokens.cache_creation_1h_tokens, Some(20));
    assert_eq!(usage.tokens.reasoning_tokens, Some(4));
    assert_eq!(usage.attempts.len(), 2);
    assert_eq!(usage.attempts[0].model, "claude-fable-5");
    assert_eq!(usage.attempts[0].billable, Some(false));
    assert_eq!(
        usage.attempts[1].model, "claude-opus-4-8",
        "fallback target"
    );
    assert_eq!(usage.attempts[1].billable, Some(true));
    assert_eq!(usage.attempts[1].usage.tokens.output_tokens, Some(12));

    let body = json!({
        "model": "claude-sonnet-4-6",
        "stop_reason": "end_turn",
        "usage": {
            "input_tokens": 10, "output_tokens": 4, "cache_read_input_tokens": 30,
            "cache_creation": {"ephemeral_5m_input_tokens": 2, "ephemeral_1h_input_tokens": 3},
            "output_tokens_details": {"thinking_tokens": 1},
            "server_tool_use": {"web_search_requests": 2, "web_fetch_requests": 1},
            "service_tier": "standard", "speed": "fast"
        }
    })
    .to_string();
    let extracted = support::settled(
        &Claudecode,
        Operation::GenerateContent,
        Dialect::Claude,
        &HeaderMap::new(),
        body.as_bytes(),
    )
    .unwrap();
    assert_eq!(extracted.tokens.input_tokens, Some(10));
    assert_eq!(extracted.tokens.output_tokens, Some(4));
    assert_eq!(extracted.tokens.cached_input_tokens, Some(30));
    assert_eq!(extracted.tokens.cache_creation_5m_tokens, Some(2));
    assert_eq!(extracted.tokens.cache_creation_1h_tokens, Some(3));
    assert_eq!(extracted.tokens.reasoning_tokens, Some(1));
    assert_eq!(extracted.metrics["web_searches"], 2.into());
    assert_eq!(extracted.metrics["web_fetches"], 1.into());
    assert_eq!(extracted.dimensions["speed"], "fast");
    assert_eq!(extracted.actual_service_tier.as_deref(), Some("standard"));
    assert!(extracted.attempts.is_empty());

    assert!(
        support::settled(
            &Claudecode,
            Operation::GenerateContent,
            Dialect::Claude,
            &HeaderMap::new(),
            br#"{"id":"msg"}"#,
        )
        .is_none()
    );
}

// ------------------------------------------------------------ services

fn service_request(method: Method, path: &str, query: Option<&str>, body: &str) -> WireRequest {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", HeaderValue::from_static("Bearer client"));
    headers.insert("cookie", HeaderValue::from_static("sessionKey=leaked"));
    headers.insert("x-api-key", HeaderValue::from_static("sk-leaked"));
    headers.insert("cache-control", HeaderValue::from_static("no-cache"));
    headers.insert("x-organization-uuid", HeaderValue::from_static("org-1"));
    headers.insert(
        "user-agent",
        HeaderValue::from_static("claude-cli/2.1.280 (external, sdk-cli)"),
    );
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
    client: &'a ScriptClient,
) -> CredentialContext<'a> {
    CredentialContext {
        provider: provider(config, None),
        credential: credential(secret, &Value::Null),
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

#[tokio::test]
async fn catalog_routes_forward_under_every_view_with_the_cli_identity() {
    let config = json!({});
    let s = secret("at");
    let client = ScriptClient::new(vec![
        reply(StatusCode::OK, json!({})),
        reply(StatusCode::OK, json!({})),
        reply(StatusCode::NOT_FOUND, json!({"type": "error"})),
    ]);
    let accounts = [account(&config, &s, &client)];
    let member = ScriptCaller::member("m");
    let admin = ScriptCaller::admin("a");
    let services = Claudecode.services().expect("claudecode exposes services");
    for (view, caller) in [
        ServiceView::Caller,
        ServiceView::Pool,
        ServiceView::Credential("c".into()),
    ]
    .into_iter()
    .zip([&member, &admin, &admin])
    {
        services
            .call(context(
                &accounts,
                caller,
                view,
                service_request(Method::GET, "/api/hello", Some("key=leaked&x=1"), ""),
            ))
            .await
            .unwrap();
    }
    let sent = client.sent();
    assert_eq!(sent.len(), 3);
    assert_eq!(sent[0].1, "https://api.anthropic.com/api/hello?x=1");
    let h = &sent[0].2;
    assert_eq!(h["authorization"], "Bearer at");
    assert_eq!(h["anthropic-beta"], "oauth-2025-04-20");
    assert_eq!(h["anthropic-version"], "2023-06-01");
    assert_eq!(h["user-agent"], "claude-cli/2.1.280 (external, sdk-cli)");
    assert_eq!(h["x-app"], "cli");
    assert_eq!(h["cache-control"], "no-cache");
    assert_eq!(h["x-organization-uuid"], "org-1");
    assert!(h.get("cookie").is_none(), "claude.ai cookies never leave");
    assert!(h.get("x-api-key").is_none());
    assert_eq!(h.get_all("authorization").iter().count(), 1);
}

#[tokio::test]
async fn identity_is_synthesized_unless_the_view_is_a_credential() {
    let config = json!({});
    let s = secret("at");
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"account": {"uuid": "real"}}),
    )]);
    let accounts = [account(&config, &s, &client)];
    let member = ScriptCaller::member("m");
    let admin = ScriptCaller::admin("a");
    let services = Claudecode.services().unwrap();
    let profile = |caller, view| {
        services.call(context(
            &accounts,
            caller,
            view,
            service_request(Method::GET, "/api/oauth/profile", None, ""),
        ))
    };
    let mine = body_json(profile(&member, ServiceView::Caller).await.unwrap()).await;
    assert_eq!(mine["account"]["email"], "m@gproxy.invalid");
    assert_eq!(mine["organization"]["name"], "cc");
    let uuid = mine["account"]["uuid"].as_str().unwrap().to_owned();
    assert!(uuid.starts_with("gproxy-account-"));
    let pool = body_json(profile(&admin, ServiceView::Pool).await.unwrap()).await;
    assert_ne!(pool["account"]["uuid"], uuid);
    assert!(client.sent().is_empty());

    let roles = body_json(
        services
            .call(context(
                &accounts,
                &admin,
                ServiceView::Pool,
                service_request(Method::GET, "/api/oauth/claude_cli/roles", None, ""),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(roles["organization_role"], "admin");

    let bootstrap = body_json(
        services
            .call(context(
                &accounts,
                &member,
                ServiceView::Caller,
                service_request(Method::GET, "/api/claude_cli/bootstrap", None, ""),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(bootstrap["oauth_account"]["account_uuid"], uuid);

    let real = body_json(
        profile(&admin, ServiceView::Credential("c".into()))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(real["account"]["uuid"], "real");
    assert_eq!(
        client.sent()[0].1,
        "https://api.anthropic.com/api/oauth/profile"
    );
}

#[tokio::test]
async fn usage_reflects_the_host_allotment() {
    let config = json!({});
    let s = secret("at");
    let client = ScriptClient::new(vec![reply(StatusCode::OK, json!({}))]);
    let accounts = [account(&config, &s, &client)];
    let allotted = ScriptCaller::member("m").with_usage(CallerUsage {
        windows: vec![CallerUsageWindow {
            key: "5h".into(),
            used_percent: Some(34.5),
            period_start_ms: None,
            reset_at_ms: Some(1_788_188_400_000),
        }],
        ..CallerUsage::default()
    });
    let bare = ScriptCaller::admin("a");
    let services = Claudecode.services().unwrap();
    let usage = |caller, view| {
        services.call(context(
            &accounts,
            caller,
            view,
            service_request(Method::GET, "/api/oauth/usage", None, ""),
        ))
    };
    let value = body_json(usage(&allotted, ServiceView::Caller).await.unwrap()).await;
    assert_eq!(value["five_hour"]["utilization"], 34.5);
    assert_eq!(value["five_hour"]["resets_at"], "2026-08-31T15:00:00Z");
    assert!(value.get("seven_day").is_none());
    assert_eq!(value["extra_usage"]["is_enabled"], false);
    let value = body_json(usage(&bare, ServiceView::Pool).await.unwrap()).await;
    assert!(
        value.get("five_hour").is_none(),
        "no windows: no window fields"
    );
    assert!(client.sent().is_empty());
    usage(&bare, ServiceView::Credential("c".into()))
        .await
        .unwrap();
    assert_eq!(
        client.sent()[0].1,
        "https://api.anthropic.com/api/oauth/usage"
    );
}

#[tokio::test]
async fn settings_are_neutral_and_policy_limits_are_unrestricted() {
    let config = json!({"claudecode_fast_mode": {"enabled": true}});
    let s = secret("at");
    let client = ScriptClient::new(vec![reply(StatusCode::OK, json!({}))]);
    let accounts = [account(&config, &s, &client)];
    let member = ScriptCaller::member("m");
    let services = Claudecode.services().unwrap();
    let limits = services
        .call(context(
            &accounts,
            &member,
            ServiceView::Caller,
            service_request(Method::GET, "/api/claude_code/policy_limits", None, ""),
        ))
        .await
        .unwrap();
    assert_eq!(limits.status, StatusCode::NOT_FOUND);
    assert_eq!(body_json(limits).await["error"]["type"], "not_found_error");
    let fast = body_json(
        services
            .call(context(
                &accounts,
                &member,
                ServiceView::Pool,
                service_request(Method::GET, "/api/claude_code_penguin_mode", None, ""),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        fast["enabled"], true,
        "provider config overrides the default"
    );
    services
        .call(context(
            &accounts,
            &member,
            ServiceView::Credential("c".into()),
            service_request(Method::GET, "/api/claude_code/policy_limits", None, ""),
        ))
        .await
        .unwrap();
    assert_eq!(client.sent().len(), 1);
}

#[tokio::test]
async fn resources_are_bound_on_creation_and_listed_from_bindings() {
    let config = json!({"claudecode_shared_skills": [{"id": "shared", "name": "Shared deploy"}]});
    let s = secret("at");
    let client = ScriptClient::new(vec![
        raw_reply(StatusCode::CREATED, br#"{"file_uuid":"f1"}"#.to_vec()),
        raw_reply(StatusCode::OK, b"bytes".to_vec()),
        raw_reply(StatusCode::OK, b"PK".to_vec()),
    ]);
    let accounts = [account(&config, &s, &client)];
    let caller = ScriptCaller::member("m").with_binding(
        KIND_SKILL,
        "s1",
        "c",
        json!({"id": "s1", "name": "Deploy helper", "organization_uuid": "real-org"}),
    );
    let services = Claudecode.services().unwrap();

    let created = services
        .call(context(
            &accounts,
            &caller,
            ServiceView::Caller,
            service_request(Method::POST, "/api/oauth/file_upload", None, "multipart"),
        ))
        .await
        .unwrap();
    assert_eq!(created.status, StatusCode::CREATED);
    let bound = caller.bound();
    let file = bound.iter().find(|b| b.kind == KIND_FILE).unwrap();
    assert_eq!(file.upstream_id, "f1");
    assert_eq!(file.credential_id, "c");

    services
        .call(context(
            &accounts,
            &caller,
            ServiceView::Caller,
            service_request(Method::GET, "/api/oauth/files/f1/content", None, ""),
        ))
        .await
        .unwrap();
    assert_eq!(
        client.sent()[1].1,
        "https://api.anthropic.com/api/oauth/files/f1/content"
    );
    let foreign = services
        .call(context(
            &accounts,
            &caller,
            ServiceView::Caller,
            service_request(Method::GET, "/api/oauth/files/f9/content", None, ""),
        ))
        .await
        .unwrap();
    assert_eq!(foreign.status, StatusCode::NOT_FOUND);
    assert_eq!(client.sent().len(), 2);

    let skills = body_json(
        services
            .call(context(
                &accounts,
                &caller,
                ServiceView::Pool,
                service_request(
                    Method::GET,
                    "/api/oauth/organizations/any-org/skills/list-skills",
                    None,
                    "",
                ),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(
        skills["skills"].as_array().unwrap().len(),
        2,
        "bindings plus shared config"
    );
    let found = body_json(
        services
            .call(context(
                &accounts,
                &caller,
                ServiceView::Caller,
                service_request(
                    Method::POST,
                    "/api/oauth/organizations/any-org/skills/search",
                    None,
                    r#"{"keywords":["shared"]}"#,
                ),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(found["results"].as_array().unwrap().len(), 1);
    assert_eq!(found["results"][0]["id"], "shared");

    services
        .call(context(
            &accounts,
            &caller,
            ServiceView::Caller,
            service_request(
                Method::GET,
                "/api/oauth/organizations/synthetic-org/skills/s1/download",
                None,
                "",
            ),
        ))
        .await
        .unwrap();
    assert_eq!(
        client.sent()[2].1,
        "https://api.anthropic.com/api/oauth/organizations/real-org/skills/s1/download",
        "the item route follows the organization recorded in the binding"
    );
}

#[tokio::test]
async fn key_minting_is_refused_everywhere_and_other_classes_follow_the_view() {
    let config = json!({});
    let s = secret("at");
    let client = ScriptClient::new(vec![
        reply(StatusCode::OK, json!({})),
        reply(StatusCode::OK, json!({})),
        reply(StatusCode::OK, json!({})),
    ]);
    let accounts = [account(&config, &s, &client)];
    let admin = ScriptCaller::admin("a");
    let services = Claudecode.services().unwrap();
    for view in [
        ServiceView::Caller,
        ServiceView::Pool,
        ServiceView::Credential("c".into()),
    ] {
        let response = services
            .call(context(
                &accounts,
                &admin,
                view,
                service_request(
                    Method::POST,
                    "/api/oauth/claude_cli/create_api_key",
                    None,
                    "null",
                ),
            ))
            .await
            .unwrap();
        assert_eq!(response.status, StatusCode::FORBIDDEN);
        assert_eq!(
            body_json(response).await["error"]["type"],
            "permission_error"
        );
    }
    let routes = [
        (
            Method::GET,
            "/api/oauth/organizations/o/payment_method",
            StatusCode::FORBIDDEN,
        ),
        (Method::POST, "/api/claude_code/metrics", StatusCode::OK),
        (
            Method::GET,
            "/api/oauth/brand_new_in_a_later_cli",
            StatusCode::NOT_FOUND,
        ),
    ];
    for (method, path, expected) in &routes {
        let response = services
            .call(context(
                &accounts,
                &admin,
                ServiceView::Pool,
                service_request(method.clone(), path, None, "{}"),
            ))
            .await
            .unwrap();
        assert_eq!(response.status, *expected, "{path}");
    }
    assert!(client.sent().is_empty(), "nothing left the gateway");
    for (method, path, _) in &routes {
        services
            .call(context(
                &accounts,
                &admin,
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
            &admin,
            ServiceView::Credential("c".into()),
            service_request(Method::POST, "/v1/messages", None, "{}"),
        ))
        .await
        .unwrap_err();
    assert!(matches!(error, ChannelError::UnsupportedService));
}

// ----------------------------------------------------------- magic cache

const MAGIC_AUTO: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH";
const MAGIC_5M: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF";
const MAGIC_1H: &str =
    "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_1FAS5GV9R5H29T5Y2J9584K6O95M2NBVW52C95CX984FRJY";
const MAGIC_PREFIX: &str = "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_";

#[test]
fn magic_cache_strings_place_cache_control_only_when_enabled() {
    let secret = secret("at-1");
    let shaped = |config: &Value, operation: Operation, body: Vec<u8>| -> Vec<u8> {
        let request = Claudecode
            .prepare(PrepareContext {
                provider: provider(config, None),
                credential: credential(&secret, &Value::Null),
                operation: key(operation),
                request: WireRequest {
                    method: Method::POST,
                    path: "/v1/messages".into(),
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
    let body = |system_token: &str, user_token: &str| {
        json!({
            "model": "claude-sonnet-4-6",
            "system": format!("policy {system_token}"),
            "messages": [
                {"role": "user", "content": [
                    {"type": "text", "text": "pinned", "cache_control": {"type": "ephemeral"}},
                    {"type": "text", "text": format!("hello {user_token}")}
                ]}
            ]
        })
        .to_string()
        .into_bytes()
    };
    let enabled = json!({"enable_claude_magic_cache": true});
    let disabled = json!({});

    let bytes = shaped(
        &enabled,
        Operation::GenerateContent,
        body(MAGIC_1H, MAGIC_AUTO),
    );
    assert!(!String::from_utf8_lossy(&bytes).contains(MAGIC_PREFIX));
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        value["system"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("x-anthropic-billing-header:")
    );
    assert_eq!(
        value["system"][1],
        json!({"type": "text", "text": "policy", "cache_control": {"type": "ephemeral", "ttl": "1h"}}),
        "the string system is canonicalized and marked with the token's ttl"
    );
    assert_eq!(
        value["messages"][0]["content"],
        json!([
            {"type": "text", "text": "pinned", "cache_control": {"type": "ephemeral"}},
            {"type": "text", "text": "hello", "cache_control": {"type": "ephemeral"}}
        ])
    );

    let with_tokens = shaped(
        &disabled,
        Operation::GenerateContent,
        body(MAGIC_1H, MAGIC_AUTO),
    );
    let without = shaped(&disabled, Operation::GenerateContent, body("", ""));
    assert!(!String::from_utf8_lossy(&with_tokens).contains(MAGIC_PREFIX));
    assert_eq!(
        with_tokens, without,
        "disabled: tokens stripped, nothing else changes"
    );
    let value: Value = serde_json::from_slice(&with_tokens).unwrap();
    assert_eq!(
        value["messages"][0]["content"][0]["cache_control"],
        json!({"type": "ephemeral"}),
        "the client's own breakpoint stays"
    );
    assert!(
        value["messages"][0]["content"][1]
            .get("cache_control")
            .is_none()
    );

    let count = format!(
        r#"{{"model":"claude-sonnet-4-6","messages":[{{"role":"user","content":"count {MAGIC_5M}"}}]}}"#
    );
    let bytes = shaped(&enabled, Operation::CountTokens, count.clone().into_bytes());
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        value["messages"][0]["content"],
        json!([{"type": "text", "text": "count", "cache_control": {"type": "ephemeral", "ttl": "5m"}}]),
        "count_tokens bodies take breakpoints too"
    );
    let bytes = shaped(&disabled, Operation::CountTokens, count.into_bytes());
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap(),
        json!({"model": "claude-sonnet-4-6", "messages": [{"role": "user", "content": "count "}]}),
        "disabled count_tokens: stripped, not canonicalized"
    );
    let plain = br#"{"model":"claude-sonnet-4-6",  "messages":[]}"#.to_vec();
    assert_eq!(
        shaped(&enabled, Operation::CountTokens, plain.clone()),
        plain,
        "no token, no rewrite, even with magic cache enabled"
    );
}

fn reset_eligibility() -> Vec<WireResponse> {
    vec![
        reply(
            StatusCode::OK,
            json!({"juniper_tide": {"eligible": true, "arm": "reset", "available": true}}),
        ),
        reply(
            StatusCode::OK,
            json!({"cedar_ember": {
                "eligible": true, "at_limit": false, "next_grant_id": "gift-1",
                "grants": [{"id": "gift-1", "label": "Gift reset", "resets_left": 2,
                    "usable_now": true, "use_requires_limit": false, "clears": ["five_hour", "seven_day"], "ends_at": "2099-01-01T00:00:00Z"}]
            }}),
        ),
    ]
}

#[tokio::test]
async fn reset_eligibility_is_separate_and_ineligible_counts_stay_unknown() {
    assert!(Claudecode.descriptor().capabilities.quota_reset);
    let client = ScriptClient::new(vec![
        reply(
            StatusCode::OK,
            json!({"juniper_tide": {"eligible": false, "ineligible_reason": "other_experiment"}}),
        ),
        reply(
            StatusCode::OK,
            json!({"cedar_ember": {"eligible": false, "ineligible_reason": "cli_version", "grants": []}}),
        ),
    ]);
    let config = json!({});
    let s = secret("at");
    let credits = Claudecode
        .quota_reset()
        .unwrap()
        .credits(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&s, &Value::Null),
            client: &client,
        })
        .await
        .unwrap();
    assert_eq!(credits.available_count, None);
    assert_eq!(credits.options.len(), 2);
    assert!(
        credits
            .options
            .iter()
            .all(|option| !option.usable && option.available_count.is_none())
    );
    assert_eq!(
        credits.options[1].ineligible_reason.as_deref(),
        Some("cli_version")
    );
    let sent = client.sent();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0].0, Method::GET);
    assert!(
        sent[0]
            .1
            .ends_with("/api/oauth/usage?at_wall=1&skip_spend=1")
    );
    assert!(
        sent[1]
            .1
            .ends_with("/api/oauth/usage?cedar_ember=1&skip_spend=1")
    );
    assert_eq!(sent[0].2["authorization"], "Bearer at");
    assert_eq!(sent[0].2["user-agent"], CLI_USER_AGENT);
}

#[tokio::test]
async fn reset_revalidates_the_selected_program_and_never_switches_grants() {
    use gproxy_channel::channel::{QuotaResetOutcome, QuotaResetRequest};
    for (program, grant_id) in [
        ("juniper_tide", None),
        ("cedar_ember", Some("gift-1")),
        ("cedar_ember", Some("different-grant")),
    ] {
        let mut replies = reset_eligibility();
        let allowed = grant_id != Some("different-grant");
        if allowed {
            replies.push(reply(
                StatusCode::OK,
                json!({"organization": {"uuid": "org-123"}}),
            ));
            replies.push(reply(
                StatusCode::OK,
                json!({"result": "reset", "cleared": ["five_hour"]}),
            ));
        }
        let client = ScriptClient::new(replies);
        let config = json!({});
        let s = secret("at");
        let result = Claudecode
            .quota_reset()
            .unwrap()
            .reset(
                CredentialContext {
                    provider: provider(&config, Some("https://claude.example")),
                    credential: credential(&s, &Value::Null),
                    client: &client,
                },
                QuotaResetRequest {
                    redeem_request_id: "same-request-123",
                    program: Some(program),
                    grant_id,
                },
            )
            .await
            .unwrap();
        let sent = client.sent();
        if !allowed {
            assert_eq!(result.outcome, QuotaResetOutcome::Ineligible);
            assert!(sent.iter().all(|request| request.0 == Method::GET));
        } else {
            assert_eq!(result.outcome, QuotaResetOutcome::Reset);
            assert_eq!(result.windows_reset, Some(1));
            assert_eq!(sent[3].0, Method::POST);
            assert_eq!(
                sent[3].1,
                "https://claude.example/api/organizations/org-123/reset_rate_limits"
            );
            let body: Value = serde_json::from_slice(&sent[3].3).unwrap();
            let expected = if program == "cedar_ember" {
                json!({"program": program, "grant_id": "gift-1", "request_id": "same-request-123"})
            } else {
                json!({"program": program})
            };
            assert_eq!(body, expected);
        }
    }
}

#[tokio::test]
async fn reset_does_not_consume_paused_expired_or_not_yet_usable_grants() {
    use gproxy_channel::channel::{QuotaResetOutcome, QuotaResetRequest};
    for patch in [
        json!({"paused": true}),
        json!({"ends_at": "2000-01-01T00:00:00Z"}),
        json!({"use_requires_limit": true}),
        json!({"blocking": ["seven_day_opus"]}),
    ] {
        let mut grant = json!({"id": "gift-1", "resets_left": 2, "usable_now": true, "use_requires_limit": false, "clears": ["five_hour"]});
        for (key, value) in patch.as_object().unwrap() {
            grant[key] = value.clone();
        }
        let client = ScriptClient::new(vec![
            reply(StatusCode::OK, json!({})),
            reply(
                StatusCode::OK,
                json!({"cedar_ember": {"eligible": true, "next_grant_id": "gift-1", "at_limit": false, "grants": [grant]}}),
            ),
        ]);
        let config = json!({});
        let s = secret("at");
        let result = Claudecode
            .quota_reset()
            .unwrap()
            .reset(
                CredentialContext {
                    provider: provider(&config, None),
                    credential: credential(&s, &Value::Null),
                    client: &client,
                },
                QuotaResetRequest {
                    redeem_request_id: "test-request",
                    program: Some("cedar_ember"),
                    grant_id: Some("gift-1"),
                },
            )
            .await
            .unwrap();
        assert_eq!(result.outcome, QuotaResetOutcome::Ineligible);
        assert_eq!(client.sent().len(), 2);
        assert!(client.sent().iter().all(|request| request.0 == Method::GET));
    }
}

#[tokio::test]
async fn weekly_breakdown_is_composition_not_an_allowance() {
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({
            "seven_day": {"utilization": 22, "resets_at": "2026-09-26T11:00:00Z"},
            "seven_day_breakdown": {"rows": [
                {"key": "claude_code", "display_name": "Claude Code", "percent": 70},
                {"key": "chat", "display_name": "Chats", "percent": 20},
                {"key": "cowork", "display_name": "Cowork", "percent": 10},
                {"key": "other", "display_name": "Other", "percent": 0}
            ]},
            "limits": [{"kind": "weekly_scoped", "percent": 0, "resets_at": "2026-09-26T11:00:00Z", "scope": {"model": {"display_name": "Fable"}}}]
        }),
    )]);
    let config = json!({});
    let s = secret("at");
    let result = Claudecode
        .quota_query()
        .unwrap()
        .query(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&s, &Value::Null),
            client: &client,
        })
        .await
        .unwrap();
    let QuotaValue::Breakdown(rows) = &result.entries[1].value else {
        panic!("breakdown must not be a quota window")
    };
    assert_eq!(rows.len(), 4);
    assert_eq!(
        rows.iter()
            .map(|row| row.percent)
            .sum::<rust_decimal::Decimal>(),
        100.into()
    );
    assert_eq!(rows[3].percent, 0.into());
    assert_eq!(result.entries[2].id, "seven_day_fable");
    assert_eq!(result.entries[2].label.as_deref(), Some("Fable"));
}

#[test]
fn configured_fallbacks_shape_messages_and_preserve_credit_replays() {
    let descriptor = Claudecode.descriptor();
    for name in ["fallback_mode", "fallback_models"] {
        assert!(descriptor.config_keys.iter().any(|key| key.name == name));
    }
    let config = json!({
        "fallback_mode": "models",
        "fallback_models": [" ", "claude-fable-5", "claude-opus-4-8",
            "claude-opus-4-8", "claude-sonnet-4-6", "claude-haiku-4-5", "extra"]
    });
    for operation in [Operation::GenerateContent, Operation::StreamGenerateContent] {
        for (settings, extra, expected, beta) in [
            (
                config.clone(),
                json!({}),
                json!([
                    {"model":"claude-opus-4-8"},
                    {"model":"claude-sonnet-4-6"},
                    {"model":"claude-haiku-4-5"}
                ]),
                Some("server-side-fallback-2026-06-01"),
            ),
            (
                json!({"fallback_mode":"default"}),
                json!({}),
                json!("default"),
                Some("server-side-fallback-2026-07-01"),
            ),
            (
                json!({"fallback_mode":"models","fallback_models":[]}),
                json!({}),
                json!("default"),
                Some("server-side-fallback-2026-07-01"),
            ),
            (
                config.clone(),
                json!({"fallbacks":"default"}),
                json!("default"),
                Some("server-side-fallback-2026-07-01"),
            ),
            (
                config.clone(),
                json!({"fallbacks":[{"model":"client-model"}]}),
                json!([{"model":"client-model"}]),
                Some("server-side-fallback-2026-06-01"),
            ),
            (
                config.clone(),
                json!({"fallbacks":null}),
                json!([
                    {"model":"claude-opus-4-8"},
                    {"model":"claude-sonnet-4-6"},
                    {"model":"claude-haiku-4-5"}
                ]),
                Some("server-side-fallback-2026-06-01"),
            ),
            (json!({}), json!({}), Value::Null, None),
            (
                config.clone(),
                json!({"model":"claude-opus-4-8"}),
                Value::Null,
                None,
            ),
            (
                config.clone(),
                json!({"fallback_credit_token":"credit"}),
                Value::Null,
                None,
            ),
        ] {
            let mut body = json!({"model":"claude-fable-5","messages":[]});
            body.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let original = body.clone();
            let secret = secret("at-1");
            let request = Claudecode
                .prepare(PrepareContext {
                    provider: provider(&settings, None),
                    credential: credential(&secret, &json!({})),
                    operation: key(operation),
                    request: messages_request(HeaderMap::new(), body),
                    endpoint_override: None,
                })
                .unwrap();
            let betas = request.headers()["anthropic-beta"].to_str().unwrap();
            assert!(betas.contains("oauth-2025-04-20"));
            if let Some(beta) = beta {
                assert!(betas.contains(beta), "{betas}");
                let other = if beta.ends_with("06-01") {
                    "server-side-fallback-2026-07-01"
                } else {
                    "server-side-fallback-2026-06-01"
                };
                assert!(!betas.contains(other));
            } else {
                assert!(!betas.contains("server-side-fallback"));
            }
            let HttpBody::Bytes(bytes) = request.into_body() else {
                panic!("expected bytes")
            };
            let shaped: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(shaped["fallbacks"], expected);
            if original.get("fallback_credit_token").is_some() {
                assert_eq!(shaped, original);
            }
        }
    }
}
