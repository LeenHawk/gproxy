#![cfg(feature = "kiro")]
//! Kiro against a scripted client: no real upstream is called.

mod support;

use futures_util::StreamExt as _;
use gproxy_channel::channel::{
    AcquiredCredential, AuthorizationCode, AuthorizationRequest, BaseChannel, ChannelError,
    CredentialContext, CredentialRefresh, CredentialView, DevicePoll, LoginContext, NoState,
    OAuthAuthorizationCode, OAuthDeviceCode, OperationContext, PrepareContext, ProviderView,
    QuotaQuery, QuotaValue, ResponseView, UsageContext, UsageExtractor,
};
use gproxy_channel::channels::kiro::{AGENTIC_REQUEST_DIMENSION, Kiro};
use gproxy_channel::{LoginMode, OutboundClient};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    capability::{CapabilityError, CapabilityFuture},
    connection::Bytes,
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
        id: "k",
        channel: "kiro",
        base_url,
        config,
    }
}

fn credential<'a>(secret: &'a Value, metadata: &'a Value) -> CredentialView<'a> {
    CredentialView {
        id: "c",
        provider_id: "k",
        auth_kind: "oauth",
        secret,
        metadata,
        version: 3,
        expires_at_ms: None,
    }
}

fn secret() -> Value {
    json!({"access_token": "kiro-access", "refresh_token": "kiro-refresh"})
}

/// The facts a login recorded, as the host publishes them.
fn metadata() -> Value {
    json!({"profile_arn": "arn:aws:codewhisperer:us-east-1:1:profile/P"})
}

/// A caller's request, carrying the source authentication and the hop-by-hop
/// headers every channel drops.
fn request(headers: HeaderMap, path: &str, body: Value) -> WireRequest<HttpBody> {
    let mut all = headers;
    all.insert("authorization", HeaderValue::from_static("Bearer client"));
    all.insert("host", HeaderValue::from_static("gproxy.local"));
    all.insert("content-length", HeaderValue::from_static("2"));
    WireRequest {
        method: Method::POST,
        path: path.into(),
        query: None,
        headers: all,
        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&body).unwrap())),
    }
}

fn responses_body() -> Value {
    json!({"model": "claude-sonnet-4-5", "instructions": "be brief", "input": "hello"})
}

