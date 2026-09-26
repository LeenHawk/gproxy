#![cfg(feature = "geminicli")]
//! Gemini CLI against a scripted client: no real upstream is called.

mod support;

use gproxy_channel::channel::{
    AuthorizationCode, AuthorizationRequest, BaseChannel, ChannelError, CredentialContext,
    CredentialRefresh, CredentialView, LoginContext, NoState, OperationContext, PrepareContext,
    ProviderView, QuotaScope, QuotaValue, ResponseView, UsageContext, UsageFrame,
    UsageStreamContext, UsageStreamEnd, UsageTransport,
};
use gproxy_channel::channels::geminicli::{
    CLI_USER_AGENT, DEFAULT_CLIENT_ID, DEFAULT_REDIRECT_URI, GOOG_API_CLIENT, GeminiCli,
};
use gproxy_channel::{ChannelDescriptor, LoginMode, OutboundClient};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    capability::{CapabilityError, CapabilityFuture},
    connection::{Bytes, StreamFraming},
};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
};

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
        id: "gc",
        channel: "geminicli",
        base_url,
        config,
    }
}

fn credential<'a>(secret: &'a Value, metadata: &'a Value) -> CredentialView<'a> {
    CredentialView {
        id: "c",
        provider_id: "gc",
        auth_kind: "oauth",
        secret,
        metadata,
        version: 3,
        expires_at_ms: None,
    }
}

fn secret() -> Value {
    json!({
        "access_token": "ya29.token",
        "refresh_token": "1//refresh",
        "token_type": "Bearer",
        "scopes": ["https://www.googleapis.com/auth/cloud-platform"],
        "provider_fields": {"project_id": "proj-secret", "rate_limit_tier": "pro"},
    })
}

fn request(headers: HeaderMap, path: &str, body: Value) -> WireRequest<HttpBody> {
    WireRequest {
        method: Method::POST,
        path: path.into(),
        query: Some("key=leaked&alt=sse".into()),
        headers,
        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&body).unwrap())),
    }
}

fn prepare(
    config: &Value,
    credential_view: CredentialView<'_>,
    operation: Operation,
    request: WireRequest<HttpBody>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    GeminiCli.prepare(PrepareContext {
        provider: provider(config, None),
        credential: credential_view,
        operation: OperationKey {
            operation,
            dialect: Dialect::Gemini,
        },
        request,
        endpoint_override: None,
    })
}

fn body_json(request: &http::Request<HttpBody>) -> Value {
    match request.body() {
        HttpBody::Bytes(bytes) => serde_json::from_slice(bytes).expect("JSON body"),
        HttpBody::Stream(_) => panic!("expected a buffered body"),
    }
}

async fn collect(body: HttpBody) -> Vec<u8> {
    match body {
        HttpBody::Bytes(bytes) => bytes.to_vec(),
        HttpBody::Stream(mut stream) => {
            use futures_util::StreamExt;
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                out.extend_from_slice(&chunk.expect("stream chunk"));
            }
            out
        }
    }
}

// ------------------------------------------------------------- descriptor

#[test]
fn descriptor_declares_the_login_and_the_keys_it_reads() {
    let descriptor: ChannelDescriptor = GeminiCli.descriptor();
    assert_eq!(descriptor.id, "geminicli");
    assert_eq!(descriptor.login_modes, vec![LoginMode::AuthorizationCode]);
    assert!(descriptor.capabilities.refresh);
    assert!(descriptor.capabilities.quota_query);
    assert!(!descriptor.capabilities.websocket);
    for key in ["base_url", "allowed_headers"] {
        assert!(descriptor.config_key(key).is_some(), "missing key {key}");
    }
    assert_eq!(
        GeminiCli.native_dialects(provider(&Value::Null, None), Operation::GenerateContent),
        vec![Dialect::Gemini]
    );
    // A plain HTTPS client would not be worth impersonating; the CLI's Node
    // stack has its own ClientHello, so the channel names one.
    assert!(GeminiCli.default_connection().is_some());
}

// ----------------------------------------------------------------- prepare

