#![cfg(feature = "antigravity")]
//! Antigravity against a scripted client: no real upstream is called.

mod support;

use gproxy_channel::channel::{
    AuthorizationCode, AuthorizationRequest, BaseChannel, ChannelError, CredentialContext,
    CredentialRefresh, CredentialView, LoginContext, NoState, OperationContext, PrepareContext,
    ProviderView, QuotaEntry, QuotaScope, QuotaValue, QuotaWindow, ResponseView, UsageContext,
    UsageFrame, UsageStreamContext, UsageStreamEnd, UsageTransport,
};
use gproxy_channel::channels::antigravity::{
    Antigravity, CLI_USER_AGENT, DEFAULT_CLIENT_ID, DEFAULT_REDIRECT_URI,
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
        id: "ag",
        channel: "antigravity",
        base_url,
        config,
    }
}

fn credential<'a>(secret: &'a Value, metadata: &'a Value) -> CredentialView<'a> {
    CredentialView {
        id: "c",
        provider_id: "ag",
        auth_kind: "oauth",
        secret,
        metadata,
        version: 2,
        expires_at_ms: None,
    }
}

fn secret() -> Value {
    json!({
        "access_token": "ya29.token",
        "refresh_token": "1//refresh",
        "token_type": "Bearer",
        "provider_fields": {"project_id": "proj-secret", "rate_limit_tier": "pro"},
    })
}

fn request(headers: HeaderMap, path: &str, body: Value) -> WireRequest<HttpBody> {
    WireRequest {
        method: Method::POST,
        path: path.into(),
        query: Some("key=leaked".into()),
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
    Antigravity.prepare(PrepareContext {
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
    let descriptor: ChannelDescriptor = Antigravity.descriptor();
    assert_eq!(descriptor.id, "antigravity");
    assert_eq!(descriptor.login_modes, vec![LoginMode::AuthorizationCode]);
    assert!(descriptor.capabilities.refresh);
    assert!(descriptor.capabilities.quota_query);
    assert!(!descriptor.capabilities.services);
    for key in ["base_url", "allowed_headers"] {
        assert!(descriptor.config_key(key).is_some(), "missing key {key}");
    }
    assert_eq!(
        Antigravity.native_dialects(provider(&Value::Null, None), Operation::CountTokens),
        vec![Dialect::Gemini]
    );
    // The editor's Go client has its own ClientHello, so the channel names one.
    assert!(Antigravity.default_connection().unwrap().gzip);
}

// ----------------------------------------------------------------- prepare

#[test]
fn prepare_targets_the_daily_code_assist_host_with_the_bearer_token() {
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
            json!({"contents": []}),
        ),
    )
    .expect("prepared");
    assert_eq!(
        prepared.uri().to_string(),
        "https://daily-cloudcode-pa.googleapis.com/v1internal:streamGenerateContent?alt=sse"
    );
    assert_eq!(
        prepared.headers()["authorization"],
        HeaderValue::from_static("Bearer ya29.token")
    );
    assert!(!prepared.uri().to_string().contains("leaked"));
}

#[test]
fn prepare_sends_the_editor_identity_and_a_client_cannot_spoof_it() {
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    let mut headers = HeaderMap::new();
    headers.insert("user-agent", HeaderValue::from_static("claude-cli/2.1.258"));
    headers.insert("cookie", HeaderValue::from_static("SID=stolen"));
    headers.insert(
        "accept-encoding",
        HeaderValue::from_static("gzip, deflate, br"),
    );
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
    assert_eq!(
        prepared.headers()["user-agent"],
        HeaderValue::from_static(CLI_USER_AGENT)
    );
    assert_eq!(prepared.headers()["content-type"], "application/json");
    assert!(!prepared.headers().contains_key("cookie"));
    assert!(!prepared.headers().contains_key("accept-encoding"));
    // Unlike the Gemini CLI, Antigravity sends no Node client banner and no
    // Accept on a generation call.
    assert!(!prepared.headers().contains_key("x-goog-api-client"));
    assert!(!prepared.headers().contains_key("accept"));
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
    assert_eq!(
        prepared.headers()["user-agent"],
        HeaderValue::from_static(CLI_USER_AGENT)
    );
}

// ------------------------------------------------------------ body shaping

#[test]
fn the_gemini_body_is_wrapped_and_the_high_tier_gets_its_thinking_budget() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({"project_id": "proj-metadata"});
    let prepared = prepare(
        &config,
        credential(&secret, &metadata),
        Operation::GenerateContent,
        request(
            HeaderMap::new(),
            "/v1beta/models/gemini-3.1-pro-high:generateContent",
            json!({"contents": [{"parts": [{"text": "hi"}]}], "store": true}),
        ),
    )
    .expect("prepared");
    let body = body_json(&prepared);
    assert_eq!(body["model"], "gemini-3.1-pro-high");
    // Read from the host-persisted metadata, never rediscovered.
    assert_eq!(body["project"], "proj-metadata");
    assert!(body["request"].get("store").is_none());
    assert_eq!(
        body["request"]["generationConfig"]["thinkingConfig"]["thinkingBudget"],
        10_001
    );

    // An explicit budget from the caller is left alone, and other models get
    // no budget at all.
    let prepared = prepare(
        &config,
        credential(&secret, &metadata),
        Operation::GenerateContent,
        request(
            HeaderMap::new(),
            "/v1beta/models/gemini-3.1-pro-high:generateContent",
            json!({"contents": [], "generationConfig": {"thinkingConfig": {"thinkingBudget": 7}}}),
        ),
    )
    .expect("prepared");
    assert_eq!(
        body_json(&prepared)["request"]["generationConfig"]["thinkingConfig"]["thinkingBudget"],
        7
    );

    let prepared = prepare(
        &config,
        credential(&secret, &metadata),
        Operation::GenerateContent,
        request(
            HeaderMap::new(),
            "/v1beta/models/gemini-3-pro:generateContent",
            json!({"contents": []}),
        ),
    )
    .expect("prepared");
    assert!(
        body_json(&prepared)["request"]
            .get("generationConfig")
            .is_none()
    );
}