fn prepare(
    config: &Value,
    base_url: Option<&str>,
    secret: &Value,
    metadata: &Value,
    operation: Operation,
    request: WireRequest<HttpBody>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    Kiro.prepare(PrepareContext {
        provider: provider(config, base_url),
        credential: credential(secret, metadata),
        operation: OperationKey {
            operation,
            dialect: Dialect::OpenAi,
        },
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
fn a_generation_is_a_smithy_call_carrying_the_conversation_envelope() {
    let prepared = prepare(
        &json!({}),
        None,
        &secret(),
        &metadata(),
        Operation::StreamGenerateContent,
        request(HeaderMap::new(), "/v1/responses", responses_body()),
    )
    .expect("prepared");
    assert_eq!(prepared.uri(), "https://runtime.us-east-1.kiro.dev/");
    assert_eq!(prepared.method(), Method::POST);
    let headers = prepared.headers();
    assert_eq!(headers["authorization"], "Bearer kiro-access");
    assert_eq!(
        headers["x-amz-target"],
        "AmazonCodeWhispererStreamingService.GenerateAssistantResponse"
    );
    assert_eq!(headers["content-type"], "application/x-amz-json-1.0");
    assert_eq!(headers["x-amzn-codewhisperer-optout"], "false");
    assert_eq!(headers["amz-sdk-request"], "attempt=1; max=3");
    assert!(
        headers["user-agent"]
            .to_str()
            .unwrap()
            .starts_with("aws-sdk-rust/"),
        "the runtime plane's own SDK banner"
    );
    assert_eq!(headers["user-agent"], headers["x-amz-user-agent"]);
    assert_eq!(
        headers["amz-sdk-invocation-id"].to_str().unwrap().len(),
        36,
        "a UUID per call"
    );
    assert!(headers.get("host").is_none());

    let body = body_json(&prepared);
    let state = &body["conversationState"];
    assert_eq!(
        state["currentMessage"]["userInputMessage"]["content"],
        "hello"
    );
    assert_eq!(
        state["currentMessage"]["userInputMessage"]["modelId"],
        "claude-sonnet-4.5"
    );
    assert_eq!(
        state["history"][0]["userInputMessage"]["content"],
        "be brief"
    );
    assert_eq!(
        body["profileArn"], "arn:aws:codewhisperer:us-east-1:1:profile/P",
        "the profile the login recorded, read back out of the metadata"
    );
}

#[test]
fn a_region_moves_both_planes_and_a_base_url_replaces_the_runtime_one() {
    let regional = prepare(
        &json!({"region": "eu-central-1"}),
        None,
        &secret(),
        &metadata(),
        Operation::StreamGenerateContent,
        request(HeaderMap::new(), "/v1/responses", responses_body()),
    )
    .expect("prepared");
    assert_eq!(regional.uri(), "https://runtime.eu-central-1.kiro.dev/");

    let catalogue = prepare(
        &json!({"region": "eu-central-1"}),
        None,
        &secret(),
        &metadata(),
        Operation::ListModels,
        request(HeaderMap::new(), "/v1/models", json!({})),
    )
    .expect("prepared");
    assert!(
        catalogue
            .uri()
            .to_string()
            .starts_with("https://management.eu-central-1.kiro.dev/?"),
        "{}",
        catalogue.uri()
    );
    assert!(
        catalogue
            .uri()
            .to_string()
            .contains("profileArn=arn%3Aaws%3Acodewhisperer%3Aus-east-1%3A1%3Aprofile%2FP"),
        "the profile is a query value"
    );
    assert_eq!(
        catalogue.headers()["x-amz-target"],
        "AmazonCodeWhispererService.ListAvailableModels"
    );

    let proxied = prepare(
        &json!({}),
        Some("https://gateway.invalid/kiro"),
        &secret(),
        &metadata(),
        Operation::StreamGenerateContent,
        request(HeaderMap::new(), "/v1/responses", responses_body()),
    )
    .expect("prepared");
    assert_eq!(proxied.uri(), "https://gateway.invalid/kiro/");
}

#[test]
fn a_client_cannot_forge_the_sdk_identity_but_keeps_its_allow_listed_headers() {
    let mut caller = HeaderMap::new();
    for (name, value) in [
        ("x-amz-target", "AmazonCodeWhispererService.Whatever"),
        ("x-amz-user-agent", "curl/8"),
        ("user-agent", "curl/8"),
        ("amz-sdk-invocation-id", "0000"),
        ("x-amzn-codewhisperer-optout", "true"),
        ("cookie", "session=1"),
        ("x-trace", "keep-me"),
    ] {
        caller.insert(name, HeaderValue::from_static(value));
    }
    let prepared = prepare(
        &json!({"allowed_headers": ["x-trace"]}),
        None,
        &secret(),
        &metadata(),
        Operation::StreamGenerateContent,
        request(caller, "/v1/responses", responses_body()),
    )
    .expect("prepared");
    let headers = prepared.headers();
    assert_eq!(
        headers["x-amz-target"],
        "AmazonCodeWhispererStreamingService.GenerateAssistantResponse"
    );
    assert_ne!(headers["x-amz-user-agent"], "curl/8");
    assert_ne!(headers["user-agent"], "curl/8");
    assert_ne!(headers["amz-sdk-invocation-id"], "0000");
    assert_eq!(headers["x-amzn-codewhisperer-optout"], "false");
    assert!(headers.get("cookie").is_none());
    assert_eq!(
        headers["x-trace"], "keep-me",
        "the allow-list still forwards what it names"
    );
}

#[test]
fn a_credential_without_an_access_token_is_refused_and_so_is_a_catalogue_without_a_profile() {
    assert!(matches!(
        prepare(
            &json!({}),
            None,
            &json!({"refresh_token": "r"}),
            &metadata(),
            Operation::StreamGenerateContent,
            request(HeaderMap::new(), "/v1/responses", responses_body()),
        ),
        Err(ChannelError::InvalidCredential)
    ));
    assert!(matches!(
        prepare(
            &json!({}),
            None,
            &secret(),
            &Value::Null,
            Operation::ListModels,
            request(HeaderMap::new(), "/v1/models", json!({})),
        ),
        Err(ChannelError::InvalidCredential)
    ));
    // A provider-level profile stands in for a credential that has none.
    assert!(
        prepare(
            &json!({"profile_arn": "arn:from:config"}),
            None,
            &secret(),
            &Value::Null,
            Operation::ListModels,
            request(HeaderMap::new(), "/v1/models", json!({})),
        )
        .is_ok()
    );
}

#[test]
fn the_declared_dialect_is_responses_and_nothing_else_is_prepared() {
    let config = json!({});
    for operation in [
        Operation::GenerateContent,
        Operation::StreamGenerateContent,
        Operation::ListModels,
    ] {
        assert_eq!(
            Kiro.native_dialects(provider(&config, None), operation),
            [Dialect::OpenAi],
            "{operation:?}"
        );
    }
    assert!(
        Kiro.native_dialects(provider(&config, None), Operation::CountTokens)
            .is_empty()
    );
    assert!(matches!(
        prepare(
            &config,
            None,
            &secret(),
            &metadata(),
            Operation::CountTokens,
            request(HeaderMap::new(), "/v1/responses", json!({})),
        ),
        Err(ChannelError::UnsupportedOperation(_))
    ));
}

// ---------------------------------------------------------------- streaming

/// One `vnd.amazon.eventstream` frame, built the way CodeWhisperer builds it.
fn event_frame(event_type: &str, payload: Value) -> Vec<u8> {
    let mut headers = Vec::new();
    for (name, value) in [(":message-type", "event"), (":event-type", event_type)] {
        headers.push(name.len() as u8);
        headers.extend_from_slice(name.as_bytes());
        headers.push(7);
        headers.extend_from_slice(&(value.len() as u16).to_be_bytes());
        headers.extend_from_slice(value.as_bytes());
    }
    let payload = serde_json::to_vec(&payload).unwrap();
    let total = 12 + headers.len() + payload.len() + 4;
    let mut frame = Vec::with_capacity(total);
    frame.extend_from_slice(&(total as u32).to_be_bytes());
    frame.extend_from_slice(&(headers.len() as u32).to_be_bytes());
    frame.extend_from_slice(&crc32(&frame).to_be_bytes());
    frame.extend_from_slice(&headers);
    frame.extend_from_slice(&payload);
    frame.extend_from_slice(&crc32(&frame).to_be_bytes());
    frame
}

/// The CRC-32 the event-stream format uses, computed here rather than pulled
/// in so the test does not agree with the implementation by construction.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// A full answer: reasoning, text in two overlapping frames, one tool call
/// and the metering event.
fn answer() -> Vec<u8> {
    let mut wire = Vec::new();
    wire.extend(event_frame(
        "reasoningContentEvent",
        json!({"text": "thinking"}),
    ));
    wire.extend(event_frame(
        "assistantResponseEvent",
        json!({"content": "Hel"}),
    ));
    // The upstream resends what it already said; only the tail is new.
    wire.extend(event_frame(
        "assistantResponseEvent",
        json!({"content": "Hello%20there"}),
    ));
    wire.extend(event_frame(
        "toolUseEvent",
        json!({"toolUseId": "call-1", "name": "readFile", "input": "{\"p\":"}),
    ));
    wire.extend(event_frame(
        "toolUseEvent",
        json!({"toolUseId": "call-1", "input": "\"a\"}", "stop": true}),
    ));
    wire.extend(event_frame(
        "metadataEvent",
        json!({"tokenUsage": {"uncachedInputTokens": 10, "cacheReadInputTokens": 30,
                              "cacheWriteInputTokens": 5, "outputTokens": 7}}),
    ));
    wire
}

fn context<'a>(
    config: &'a Value,
    secret: &'a Value,
    metadata: &'a Value,
    client: Arc<ScriptClient>,
    body: Value,
) -> OperationContext<'a> {
    OperationContext {
        provider: provider(config, None),
        credential: credential(secret, metadata),
        dialect: Dialect::OpenAi,
        request: request(HeaderMap::new(), "/v1/responses", body),
        client,
        state: Arc::new(NoState::default()),
        instance_id: Arc::from("i"),
        endpoint_override: None,
    }
}

#[tokio::test]
async fn the_event_stream_becomes_responses_sse() {
    let client = ScriptClient::new(vec![raw_reply(StatusCode::OK, answer())]);
    let config = json!({});
    let secret = secret();
    let metadata = metadata();
    let response = Kiro
        .stream_generate_content(context(
            &config,
            &secret,
            &metadata,
            client.clone(),
            responses_body(),
        ))
        .await
        .expect("a response");
    assert_eq!(response.headers["content-type"], "text/event-stream");
    let text = String::from_utf8(collect(response.body).await).unwrap();
    let events: Vec<Value> = text
        .split("\n\n")
        .filter_map(|record| record.strip_prefix("data: "))
        .map(|data| serde_json::from_str(data).unwrap())
        .collect();
    let kinds: Vec<&str> = events
        .iter()
        .map(|event| event["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds.first(), Some(&"response.created"));
    assert_eq!(kinds.last(), Some(&"response.completed"));
    assert!(kinds.contains(&"response.reasoning_text.delta"));
    assert!(kinds.contains(&"response.function_call_arguments.done"));

    let text_deltas: Vec<&str> = events
        .iter()
        .filter(|event| event["type"] == "response.output_text.delta")
        .map(|event| event["delta"].as_str().unwrap())
        .collect();
    assert_eq!(
        text_deltas.concat(),
        "Hello there",
        "the repeat is deduped and the percent escape decoded"
    );
    // Every record is numbered once, in order.
    let numbers: Vec<u64> = events
        .iter()
        .map(|event| event["sequence_number"].as_u64().unwrap())
        .collect();
    assert_eq!(numbers, (0..numbers.len() as u64).collect::<Vec<_>>());

    let completed = events.last().unwrap();
    assert_eq!(completed["response"]["status"], "completed");
    assert_eq!(completed["response"]["output_text"], "Hello there");
    assert_eq!(completed["response"]["usage"]["input_tokens"], 45);
    assert_eq!(
        completed["response"]["usage"]["input_tokens_details"]["cached_tokens"],
        30
    );
    let call = completed["response"]["output"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["type"] == "function_call")
        .expect("the tool call");
    assert_eq!(call["call_id"], "call-1");
    assert_eq!(call["arguments"], "{\"p\":\"a\"}");
    assert_eq!(client.sent().len(), 1);
}

#[tokio::test]
async fn a_buffered_caller_gets_the_same_answer_as_one_responses_object() {
    let client = ScriptClient::new(vec![raw_reply(StatusCode::OK, answer())]);
    let config = json!({});
    let secret = secret();
    let metadata = metadata();
    let response = Kiro
        .generate_content(context(
            &config,
            &secret,
            &metadata,
            client,
            responses_body(),
        ))
        .await
        .expect("a response");
    assert_eq!(response.headers["content-type"], "application/json");
    let body: Value = serde_json::from_slice(&collect(response.body).await).unwrap();
    assert_eq!(body["object"], "response");
    assert_eq!(body["status"], "completed");
    assert_eq!(body["output_text"], "Hello there");
    assert_eq!(body["usage"]["output_tokens"], 7);

    // The same object is what the usage extractor reads.
    let headers = HeaderMap::new();
    let encoded = body.to_string();
    let usage = Kiro
        .extract(UsageContext {
            operation: OperationKey {
                operation: Operation::GenerateContent,
                dialect: Dialect::OpenAi,
            },
            request_body: None,
            response: ResponseView {
                status: StatusCode::OK,
                headers: &headers,
                body: encoded.as_bytes(),
            },
        })
        .expect("read")
        .expect("usage");
    assert_eq!(usage.tokens.input_tokens, Some(10));
    assert_eq!(usage.tokens.cached_input_tokens, Some(30));
    assert_eq!(usage.tokens.cache_creation_30m_tokens, Some(5));
    assert_eq!(usage.tokens.output_tokens, Some(7));
}

#[tokio::test]
async fn a_truncated_stream_fails_rather_than_reporting_a_complete_answer() {
    let mut wire = answer();
    wire.truncate(wire.len() - 5);
    let client = ScriptClient::new(vec![raw_reply(StatusCode::OK, wire)]);
    let config = json!({});
    let secret = secret();
    let metadata = metadata();
    let response = Kiro
        .stream_generate_content(context(
            &config,
            &secret,
            &metadata,
            client,
            responses_body(),
        ))
        .await
        .expect("a response");
    let HttpBody::Stream(mut stream) = response.body else {
        panic!("a stream");
    };
    let mut failed = false;
    while let Some(chunk) = stream.next().await {
        failed |= chunk.is_err();
    }
    assert!(failed, "the stream ended inside a frame");
}

#[tokio::test]
async fn an_upstream_failure_is_returned_rather_than_translated() {
    let client = ScriptClient::new(vec![reply(
        StatusCode::TOO_MANY_REQUESTS,
        json!({"message": "slow down"}),
    )]);
    let config = json!({});
    let secret = secret();
    let metadata = metadata();
    let response = Kiro
        .stream_generate_content(context(
            &config,
            &secret,
            &metadata,
            client,
            responses_body(),
        ))
        .await
        .expect("a response");
    assert_eq!(response.status, StatusCode::TOO_MANY_REQUESTS);
}

// ---------------------------------------------------------------- catalogue

#[tokio::test]
async fn the_catalogue_is_rewritten_into_an_openai_list() {
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"models": ["claude-sonnet-4.5", {"modelId": "claude-opus-4.5"}]}),
    )]);
    let config = json!({});
    let secret = secret();
    let metadata = metadata();
    let response = Kiro
        .list_models(OperationContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            dialect: Dialect::OpenAi,
            request: request(HeaderMap::new(), "/v1/models", json!({})),
            client: client.clone(),
            state: Arc::new(NoState::default()),
            instance_id: Arc::from("i"),
            endpoint_override: None,
        })
        .await
        .expect("a response");
    let body: Value = serde_json::from_slice(&collect(response.body).await).unwrap();
    assert_eq!(body["object"], "list");
    assert_eq!(body["data"][0]["id"], "claude-sonnet-4.5");
    assert_eq!(body["data"][1]["id"], "claude-opus-4.5");
    let (_, _, _, sent) = client.sent().into_iter().next().unwrap();
    let sent: Value = serde_json::from_slice(&sent).unwrap();
    assert_eq!(sent["origin"], "KIRO_CLI");
    assert_eq!(
        sent["profileArn"], "arn:aws:codewhisperer:us-east-1:1:profile/P",
        "the API wants the profile in the body as well as the query"
    );
}