#[test]
fn prepare_targets_code_assist_and_injects_the_bearer_token() {
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    let prepared = prepare(
        &config,
        credential(&secret, &metadata),
        Operation::StreamGenerateContent,
        request(
            HeaderMap::new(),
            "/v1beta/models/gemini-3-pro:streamGenerateContent",
            json!({"contents": [{"parts": [{"text": "hi"}]}]}),
        ),
    )
    .expect("prepared");
    assert_eq!(
        prepared.uri().to_string(),
        "https://cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse"
    );
    assert_eq!(prepared.method(), Method::POST);
    assert_eq!(
        prepared.headers()["authorization"],
        HeaderValue::from_static("Bearer ya29.token")
    );
    // The client's `key=` credential never reaches Google.
    assert!(!prepared.uri().to_string().contains("leaked"));
}

#[test]
fn prepare_sends_the_cli_identity_and_a_client_cannot_spoof_it() {
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    let mut headers = HeaderMap::new();
    headers.insert("user-agent", HeaderValue::from_static("curl/8.0"));
    headers.insert("x-goog-api-client", HeaderValue::from_static("gl-python/3"));
    headers.insert("accept", HeaderValue::from_static("text/plain"));
    headers.insert("cookie", HeaderValue::from_static("SID=stolen"));
    let prepared = prepare(
        &config,
        credential(&secret, &metadata),
        Operation::GenerateContent,
        request(
            headers,
            "/v1beta/models/gemini-3-pro:generateContent",
            json!({"contents": []}),
        ),
    )
    .expect("prepared");
    let sent = prepared.headers();
    // The CLI appends the model it is about to call to its version.
    assert_eq!(
        sent["user-agent"],
        HeaderValue::from_str(&CLI_USER_AGENT.replacen(
            "GeminiCLI-tui/0.55.1 ",
            "GeminiCLI-tui/0.55.1/gemini-3-pro ",
            1
        ))
        .unwrap()
    );
    assert_eq!(
        sent["x-goog-api-client"],
        HeaderValue::from_static(GOOG_API_CLIENT)
    );
    assert_eq!(sent["accept"], HeaderValue::from_static("*/*"));
    assert_eq!(sent["content-type"], "application/json");
    assert!(!sent.contains_key("cookie"));
}

#[test]
fn a_provider_allowlist_keeps_the_client_headers_it_names() {
    let config = json!({"allowed_headers": ["x-goog-request-params"]});
    let secret = secret();
    let metadata = Value::Null;
    let mut headers = HeaderMap::new();
    headers.insert("x-goog-request-params", HeaderValue::from_static("a=b"));
    headers.insert("x-not-allowed", HeaderValue::from_static("c"));
    let prepared = prepare(
        &config,
        credential(&secret, &metadata),
        Operation::GenerateContent,
        request(
            headers,
            "/v1beta/models/gemini-3-pro:generateContent",
            json!({"contents": []}),
        ),
    )
    .expect("prepared");
    assert_eq!(prepared.headers()["x-goog-request-params"], "a=b");
    assert!(!prepared.headers().contains_key("x-not-allowed"));
    // The channel's own identity survives the allow-list.
    assert!(prepared.headers().contains_key("x-goog-api-client"));
}

// ------------------------------------------------------------ body shaping

#[test]
fn the_gemini_body_is_wrapped_in_the_code_assist_envelope() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({"project_id": "proj-metadata"});
    let prepared = prepare(
        &config,
        credential(&secret, &metadata),
        Operation::GenerateContent,
        request(
            HeaderMap::new(),
            "/v1beta/models/gemini-3-pro:generateContent",
            json!({
                "contents": [{"parts": [{"text": "hi"}]}],
                "store": true,
                "generationConfig": {"temperature": 0.5, "maxOutputTokens": 99},
                "tools": [{"googleSearch": {}}, {"functionDeclarations": [{"name": "f"}]}],
            }),
        ),
    )
    .expect("prepared");
    let body = body_json(&prepared);
    assert_eq!(body["model"], "gemini-3-pro");
    // Read from the host-persisted metadata, never rediscovered.
    assert_eq!(body["project"], "proj-metadata");
    assert!(
        body["user_prompt_id"]
            .as_str()
            .is_some_and(|id| id.len() == 32)
    );
    // Options Code Assist rejects once the body is nested.
    assert!(body["request"].get("store").is_none());
    assert!(
        body["request"]["generationConfig"]
            .get("maxOutputTokens")
            .is_none()
    );
    assert_eq!(body["request"]["generationConfig"]["temperature"], 0.5);
    // A tool list that declares functions keeps only those entries.
    assert_eq!(body["request"]["tools"].as_array().unwrap().len(), 1);
    // Code Assist rejects a content without an explicit role.
    assert_eq!(body["request"]["contents"][0]["role"], "user");
}