fn claude_request(body: Value) -> Value {
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    let prepared = prepare(
        &config,
        credential(&secret, &metadata),
        Operation::StreamGenerateContent,
        request(
            HeaderMap::new(),
            "/v1beta/models/claude-opus-4-6-thinking:streamGenerateContent",
            body,
        ),
    )
    .expect("prepared");
    body_json(&prepared)["request"].clone()
}

#[test]
fn claude_tools_use_the_proto_parameters_both_sides_accept() {
    // Live refusals: `parametersJsonSchema` never reaches Anthropic, and the
    // proto rejects `$schema`, `$defs`/`$ref`, `const`, `deprecated`,
    // `examples` and a `type` array; `anyOf` passes the proto but Anthropic
    // refuses the schema.
    let request = claude_request(json!({
        "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
        "tools": [{"functionDeclarations": [{
            "name": "edit",
            "parametersJsonSchema": {
                "$schema": "http://json-schema.org/draft-07/schema#",
                "type": "object",
                "$defs": {"Mode": {"type": "string", "enum": ["a", "b"], "deprecated": true}},
                "additionalProperties": false,
                "properties": {
                    "path": {"type": "string", "minLength": 1, "examples": ["x"]},
                    "mode": {"$ref": "#/$defs/Mode", "description": "how"},
                    "kind": {"const": "file"},
                    "count": {"type": ["integer", "null"]},
                    "target": {"anyOf": [{"type": "string"}, {"type": "null"}]},
                    "level": {"enum": [1, 2]},
                    "tags": {"type": "array", "items": {"type": "string", "format": "uri"}}
                },
                "required": ["path", "missing"]
            }
        }]}]
    }));
    let declaration = &request["tools"][0]["functionDeclarations"][0];
    assert!(declaration.get("parametersJsonSchema").is_none());
    assert_eq!(
        declaration["parameters"],
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["path"],
            "properties": {
                "path": {"type": "string", "minLength": 1},
                "mode": {"type": "string", "enum": ["a", "b"], "description": "how"},
                "kind": {"enum": ["file"]},
                "count": {"type": "integer", "nullable": true},
                "target": {"type": "string", "nullable": true},
                "level": {"type": "string", "enum": ["1", "2"]},
                "tags": {"type": "array", "items": {"type": "string", "format": "uri"}}
            }
        })
    );
}

fn claude_config(generation: Value) -> Value {
    claude_request(json!({
        "contents": [{"role": "user", "parts": [{"text": "hi"}]}],
        "generationConfig": generation,
    }))
    .get("generationConfig")
    .cloned()
    .unwrap_or(Value::Null)
}