// -------------------------------------------------------------------- login

#[tokio::test]
async fn the_device_login_opens_the_desktop_flow_and_polls_it_to_a_credential() {
    let config = json!({"login_provider": "google"});
    let start_client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"deviceCode": "dc-1", "userCode": "ABCD",
               "verificationUri": "https://kiro.dev/device",
               "verificationUriComplete": "https://kiro.dev/device?code=ABCD",
               "intervalInMilliseconds": 3000, "expiresIn": 900}),
    )]);
    let started = Kiro
        .start(LoginContext {
            provider: provider(&config, None),
            client: start_client.as_ref(),
        })
        .await
        .expect("a device authorization");
    let (method, url, _, body) = start_client.sent().into_iter().next().unwrap();
    assert_eq!(method, Method::POST);
    assert_eq!(
        url,
        "https://prod.us-east-1.auth.desktop.kiro.dev/oauth/device/authorization"
    );
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["clientId"], "Kiro-CLI");
    assert_eq!(body["loginProvider"], "Google");
    assert_eq!(started.user_code, "ABCD");
    assert_eq!(
        started.interval_secs, 3,
        "the desktop app states its interval in milliseconds"
    );

    let pending = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"status": "authorization_pending"}),
    )]);
    assert!(matches!(
        Kiro.poll(
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

    let granted = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"status": "authorized", "accessToken": "at", "refreshToken": "rt",
               "profileArn": "arn:aws:codewhisperer:us-east-1:9:profile/Q", "expiresIn": 7200}),
    )]);
    let DevicePoll::Ready(acquired) = Kiro
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
    assert_eq!(
        acquired.provider_fields["profile_arn"], "arn:aws:codewhisperer:us-east-1:9:profile/Q",
        "the profile is a login fact, so prepare can read it back"
    );
    assert!(acquired.expires_at_ms.unwrap() > 0);
}