#[test]
fn the_session_id_the_gemini_cli_sends_survives_unrenamed() {
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    // A plain Gemini body carrying the CLI's own field: wrapping puts it
    // exactly where Code Assist expects it.
    let prepared = prepare(
        &config,
        credential(&secret, &metadata),
        Operation::GenerateContent,
        request(
            HeaderMap::new(),
            "/v1beta/models/gemini-3-pro:generateContent",
            json!({"contents": [], "session_id": "sess-1"}),
        ),
    )
    .expect("prepared");
    assert_eq!(body_json(&prepared)["request"]["session_id"], "sess-1");

    // A client that already built the envelope keeps its own `request`.
    let prepared = prepare(
        &config,
        credential(&secret, &metadata),
        Operation::GenerateContent,
        request(
            HeaderMap::new(),
            "/v1beta/models/gemini-3-pro:generateContent",
            json!({
                "model": "ignored",
                "user_prompt_id": "kept-prompt",
                "request": {"contents": [], "session_id": "sess-2"},
            }),
        ),
    )
    .expect("prepared");
    let body = body_json(&prepared);
    assert_eq!(body["request"]["session_id"], "sess-2");
    assert_eq!(body["user_prompt_id"], "kept-prompt");
    assert_eq!(body["model"], "gemini-3-pro");
}

#[test]
fn count_tokens_uses_the_request_only_envelope() {
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    let prepared = prepare(
        &config,
        credential(&secret, &metadata),
        Operation::CountTokens,
        request(
            HeaderMap::new(),
            "/v1beta/models/gemini-3-pro:countTokens",
            json!({"contents": [{"parts": [{"text": "hi"}]}]}),
        ),
    )
    .expect("prepared");
    assert_eq!(
        prepared.uri().to_string(),
        "https://cloudcode-pa.googleapis.com/v1internal:countTokens"
    );
    let body = body_json(&prepared);
    assert!(body.get("project").is_none());
    assert_eq!(body["request"]["model"], "models/gemini-3-pro");
    assert_eq!(body["request"]["contents"][0]["role"], "user");
}

#[test]
fn a_credential_without_a_project_or_a_token_is_invalid() {
    let config = json!({});
    let metadata = Value::Null;
    let no_project = json!({"access_token": "ya29.token"});
    let error = prepare(
        &config,
        credential(&no_project, &metadata),
        Operation::GenerateContent,
        request(
            HeaderMap::new(),
            "/v1beta/models/m:generateContent",
            json!({}),
        ),
    )
    .expect_err("no project");
    assert!(matches!(error, ChannelError::InvalidCredential));

    let no_token = json!({"provider_fields": {"project_id": "p"}});
    let error = prepare(
        &config,
        credential(&no_token, &metadata),
        Operation::GenerateContent,
        request(
            HeaderMap::new(),
            "/v1beta/models/m:generateContent",
            json!({}),
        ),
    )
    .expect_err("no access token");
    assert!(matches!(error, ChannelError::InvalidCredential));
}

// -------------------------------------------------------------- responses