#[test]
fn claude_keeps_its_output_limit_and_takes_one_thinking_budget() {
    // Stripped, the host caps Claude at 8192 output tokens.
    assert_eq!(
        claude_config(json!({"maxOutputTokens": 32_000, "temperature": 0.2})),
        json!({"maxOutputTokens": 32_000, "temperature": 0.2})
    );
    // The snake spelling, clamped to the catalogue's ceiling.
    assert_eq!(
        claude_config(json!({"max_output_tokens": 200_000})),
        json!({"maxOutputTokens": 64_000})
    );
    // No limit and no thinking: the body stays as the caller left it.
    assert_eq!(claude_config(json!({})), Value::Null);
    // Thinking without a limit gets the model maximum, so the budget fits.
    assert_eq!(
        claude_config(json!({"thinkingConfig": {"thinkingBudget": 20_000}})),
        json!({"maxOutputTokens": 64_000,
               "thinkingConfig": {"thinkingBudget": 20_000, "includeThoughts": true}})
    );
    // A budget under 1024 is refused upstream; one at or past the limit too.
    assert_eq!(
        claude_config(json!({"thinkingConfig": {"thinkingBudget": 500}}))["thinkingConfig"]["thinkingBudget"],
        1024
    );
    assert_eq!(
        claude_config(json!({"maxOutputTokens": 4096,
                             "thinkingConfig": {"thinkingBudget": 8192}})),
        json!({"maxOutputTokens": 4096,
               "thinkingConfig": {"thinkingBudget": 4095, "includeThoughts": true}})
    );
    // A limit with no room for the smallest budget turns thinking off.
    assert_eq!(
        claude_config(json!({"maxOutputTokens": 1000,
                             "thinkingConfig": {"thinkingBudget": 2048}})),
        json!({"maxOutputTokens": 1000})
    );
    // Levels and the dynamic budget are ignored upstream, so they become
    // budgets; a zero budget is thinking off.
    assert_eq!(
        claude_config(json!({"thinkingConfig": {"thinkingLevel": "high"}}))["thinkingConfig"]["thinkingBudget"],
        32_768
    );
    assert_eq!(
        claude_config(json!({"thinkingConfig": {"thinkingBudget": -1}}))["thinkingConfig"]["thinkingBudget"],
        16_384
    );
    assert_eq!(
        claude_config(json!({"thinkingConfig": {"thinkingBudget": 0}})),
        Value::Null
    );
}

#[test]
fn gemini_models_still_lose_the_output_limit() {
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    let prepared = prepare(
        &config,
        credential(&secret, &metadata),
        Operation::GenerateContent,
        request(
            HeaderMap::new(),
            "/v1beta/models/gemini-3-flash:generateContent",
            json!({"contents": [], "generationConfig": {"maxOutputTokens": 100}}),
        ),
    )
    .expect("prepared");
    assert!(
        body_json(&prepared)["request"]["generationConfig"]
            .get("maxOutputTokens")
            .is_none()
    );
}

#[test]
fn claude_history_is_reshaped_into_what_anthropic_accepts() {
    let call = json!({"functionCall": {"name": "f", "args": {}, "id": "toolu_1"}});
    let request = claude_request(json!({"contents": [
        {"role": "user", "parts": [{"text": ""}, {"text": "go"}]},
        // Streamed pieces: thought text, then the signature on an empty part.
        {"role": "model", "parts": [
            {"thought": true, "text": "plan "},
            {"thought": true, "text": "it"},
            {"thought": true, "text": "", "thoughtSignature": "SIG1"},
            call,
        ]},
        {"role": "user", "parts": [{"functionResponse": {"name": "f", "id": "toolu_1", "response": {}}}]},
        // Unsigned thinking (from another upstream) cannot be replayed.
        {"role": "model", "parts": [{"thought": true, "text": "unsigned"}, {"text": "done"}]},
        {"role": "user", "parts": [{"text": "again"}]},
        // Only empty text: the turn disappears.
        {"role": "model", "parts": [{"text": ""}]},
        {"role": "user", "parts": [{"text": "more"}]},
        // A prefill is refused, so the trailing model turn is cut.
        {"role": "model", "parts": [{"text": "Sure "}]},
    ]}));
    assert_eq!(
        request["contents"],
        json!([
            {"role": "user", "parts": [{"text": "go"}]},
            {"role": "model", "parts": [
                {"thought": true, "text": "plan it", "thoughtSignature": "SIG1"},
                call,
            ]},
            {"role": "user", "parts": [{"functionResponse": {"name": "f", "id": "toolu_1", "response": {}}}]},
            {"role": "model", "parts": [{"text": "done"}]},
            {"role": "user", "parts": [{"text": "again"}]},
            {"role": "user", "parts": [{"text": "more"}]},
        ])
    );
}