#[tokio::test]
async fn a_device_authorization_with_nothing_to_open_is_not_one() {
    let config = json!({});
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"deviceCode": "dc", "userCode": "u"}),
    )]);
    assert!(matches!(
        Kiro.start(LoginContext {
            provider: provider(&config, None),
            client: client.as_ref(),
        })
        .await,
        Err(ChannelError::InvalidResponse(_))
    ));
}

#[tokio::test]
async fn the_desktop_refresh_rotates_and_a_named_refusal_is_definitive() {
    let config = json!({});
    let secret = secret();
    let metadata = metadata();

    let rotated = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"accessToken": "at2", "refreshToken": "rt2", "expiresIn": 3600,
               "profileArn": "arn:new"}),
    )]);
    let update = CredentialRefresh::refresh(
        &Kiro,
        CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            client: rotated.as_ref(),
        },
    )
    .await
    .expect("a rotation");
    let (_, url, _, body) = rotated.sent().into_iter().next().unwrap();
    assert_eq!(
        url,
        "https://prod.us-east-1.auth.desktop.kiro.dev/refreshToken"
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap()["refreshToken"],
        "kiro-refresh"
    );
    assert_eq!(update.secret["access_token"], "at2");
    assert_eq!(update.secret["refresh_token"], "rt2");
    assert_eq!(update.secret["provider_fields"]["profile_arn"], "arn:new");
    assert_eq!(
        update.expires_at_ms,
        update.secret["expires_at_ms"].as_i64()
    );

    let rejected = ScriptClient::new(vec![reply(
        StatusCode::BAD_REQUEST,
        json!({"__type": "InvalidGrantException"}),
    )]);
    assert!(matches!(
        CredentialRefresh::refresh(
            &Kiro,
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
                &Kiro,
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
}

#[tokio::test]
async fn an_identity_center_credential_renews_against_aws_oidc() {
    let config = json!({});
    let secret = json!({"access_token": "a", "refresh_token": "r", "client_id": "cid", "client_secret": "csec"});
    let metadata = json!({"region": "eu-west-1"});
    let rotated = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"accessToken": "at2", "refreshToken": "rt2", "expiresIn": 3600}),
    )]);
    CredentialRefresh::refresh(
        &Kiro,
        CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            client: rotated.as_ref(),
        },
    )
    .await
    .expect("a rotation");
    let (_, url, _, body) = rotated.sent().into_iter().next().unwrap();
    assert_eq!(url, "https://oidc.eu-west-1.amazonaws.com/token");
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["grantType"], "refresh_token");
    assert_eq!(
        body["clientSecret"], "csec",
        "the imported credential retains its registered client"
    );
}