#[tokio::test]
async fn a_buffered_reply_is_unwrapped_into_the_gemini_shape() {
    let client = Arc::new(ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"response": {
            "candidates": [{
                "content": {"parts": [{"text": "hello"}]},
                "citationMetadata": {"citations": [{"uri": "u"}]},
            }],
            "promptFeedback": {"blockReason": "BLOCKED_REASON_UNSPECIFIED"},
            "usageMetadata": {"promptTokenCount": 10, "candidatesTokenCount": 4},
        }}),
    )]));
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    let response = GeminiCli
        .generate_content(OperationContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            dialect: Dialect::Gemini,
            request: request(
                HeaderMap::new(),
                "/v1beta/models/gemini-3-pro:generateContent",
                json!({"contents": []}),
            ),
            client: client.clone(),
            state: Arc::new(NoState::default()),
            instance_id: Arc::from("i"),
            endpoint_override: None,
        })
        .await
        .expect("response");
    let body: Value = serde_json::from_slice(&collect(response.body).await).unwrap();
    assert!(body.get("response").is_none());
    assert_eq!(
        body["candidates"][0]["content"]["parts"][0]["text"],
        "hello"
    );
    // The Vertex spellings are normalized for a Gemini API client.
    assert!(body["candidates"][0]["citationMetadata"]["citationSources"].is_array());
    assert_eq!(
        body["promptFeedback"]["blockReason"],
        "BLOCK_REASON_UNSPECIFIED"
    );
    assert_eq!(client.sent().len(), 1);
}

#[tokio::test]
async fn a_streamed_reply_is_unwrapped_frame_by_frame() {
    let sse = concat!(
        "data: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"a\"}]}}]}}\n\n",
        "data: {\"response\":{\"usageMetadata\":{\"promptTokenCount\":9,",
        "\"candidatesTokenCount\":3,\"thoughtsTokenCount\":2}}}\n\n",
    );
    let client = Arc::new(ScriptClient::new(vec![raw_reply(StatusCode::OK, sse)]));
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    let response = GeminiCli
        .stream_generate_content(OperationContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            dialect: Dialect::Gemini,
            request: request(
                HeaderMap::new(),
                "/v1beta/models/gemini-3-pro:streamGenerateContent",
                json!({"contents": []}),
            ),
            client: client.clone(),
            state: Arc::new(NoState::default()),
            instance_id: Arc::from("i"),
            endpoint_override: None,
        })
        .await
        .expect("response");
    let text = String::from_utf8(collect(response.body).await).unwrap();
    assert!(!text.contains("\"response\""), "{text}");
    assert!(text.contains("\"candidates\""), "{text}");
    assert!(text.contains("\"usageMetadata\""), "{text}");
}

#[tokio::test]
async fn the_model_directory_comes_from_the_quota_buckets() {
    let buckets = json!({"buckets": [
        {"modelId": "gemini-3-pro", "tokenType": "REQUESTS"},
        {"modelId": "gemini-3-pro", "tokenType": "INPUT_TOKENS"},
        {"modelId": "models/gemini-3-flash", "tokenType": "REQUESTS"},
    ]});
    let client = Arc::new(ScriptClient::new(vec![
        reply(StatusCode::OK, buckets.clone()),
        reply(StatusCode::OK, buckets.clone()),
        reply(StatusCode::OK, buckets),
    ]));
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    let context = |path: &str| OperationContext {
        provider: provider(&config, None),
        credential: credential(&secret, &metadata),
        dialect: Dialect::Gemini,
        request: WireRequest {
            method: Method::GET,
            path: path.into(),
            query: None,
            headers: HeaderMap::new(),
            body: HttpBody::Bytes(Bytes::new()),
        },
        client: client.clone(),
        state: Arc::new(NoState::default()),
        instance_id: Arc::from("i"),
        endpoint_override: None,
    };

    let response = GeminiCli
        .list_models(context("/v1beta/models"))
        .await
        .expect("catalogue");
    let body: Value = serde_json::from_slice(&collect(response.body).await).unwrap();
    let names: Vec<&str> = body["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["models/gemini-3-flash", "models/gemini-3-pro"]);

    let response = GeminiCli
        .get_model(context("/v1beta/models/gemini-3-pro"))
        .await
        .expect("model");
    let body: Value = serde_json::from_slice(&collect(response.body).await).unwrap();
    assert_eq!(body["baseModelId"], "gemini-3-pro");

    let response = GeminiCli
        .get_model(context("/v1beta/models/gemini-9"))
        .await
        .expect("model");
    assert_eq!(response.status, StatusCode::NOT_FOUND);

    // Every catalogue call posts the discovered project to the quota method.
    let (method, uri, _, body) = client.sent()[0].clone();
    assert_eq!(method, Method::POST);
    assert!(uri.ends_with("/v1internal:retrieveUserQuota"), "{uri}");
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["project"], "proj-secret");
}