#[test]
fn gemini_tools_keep_their_json_schema() {
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    let schema = json!({"$schema": "x", "type": "object", "properties": {}});
    let prepared = prepare(
        &config,
        credential(&secret, &metadata),
        Operation::GenerateContent,
        request(
            HeaderMap::new(),
            "/v1beta/models/gemini-3-flash:generateContent",
            json!({"contents": [], "tools": [{"functionDeclarations": [
                {"name": "f", "parametersJsonSchema": schema}
            ]}]}),
        ),
    )
    .expect("prepared");
    assert_eq!(
        body_json(&prepared)["request"]["tools"][0]["functionDeclarations"][0]["parametersJsonSchema"],
        schema
    );
}

#[test]
fn the_camel_case_session_id_antigravity_sends_survives_unrenamed() {
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    let prepared = prepare(
        &config,
        credential(&secret, &metadata),
        Operation::GenerateContent,
        request(
            HeaderMap::new(),
            "/v1beta/models/gemini-3-pro:generateContent",
            json!({"contents": [], "sessionId": "sess-1"}),
        ),
    )
    .expect("prepared");
    let body = body_json(&prepared);
    assert_eq!(body["request"]["sessionId"], "sess-1");
    // Not rewritten into the Gemini CLI's snake-case spelling.
    assert!(body["request"].get("session_id").is_none());

    // A client that already built the envelope keeps its own `request`.
    let prepared = prepare(
        &config,
        credential(&secret, &metadata),
        Operation::GenerateContent,
        request(
            HeaderMap::new(),
            "/v1beta/models/gemini-3-pro:generateContent",
            json!({"request": {"contents": [], "sessionId": "sess-2"}}),
        ),
    )
    .expect("prepared");
    assert_eq!(body_json(&prepared)["request"]["sessionId"], "sess-2");
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
        json!({"response": {"candidates": [{"content": {"parts": [{"text": "hello"}]}}]}}),
    )]));
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    let response = Antigravity
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
}

#[tokio::test]
async fn a_streamed_reply_is_unwrapped_frame_by_frame() {
    let sse =
        "data: {\"response\":{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"a\"}]}}]}}\n\n";
    let client = Arc::new(ScriptClient::new(vec![raw_reply(StatusCode::OK, sse)]));
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    let response = Antigravity
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
}

fn catalogue() -> Value {
    json!({
        "models": {
            "gemini-3-pro": {"display_name": "Gemini 3 Pro", "maxTokens": 1_000_000,
                             "maxOutputTokens": 64_000,
                             "quotaInfo": {"remainingFraction": 0.8,
                                           "resetTime": "2026-08-01T12:00:00Z"}},
            "text-embedding-004": {},
        },
        "defaultAgentModelId": "gemini-3-pro",
        "commandModelIds": ["gemini-3-flash"],
        "tieredModelIds": [{"modelId": "models/gemini-3.1-pro-high"}],
    })
}