/// The login context this channel's authorization-code flow takes.
fn login<'a>(config: &'a Value, client: &'a ScriptClient) -> LoginContext<'a> {
    LoginContext {
        provider: provider(config, None),
        client,
    }
}

fn authorization_request<'a>() -> AuthorizationRequest<'a> {
    AuthorizationRequest {
        // Empty: the channel names the loopback the Kiro CLI registers.
        redirect_uri: "",
        state: "st-1",
        code_challenge: "challenge-1",
    }
}

#[tokio::test]
async fn the_identity_center_login_registers_its_own_client_and_carries_it_to_the_exchange() {
    let config = json!({
        "region": "eu-west-1",
        "sso_start_url": "https://acme.awsapps.com/start",
    });
    let registrar = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"clientId": "dyn-id", "clientSecret": "dyn-secret",
               "clientIdIssuedAt": 1, "clientSecretExpiresAt": 2}),
    )]);
    let started = Kiro
        .authorize(login(&config, &registrar), authorization_request())
        .await
        .expect("an authorize url");

    let (method, url, _, body) = registrar.sent().into_iter().next().unwrap();
    assert_eq!(method, Method::POST);
    assert_eq!(url, "https://oidc.eu-west-1.amazonaws.com/client/register");
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["clientName"], "Kiro-CLI");
    assert_eq!(body["clientType"], "public");
    assert_eq!(
        body["grantTypes"],
        json!(["authorization_code", "refresh_token"])
    );
    assert_eq!(
        body["redirectUris"],
        json!(["http://127.0.0.1:1455/oauth/callback"])
    );
    assert_eq!(body["issuerUrl"], "https://acme.awsapps.com/start");
    assert_eq!(body["scopes"][0], "codewhisperer:completions");

    assert!(
        started
            .authorize_url
            .starts_with("https://oidc.eu-west-1.amazonaws.com/authorize?"),
        "{}",
        started.authorize_url
    );
    assert!(
        started.authorize_url.contains("client_id=dyn-id"),
        "{}",
        started.authorize_url
    );
    assert!(!started.authorize_url.contains("operator-id"));
    assert_eq!(started.provider_state["client_id"], "dyn-id");
    assert_eq!(
        started.provider_state["client_secret"], "dyn-secret",
        "the pocket is where a minted secret waits for the exchange"
    );

    let exchanger = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"accessToken": "at", "refreshToken": "rt", "expiresIn": 3600,
               "profileArn": "arn:aws:codewhisperer:eu-west-1:1:profile/P"}),
    )]);
    let acquired = Kiro
        .exchange(
            login(&config, &exchanger),
            AuthorizationCode {
                code: "code-1",
                redirect_uri: &started.redirect_uri,
                code_verifier: "verifier-1",
                state: "st-1",
                provider_state: &started.provider_state,
            },
        )
        .await
        .expect("a credential");
    let (_, url, _, body) = exchanger.sent().into_iter().next().unwrap();
    assert_eq!(url, "https://oidc.eu-west-1.amazonaws.com/token");
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["grantType"], "authorization_code");
    assert_eq!(body["clientId"], "dyn-id");
    assert_eq!(
        body["clientSecret"], "dyn-secret",
        "the exchange redeems with the client registered for this login"
    );
    assert_eq!(body["codeVerifier"], "verifier-1");

    // The id is a public fact the refresh reads back out of the metadata; the
    // secret is sealed with the tokens and appears nowhere else.
    assert_eq!(acquired.provider_fields["client_id"], "dyn-id");
    assert_eq!(acquired.provider_fields["region"], "eu-west-1");
    assert_eq!(
        acquired.provider_fields["start_url"],
        "https://acme.awsapps.com/start"
    );
    assert!(!acquired.provider_fields.contains_key("client_secret"));
    assert_eq!(acquired.provider_secrets["client_secret"], "dyn-secret");

    // And that is what the host persists: sealed blob yes, rendered metadata
    // no.
    let persisted = AcquiredCredential::from(acquired);
    assert_eq!(
        persisted.secret["provider_secrets"]["client_secret"],
        "dyn-secret"
    );
    assert!(
        !persisted.metadata.to_string().contains("dyn-secret"),
        "no login state may reach the credential's metadata: {}",
        persisted.metadata
    );
    assert_eq!(persisted.metadata["client_id"], "dyn-id");
}