// ------------------------------------------------------------------ quota

#[tokio::test]
async fn quota_buckets_become_periodic_windows() {
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"buckets": [
            {"modelId": "gemini-3-pro", "tokenType": "REQUESTS",
             "remainingFraction": 0.75, "resetTime": "2026-06-22T16:01:15Z"},
            {"modelId": "gemini-3-flash", "tokenType": "REQUESTS",
             "remainingFraction": 0.5, "remainingAmount": "50"},
        ]}),
    )]);
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    let snapshot = GeminiCli
        .quota_query()
        .expect("quota query")
        .query(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            client: &client,
        })
        .await
        .expect("snapshot");
    assert_eq!(snapshot.entries.len(), 2);
    let first = &snapshot.entries[0];
    assert_eq!(first.id, "gemini-3-pro:requests");
    assert_eq!(
        first.model_scope,
        QuotaScope::Models(vec!["gemini-3-pro".into()])
    );
    let QuotaValue::Window(allowance) = &first.value else {
        panic!("expected a window");
    };
    assert_eq!(allowance.used_percent, Some("25".parse().unwrap()));
    assert_eq!(allowance.period_end_ms, Some(1_782_144_075_000));
    let QuotaValue::Window(second) = &snapshot.entries[1].value else {
        panic!("expected a window");
    };
    // 50 left at half remaining implies a limit of 100 and 50 spent.
    assert_eq!(second.limit, Some("100".parse().unwrap()));
    assert_eq!(second.used, Some("50".parse().unwrap()));

    // The channel declares no quota dimensions: which buckets exist is only
    // known once the upstream reports them.
    assert!(GeminiCli.quota_model().is_none());
    assert!(GeminiCli.quota_headers().is_none());
    // Every `<model>:<token type>` bucket is observe-only until live probing
    // settles whether buckets share pools and how long their windows are.
    support::assert_quota_contract(None, &[], &snapshot.entries, &["gemini-*"]);
}

// ------------------------------------------------------------------ usage

#[test]
fn buffered_usage_reads_the_wrapped_metadata() {
    let body = serde_json::to_vec(&json!({"response": {"usageMetadata": {
        "promptTokenCount": 100,
        "cachedContentTokenCount": 40,
        "candidatesTokenCount": 10,
        "thoughtsTokenCount": 5,
        "candidatesTokensDetails": [{"modality": "IMAGE", "tokenCount": 4}],
    }}}))
    .unwrap();
    let headers = HeaderMap::new();
    let usage = GeminiCli
        .usage_extractor()
        .expect("extractor")
        .extract(UsageContext {
            operation: OperationKey {
                operation: Operation::GenerateContent,
                dialect: Dialect::Gemini,
            },
            request_body: None,
            response: ResponseView {
                status: StatusCode::OK,
                headers: &headers,
                body: &body,
            },
        })
        .expect("extraction")
        .expect("usage");
    // promptTokenCount includes the cache read; the normalized input does not.
    assert_eq!(usage.tokens.input_tokens, Some(60));
    assert_eq!(usage.tokens.cached_input_tokens, Some(40));
    assert_eq!(usage.tokens.output_tokens, Some(15));
    assert_eq!(usage.tokens.reasoning_tokens, Some(5));
    assert_eq!(
        usage.metrics.get("image_output_tokens"),
        Some(&"4".parse().unwrap())
    );
}