#[tokio::test]
async fn the_model_directory_harvests_every_role_field() {
    let client = Arc::new(ScriptClient::new(vec![
        reply(StatusCode::OK, catalogue()),
        reply(StatusCode::OK, catalogue()),
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

    let response = Antigravity
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
    assert_eq!(
        names,
        [
            "models/gemini-3-flash",
            "models/gemini-3-pro",
            "models/gemini-3.1-pro-high"
        ]
    );
    // Embedding models are not generation models.
    assert!(!names.iter().any(|name| name.contains("embedding")));
    let pro = body["models"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["baseModelId"] == "gemini-3-pro")
        .unwrap();
    assert_eq!(pro["displayName"], "Gemini 3 Pro");
    assert_eq!(pro["inputTokenLimit"], 1_000_000);
    assert_eq!(pro["outputTokenLimit"], 64_000);

    let response = Antigravity
        .get_model(context("/v1beta/models/nope"))
        .await
        .expect("model");
    assert_eq!(response.status, StatusCode::NOT_FOUND);

    // The catalogue call posts an empty body and carries no project.
    let (method, uri, _, body) = client.sent()[0].clone();
    assert_eq!(method, Method::POST);
    assert!(uri.ends_with("/v1internal:fetchAvailableModels"), "{uri}");
    assert_eq!(body, b"{}");
}

// ------------------------------------------------------------------ quota

async fn quota_snapshot(catalogue: Value) -> Vec<QuotaEntry> {
    let client = ScriptClient::new(vec![reply(StatusCode::OK, catalogue)]);
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    Antigravity
        .quota_query()
        .expect("quota query")
        .query(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            client: &client,
        })
        .await
        .expect("snapshot")
        .entries
}

#[tokio::test]
async fn quota_info_becomes_a_periodic_window_per_pool() {
    let entries = quota_snapshot(catalogue()).await;
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(
        entry.id, "pool_google",
        "a provider-less Gemini model is Google's"
    );
    assert_eq!(
        entry.model_scope,
        QuotaScope::Models(vec!["gemini-3-pro".into()])
    );
    let QuotaValue::Window(allowance) = &entry.value else {
        panic!("expected a window");
    };
    assert_eq!(allowance.used_percent, Some("20".parse().unwrap()));
    assert_eq!(allowance.period_end_ms, Some(1_785_585_600_000));
    assert_eq!(allowance.period_start_ms, None);

    assert!(Antigravity.quota_headers().is_none());
}

// Live captures (2026-09-26), trimmed to the fields quota reads: idle, then
// after one gemini-3-flash request, then after one claude-sonnet-4-6 request.
const IDLE: &str = include_str!("fixtures/quota/antigravity_models_idle.json");
const AFTER_GEMINI: &str = include_str!("fixtures/quota/antigravity_models_after_gemini.json");
const AFTER_CLAUDE: &str = include_str!("fixtures/quota/antigravity_models_after_claude.json");

fn pool_reading(entries: &[QuotaEntry], id: &str) -> (Option<rust_decimal::Decimal>, Option<i64>) {
    let entry = entries.iter().find(|e| e.id == id).expect(id);
    let QuotaValue::Window(window) = &entry.value else {
        panic!("window");
    };
    (window.used_percent, window.period_end_ms)
}

#[tokio::test]
async fn captured_catalogues_report_two_shared_pools() {
    let model = Antigravity.quota_model().unwrap();
    let config = json!({});
    let s = secret();
    let declared = model.dimensions(provider(&config, None), credential(&s, &Value::Null));
    assert_eq!(
        declared.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
        ["pool_google", "pool_third_party"]
    );
    assert!(declared.iter().all(|d| d.window
        == QuotaWindow::Rolling {
            seconds: 5 * 60 * 60
        }));

    let idle = quota_snapshot(serde_json::from_str(IDLE).unwrap()).await;
    let after_gemini = quota_snapshot(serde_json::from_str(AFTER_GEMINI).unwrap()).await;
    let after_claude = quota_snapshot(serde_json::from_str(AFTER_CLAUDE).unwrap()).await;
    for entries in [&idle, &after_gemini, &after_claude] {
        support::assert_quota_contract(Some(model), &declared, entries, &[]);
        assert_eq!(
            entries.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["pool_google", "pool_third_party"]
        );
    }
    let QuotaScope::Models(google) = &idle[0].model_scope else {
        panic!("models");
    };
    assert_eq!(google.len(), 20);
    assert!(
        google.iter().all(|id| id.starts_with("gemini")),
        "{google:?}"
    );
    assert_eq!(
        idle[1].model_scope,
        QuotaScope::Models(vec![
            "claude-opus-4-6-thinking".into(),
            "claude-sonnet-4-6".into(),
            "gpt-oss-120b-medium".into(),
        ])
    );
    // The pool's declared scope comes from the reading.
    let placed = model.classify(&declared, &idle[1]).unwrap();
    assert!(placed.scope.matches("gpt-oss-120b-medium"));
    assert!(!placed.scope.matches("gemini-3-flash"));
    assert!(
        !placed.scope.matches("chat_20706"),
        "internal models are in no pool"
    );

    // A Gemini request moves only the Google pool; a Claude request only the other.
    // `remainingFraction` 0.9999976 and 0.9999712, as percent used to 4 places.
    let used = |percent: &str| Some(percent.parse().unwrap());
    assert_eq!(pool_reading(&idle, "pool_google").0, Some(0.into()));
    assert_eq!(pool_reading(&after_gemini, "pool_google").0, used("0.0002"));
    assert_eq!(
        pool_reading(&after_gemini, "pool_third_party").0,
        Some(0.into())
    );
    assert_eq!(
        pool_reading(&after_claude, "pool_google"),
        pool_reading(&after_gemini, "pool_google")
    );
    assert_eq!(
        pool_reading(&after_claude, "pool_third_party").0,
        used("0.0029")
    );
    // 2026-09-25T21:21:14Z: first Claude use + 5h.
    assert_eq!(
        pool_reading(&after_claude, "pool_third_party").1,
        Some(1_790_371_274_000)
    );
}

// ------------------------------------------------------------------ usage

#[test]
fn buffered_usage_reads_the_wrapped_metadata() {
    let body = serde_json::to_vec(&json!({"response": {"usageMetadata": {
        "promptTokenCount": 50,
        "candidatesTokenCount": 8,
        "thoughtsTokenCount": 2,
    }}}))
    .unwrap();
    let headers = HeaderMap::new();
    let usage = Antigravity
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
    assert_eq!(usage.tokens.input_tokens, Some(50));
    assert_eq!(usage.tokens.output_tokens, Some(10));
    assert_eq!(usage.tokens.reasoning_tokens, Some(2));
}

#[test]
fn the_stream_observer_reads_the_wrapped_metadata() {
    let headers = HeaderMap::new();
    let mut observer = Antigravity
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
    observer
        .observe(UsageFrame::HttpChunk(
            b"data: {\"response\":{\"usageMetadata\":{\"promptTokenCount\":4,\"candidatesTokenCount\":6}}}\n\n",
        ))
        .expect("observed");
    let usage = observer
        .finish(UsageStreamEnd::Complete)
        .expect("finished")
        .expect("usage");
    assert_eq!(usage.tokens.input_tokens, Some(4));
    assert_eq!(usage.tokens.output_tokens, Some(6));
}

// ------------------------------------------------------------------ login

#[tokio::test]
async fn authorize_uses_antigravitys_own_client_and_scopes() {
    let client = ScriptClient::new(Vec::new());
    let config = json!({});
    let start = Antigravity
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
            .contains(&format!("client_id={DEFAULT_CLIENT_ID}"))
    );
    // The three scopes beyond the Gemini CLI's.
    for scope in ["cclog", "experimentsandconfigs", "aicode"] {
        assert!(start.authorize_url.contains(scope), "{scope} missing");
    }
}

#[tokio::test]
async fn the_exchange_discovers_the_project_and_reports_antigravity_as_the_ide() {
    let client = ScriptClient::new(vec![
        reply(
            StatusCode::OK,
            json!({"access_token": "ya29.new", "refresh_token": "1//r", "expires_in": 3599}),
        ),
        reply(
            StatusCode::OK,
            json!({"cloudaicompanionProject": {"id": "proj-loaded"},
                   "paidTier": {"id": "ws-ai-ultra-business-tier"}}),
        ),
        reply(StatusCode::OK, json!({"email": "dev@example.com"})),
    ]);
    let config = json!({});
    let acquired = Antigravity
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
    assert_eq!(
        acquired.provider_fields.get("project_id"),
        Some(&Value::String("proj-loaded".into()))
    );
    assert_eq!(
        acquired.provider_fields.get("rate_limit_tier"),
        Some(&Value::String("ultra".into()))
    );
    let sent = client.sent();
    // No onboarding call: the account already had a project.
    assert_eq!(sent.len(), 3);
    assert!(sent[1].1.ends_with("/v1internal:loadCodeAssist"));
    let load: Value = serde_json::from_slice(&sent[1].3).unwrap();
    assert_eq!(load["metadata"]["ideType"], "ANTIGRAVITY");
    assert_eq!(
        sent[1].2["user-agent"],
        HeaderValue::from_static(CLI_USER_AGENT)
    );
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
    let update = Antigravity
        .refresh(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            client: &client,
        })
        .await
        .expect("update");
    assert_eq!(update.secret["access_token"], "ya29.rotated");
    assert_eq!(update.secret["refresh_token"], "1//refresh");
    assert_eq!(
        update.secret["provider_fields"]["project_id"],
        "proj-secret"
    );
    assert_eq!(client.sent().len(), 1);
}

#[tokio::test]
async fn a_definitive_refusal_becomes_refresh_rejected() {
    let client = ScriptClient::new(vec![reply(
        StatusCode::BAD_REQUEST,
        json!({"error": "invalid_grant"}),
    )]);
    let config = json!({});
    let secret = secret();
    let metadata = Value::Null;
    let error = Antigravity
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