#[tokio::test]
async fn a_refresh_renews_with_the_client_the_login_registered() {
    let config = json!({});
    let secret = json!({"access_token": "a", "refresh_token": "r",
                        "provider_fields": {"client_id": "dyn-id", "region": "eu-west-1"},
                        "provider_secrets": {"client_secret": "dyn-secret"}});
    let metadata = json!({"client_id": "dyn-id", "region": "eu-west-1"});
    let rotated = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"accessToken": "at2", "refreshToken": "rt2", "expiresIn": 3600}),
    )]);
    let update = CredentialRefresh::refresh(
        &Kiro,
        CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            client: rotated.as_ref(),
        },
    )
    .await
    .expect("a rotation");
    let (_, url, _, body) = rotated.sent().into_iter().next().unwrap();
    assert_eq!(url, "https://oidc.eu-west-1.amazonaws.com/token");
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["clientId"], "dyn-id");
    assert_eq!(body["clientSecret"], "dyn-secret");
    // The rotation replaces the tokens and keeps everything else, so the one
    // after it can still renew.
    assert_eq!(
        update.secret["provider_secrets"]["client_secret"],
        "dyn-secret"
    );

    // A credential imported from v3 keeps its flat pair, and it is read the
    // same way.
    let imported = json!({"access_token": "a", "refresh_token": "r",
                          "client_id": "v3-id", "client_secret": "v3-secret",
                          "region": "eu-west-1"});
    let rotated = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"accessToken": "at2", "refreshToken": "rt2", "expiresIn": 3600}),
    )]);
    CredentialRefresh::refresh(
        &Kiro,
        CredentialContext {
            provider: provider(&config, None),
            credential: credential(&imported, &Value::Null),
            client: rotated.as_ref(),
        },
    )
    .await
    .expect("a rotation");
    let (_, _, _, body) = rotated.sent().into_iter().next().unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["clientId"], "v3-id");
    assert_eq!(body["clientSecret"], "v3-secret");
}