#[test]
fn the_stream_observer_keeps_the_last_reported_counts() {
    let headers = HeaderMap::new();
    let mut observer = GeminiCli
        .usage_stream()
        .expect("stream")
        .start(UsageStreamContext {
            operation: OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: Dialect::Gemini,
            },
            request_body: None,
            status: StatusCode::OK,
            headers: &headers,
            transport: UsageTransport::Http {
                framing: Some(StreamFraming::Sse),
            },
        })
        .expect("observer");
    for chunk in [
        "data: {\"response\":{\"usageMetadata\":{\"promptTokenCount\":9,\"candidatesTokenCount\":1}}}\n\n",
        "data: {\"response\":{\"usageMetadata\":{\"promptTokenCount\":9,\"candidatesTokenCount\":7}}}\n\n",
    ] {
        observer
            .observe(UsageFrame::HttpChunk(chunk.as_bytes()))
            .expect("observed");
    }
    let usage = observer
        .finish(UsageStreamEnd::Complete)
        .expect("finished")
        .expect("usage");
    assert_eq!(usage.tokens.input_tokens, Some(9));
    assert_eq!(usage.tokens.output_tokens, Some(7));
}

// ------------------------------------------------------------------ login

#[tokio::test]
async fn authorize_builds_the_google_consent_url() {
    let client = ScriptClient::new(Vec::new());
    let config = json!({});
    let start = GeminiCli
        .oauth_authorization_code()
        .expect("auth code")
        .authorize(
            LoginContext {
                provider: provider(&config, None),
                client: &client,
            },
            AuthorizationRequest {
                redirect_uri: "",
                state: "st-1",
                code_challenge: "ch-1",
            },
        )
        .await
        .expect("start");
    assert_eq!(start.redirect_uri, DEFAULT_REDIRECT_URI);
    assert!(
        start
            .authorize_url
            .starts_with("https://accounts.google.com/o/oauth2/v2/auth?")
    );
    for fragment in [
        "response_type=code",
        &format!("client_id={DEFAULT_CLIENT_ID}"),
        "access_type=offline",
        "prompt=consent",
        "code_challenge_method=S256",
        "code_challenge=ch-1",
        "state=st-1",
    ] {
        assert!(
            start.authorize_url.contains(fragment),
            "{fragment} missing from {}",
            start.authorize_url
        );
    }
}

#[tokio::test]
async fn the_exchange_discovers_the_project_and_tier_for_the_host_to_persist() {
    let client = ScriptClient::new(vec![
        reply(
            StatusCode::OK,
            json!({"access_token": "ya29.new", "refresh_token": "1//r", "expires_in": 3599,
                   "scope": "https://www.googleapis.com/auth/cloud-platform"}),
        ),
        // loadCodeAssist knows the account but not its project.
        reply(
            StatusCode::OK,
            json!({"currentTier": {"id": "free-tier"},
                   "allowedTiers": [{"id": "standard-tier", "isDefault": true}]}),
        ),
        // onboardUser completes and names it.
        reply(
            StatusCode::OK,
            json!({"done": true, "response": {"cloudaicompanionProject": {"id": "proj-onboarded"}}}),
        ),
        reply(StatusCode::OK, json!({"email": "user@example.com"})),
    ]);
    let config = json!({});
    let acquired = GeminiCli
        .oauth_authorization_code()
        .expect("auth code")
        .exchange(
            LoginContext {
                provider: provider(&config, None),
                client: &client,
            },
            AuthorizationCode {
                code: "code-1",
                redirect_uri: DEFAULT_REDIRECT_URI,
                code_verifier: "verifier-1",
                state: "st-1",
                provider_state: &BTreeMap::new(),
            },
        )
        .await
        .expect("credential");
    assert_eq!(acquired.access_token, "ya29.new");
    assert_eq!(acquired.refresh_token.as_deref(), Some("1//r"));
    assert_eq!(
        acquired.provider_fields.get("project_id"),
        Some(&Value::String("proj-onboarded".into()))
    );
    assert_eq!(
        acquired.provider_fields.get("rate_limit_tier"),
        Some(&Value::String("free".into()))
    );
    assert_eq!(
        acquired.provider_fields.get("user_email"),
        Some(&Value::String("user@example.com".into()))
    );
    let sent = client.sent();
    assert!(
        sent[0].1.ends_with("oauth2.googleapis.com/token"),
        "{}",
        sent[0].1
    );
    let form = String::from_utf8(sent[0].3.clone()).unwrap();
    assert!(form.contains("grant_type=authorization_code"));
    assert!(form.contains("code_verifier=verifier-1"));
    assert!(sent[1].1.ends_with("/v1internal:loadCodeAssist"));
    let load: Value = serde_json::from_slice(&sent[1].3).unwrap();
    assert_eq!(load["metadata"]["ideType"], "IDE_UNSPECIFIED");
    assert_eq!(load["metadata"]["pluginType"], "GEMINI");
    assert!(sent[2].1.ends_with("/v1internal:onboardUser"));
    let onboard: Value = serde_json::from_slice(&sent[2].3).unwrap();
    assert_eq!(onboard["tierId"], "standard-tier");
}