// -------------------------------------------------------------------- quota

#[tokio::test]
async fn the_usage_limits_call_reports_a_window_per_resource_type() {
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"usageBreakdownList": [
            {"resourceType": "AGENTIC_REQUEST", "currentUsageWithPrecision": 120.5,
             "usageLimitWithPrecision": 1000.0, "nextDateReset": 1_735_689_600_i64},
        ]}),
    )]);
    let config = json!({});
    let secret = secret();
    let metadata = metadata();
    let snapshot = Kiro
        .query(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            client: client.as_ref(),
        })
        .await
        .expect("a snapshot");
    let (_, url, headers, body) = client.sent().into_iter().next().unwrap();
    assert!(
        url.starts_with("https://management.us-east-1.kiro.dev/?profileArn="),
        "{url}"
    );
    assert!(url.contains("isEmailRequired=true"), "{url}");
    assert_eq!(
        headers["x-amz-target"],
        "AmazonCodeWhispererService.GetUsageLimits"
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap()["origin"],
        "KIRO_CLI"
    );
    assert_eq!(snapshot.entries.len(), 1);
    assert_eq!(snapshot.entries[0].id, AGENTIC_REQUEST_DIMENSION);
    let QuotaValue::Window(window) = &snapshot.entries[0].value else {
        panic!("a window");
    };
    assert_eq!(window.limit, Some("1000".parse().unwrap()));
    assert_eq!(window.period_end_ms, Some(1_735_689_600_000));
    // No `QuotaModel`: every resource type's window is observed, never charged.
    support::assert_quota_contract(None, &[], &snapshot.entries, &[AGENTIC_REQUEST_DIMENSION]);
}

#[tokio::test]
async fn a_quota_probe_without_a_profile_is_an_invalid_credential() {
    let client = ScriptClient::new(Vec::new());
    let config = json!({});
    let secret = secret();
    assert!(matches!(
        Kiro.query(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &Value::Null),
            client: client.as_ref(),
        })
        .await,
        Err(ChannelError::InvalidCredential)
    ));
}

// --------------------------------------------------------------- descriptor

#[test]
fn the_descriptor_names_both_ways_in_and_the_keys_a_form_needs() {
    let descriptor = Kiro.descriptor();
    assert_eq!(descriptor.id, "kiro");
    assert_eq!(
        descriptor.login_modes,
        [LoginMode::DeviceCode, LoginMode::AuthorizationCode]
    );
    assert!(descriptor.capabilities.refresh);
    assert!(descriptor.capabilities.quota_query);
    assert!(!descriptor.capabilities.websocket);
    for key in [
        "region",
        "profile_arn",
        "auth_base_url",
        "management_base_url",
        "login_provider",
        "allowed_headers",
    ] {
        assert!(descriptor.config_key(key).is_some(), "{key}");
    }
}

#[test]
fn the_kiro_app_has_a_client_identity_worth_reproducing() {
    assert!(Kiro.default_connection().is_some());
}