#[tokio::test]
async fn refresh_renews_the_token_and_carries_the_facts_forward() {
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"access_token": "ya29.rotated", "expires_in": 3599}),
    )]);
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    let update = GeminiCli
        .refresh(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            client: &client,
        })
        .await
        .expect("update");
    assert_eq!(update.secret["access_token"], "ya29.rotated");
    // Google returns no refresh token on a normal refresh.
    assert_eq!(update.secret["refresh_token"], "1//refresh");
    assert_eq!(
        update.secret["provider_fields"]["project_id"],
        "proj-secret"
    );
    assert_eq!(update.secret["provider_fields"]["rate_limit_tier"], "pro");
    assert!(update.expires_at_ms.is_some());
    // One call only: the facts were already known, so nothing is rediscovered.
    assert_eq!(client.sent().len(), 1);
}

#[tokio::test]
async fn refresh_completes_a_discovery_the_login_could_not_finish() {
    let client = ScriptClient::new(vec![
        reply(StatusCode::OK, json!({"access_token": "ya29.rotated"})),
        reply(
            StatusCode::OK,
            json!({"cloudaicompanionProject": "proj-late", "paidTier": {"id": "g1-ultra-tier"}}),
        ),
    ]);
    let config = json!({});
    let secret = json!({"access_token": "ya29.old", "refresh_token": "1//refresh"});
    let metadata = Value::Null;
    let update = GeminiCli
        .refresh(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            client: &client,
        })
        .await
        .expect("update");
    assert_eq!(update.secret["provider_fields"]["project_id"], "proj-late");
    assert_eq!(update.secret["provider_fields"]["rate_limit_tier"], "ultra");
    assert_eq!(client.sent().len(), 2);
}

#[tokio::test]
async fn a_definitive_refusal_becomes_refresh_rejected() {
    let client = ScriptClient::new(vec![reply(
        StatusCode::BAD_REQUEST,
        json!({"error": "invalid_grant", "error_description": "Token has been expired or revoked."}),
    )]);
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    let error = GeminiCli
        .refresh(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            client: &client,
        })
        .await
        .err()
        .expect("rejected");
    assert!(
        matches!(&error, ChannelError::RefreshRejected(code) if code == "invalid_grant"),
        "{error}"
    );
}

#[tokio::test]
async fn a_transient_refusal_is_not_a_rejection() {
    let client = ScriptClient::new(vec![reply(
        StatusCode::SERVICE_UNAVAILABLE,
        json!({"error": "backend_error"}),
    )]);
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    let error = GeminiCli
        .refresh(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            client: &client,
        })
        .await
        .err()
        .expect("failed");
    assert!(
        matches!(error, ChannelError::UpstreamResponse { status, .. } if status == StatusCode::SERVICE_UNAVAILABLE)
    );
}

#[tokio::test]
async fn a_credential_without_a_refresh_token_is_rejected_without_a_call() {
    let client = ScriptClient::new(Vec::new());
    let config = json!({});
    let secret = json!({"access_token": "ya29.token"});
    let metadata = Value::Null;
    let error = GeminiCli
        .refresh(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            client: &client,
        })
        .await
        .err()
        .expect("rejected");
    assert!(matches!(error, ChannelError::RefreshRejected(_)));
    assert!(client.sent().is_empty());
}
