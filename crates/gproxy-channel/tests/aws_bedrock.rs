#![cfg(feature = "aws_bedrock")]
//! Everything is scripted: no AWS endpoint is contacted. The SigV4 cases are
//! AWS's own published known answers, so a change in the canonical request,
//! the string to sign or the key derivation fails here rather than in
//! production.

use std::sync::{Arc, Mutex};

use futures_util::StreamExt as _;
use gproxy_channel::{
    BaseChannel, ChannelError, ConfigKeyKind, LoginMode, OperationContext,
    channel::{
        ChannelState, CredentialView, NoState, ProviderView, ResponseView, UsageContext,
        UsageFrame, UsageStreamContext, UsageStreamEnd, UsageTransport,
    },
    channels::aws_bedrock::{AwsBedrock, ID, sigv4},
};
use gproxy_client::OutboundClient;
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    capability::{CapabilityError, CapabilityFuture},
    connection::{Bytes, StreamFraming},
};
use http::{HeaderMap, HeaderValue, Method, StatusCode, Uri};
use serde_json::{Value, json};

// ---------------------------------------------------------------- AWS SigV4

/// `AKIDEXAMPLE` and its secret, the credential of AWS's `aws-sig-v4-test-suite`.
const VECTOR_KEY: &str = "AKIDEXAMPLE";
const VECTOR_SECRET: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";
/// 2015-08-30T12:36:00Z, the instant every suite case signs at.
const VECTOR_SECS: u64 = 1_440_938_160;

fn vector_credentials() -> sigv4::Credentials<'static> {
    sigv4::Credentials {
        access_key_id: VECTOR_KEY,
        secret_access_key: VECTOR_SECRET,
        session_token: None,
    }
}

fn vector_scope() -> sigv4::Scope<'static> {
    sigv4::Scope {
        region: "us-east-1",
        service: "service",
    }
}

fn vector_headers(extra: &[(&'static str, &'static str)]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("x-amz-date", HeaderValue::from_static("20150830T123600Z"));
    for (name, value) in extra {
        headers.insert(*name, HeaderValue::from_static(value));
    }
    headers
}

fn expect_signature(signed_headers: &str, signature: &str) -> String {
    format!(
        "AWS4-HMAC-SHA256 Credential={VECTOR_KEY}/20150830/us-east-1/service/aws4_request, \
         SignedHeaders={signed_headers}, Signature={signature}"
    )
}

#[test]
fn sigv4_reproduces_the_published_aws_test_vectors() {
    let uri: Uri = "https://example.amazonaws.com/".parse().unwrap();

    // aws-sig-v4-test-suite/get-vanilla
    let headers = vector_headers(&[]);
    let value = sigv4::authorization(
        sigv4::SigningRequest {
            method: &Method::GET,
            uri: &uri,
            payload: b"",
        },
        &headers,
        vector_credentials(),
        vector_scope(),
        VECTOR_SECS,
    )
    .unwrap();
    assert_eq!(
        value,
        expect_signature(
            "host;x-amz-date",
            "5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
        )
    );

    // aws-sig-v4-test-suite/get-vanilla-query-order-key-case: the canonical
    // query string sorts by encoded name, whatever order the client sent.
    let ordered: Uri = "https://example.amazonaws.com/?Param2=value2&Param1=value1"
        .parse()
        .unwrap();
    let value = sigv4::authorization(
        sigv4::SigningRequest {
            method: &Method::GET,
            uri: &ordered,
            payload: b"",
        },
        &vector_headers(&[]),
        vector_credentials(),
        vector_scope(),
        VECTOR_SECS,
    )
    .unwrap();
    assert_eq!(
        value,
        expect_signature(
            "host;x-amz-date",
            "b97d918cfa904a5beff61c982a1b6f458b799221646efd99d3219ec94cdf2500"
        )
    );

    // aws-sig-v4-test-suite/post-x-www-form-urlencoded: a signed body.
    let value = sigv4::authorization(
        sigv4::SigningRequest {
            method: &Method::POST,
            uri: &uri,
            payload: b"Param1=value1",
        },
        &vector_headers(&[("content-type", "application/x-www-form-urlencoded")]),
        vector_credentials(),
        vector_scope(),
        VECTOR_SECS,
    )
    .unwrap();
    assert_eq!(
        value,
        expect_signature(
            "content-type;host;x-amz-date",
            "ff11897932ad3f4e8b18135d722051e5ac45fc38421b1da7b9d196a0fe09473a"
        )
    );
}

#[test]
fn sign_adds_the_amz_headers_and_excludes_the_volatile_ones() {
    let uri: Uri = "https://bedrock-runtime.us-east-1.amazonaws.com/model/m/invoke"
        .parse()
        .unwrap();
    let mut headers = HeaderMap::new();
    // A transport rewrites these between signing and the wire, so they must
    // not appear in SignedHeaders.
    headers.insert("user-agent", HeaderValue::from_static("gproxy/4"));
    headers.insert("content-length", HeaderValue::from_static("2"));
    headers.insert("accept-encoding", HeaderValue::from_static("gzip"));
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    sigv4::sign(
        sigv4::SigningRequest {
            method: &Method::POST,
            uri: &uri,
            payload: b"{}",
        },
        &mut headers,
        sigv4::Credentials {
            access_key_id: VECTOR_KEY,
            secret_access_key: VECTOR_SECRET,
            session_token: Some("session-token"),
        },
        sigv4::Scope {
            region: "us-east-1",
            service: "bedrock",
        },
        VECTOR_SECS,
    )
    .unwrap();
    assert_eq!(headers["x-amz-date"], "20150830T123600Z");
    assert_eq!(headers["x-amz-security-token"], "session-token");
    assert_eq!(
        headers["x-amz-content-sha256"],
        // SHA-256 of "{}"
        "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
    );
    let signed = signed_headers(&headers);
    assert_eq!(
        signed,
        "content-type;host;x-amz-content-sha256;x-amz-date;x-amz-security-token"
    );
}

fn signed_headers(headers: &HeaderMap) -> String {
    let value = headers["authorization"].to_str().unwrap();
    value
        .split("SignedHeaders=")
        .nth(1)
        .unwrap()
        .split(',')
        .next()
        .unwrap()
        .to_owned()
}

// ------------------------------------------------------------- test fixtures

struct ScriptClient {
    replies: Mutex<Vec<WireResponse<HttpBody>>>,
    requests: Mutex<Vec<http::Request<HttpBody>>>,
}

impl ScriptClient {
    fn new(replies: Vec<WireResponse<HttpBody>>) -> Arc<Self> {
        Arc::new(Self {
            replies: Mutex::new(replies.into_iter().rev().collect()),
            requests: Mutex::new(Vec::new()),
        })
    }

    fn sent(&self) -> Vec<http::Request<HttpBody>> {
        std::mem::take(&mut self.requests.lock().unwrap())
    }
}

impl OutboundClient for ScriptClient {
    fn send<'a>(
        &'a self,
        request: http::Request<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request);
            Ok(self
                .replies
                .lock()
                .unwrap()
                .pop()
                .expect("the channel made an unscripted call"))
        })
    }
}

fn json_reply(status: StatusCode, value: Value) -> WireResponse<HttpBody> {
    WireResponse {
        status,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&value).unwrap())),
    }
}

fn bytes_reply(chunks: Vec<Bytes>) -> WireResponse<HttpBody> {
    WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        body: HttpBody::Stream(Box::pin(futures_util::stream::iter(
            chunks.into_iter().map(Ok),
        ))),
    }
}

fn provider<'a>(config: &'a Value, base_url: Option<&'a str>) -> ProviderView<'a> {
    ProviderView {
        id: "bedrock-provider",
        channel: ID,
        base_url,
        config,
    }
}

fn credential<'a>(secret: &'a Value, metadata: &'a Value) -> CredentialView<'a> {
    CredentialView {
        id: "bedrock-credential",
        provider_id: "bedrock-provider",
        auth_kind: "api_key",
        secret,
        metadata,
        version: 1,
        expires_at_ms: None,
    }
}

fn key_pair() -> Value {
    json!({"access_key_id": VECTOR_KEY, "secret_access_key": VECTOR_SECRET})
}

fn temporary() -> Value {
    json!({
        "access_key_id": VECTOR_KEY,
        "secret_access_key": VECTOR_SECRET,
        "session_token": "sts-session-token"
    })
}

fn prepared(
    config: &Value,
    base_url: Option<&str>,
    secret: &Value,
    operation: OperationKey,
    request: WireRequest<HttpBody>,
    endpoint_override: Option<&str>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    let metadata = Value::Null;
    AwsBedrock.prepare(gproxy_channel::channel::PrepareContext {
        provider: provider(config, base_url),
        credential: credential(secret, &metadata),
        operation,
        request,
        endpoint_override,
    })
}

fn messages_request(model: &str) -> WireRequest<HttpBody> {
    WireRequest {
        method: Method::POST,
        path: "/v1/messages".into(),
        query: None,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(
            serde_json::to_vec(&json!({
                "model": model,
                "max_tokens": 64,
                "stream": true,
                "messages": [{"role": "user", "content": "hello"}]
            }))
            .unwrap(),
        )),
    }
}

const GENERATE: OperationKey = OperationKey {
    operation: Operation::GenerateContent,
    dialect: Dialect::Claude,
};
const STREAM: OperationKey = OperationKey {
    operation: Operation::StreamGenerateContent,
    dialect: Dialect::Claude,
};
const LIST_MODELS: OperationKey = OperationKey {
    operation: Operation::ListModels,
    dialect: Dialect::OpenAi,
};

// ------------------------------------------------------------------ endpoint

#[test]
fn the_url_is_regional_and_model_scoped() {
    for (region, model, operation, expected) in [
        (
            "us-east-1",
            "anthropic.claude-sonnet-4-5-20250929-v1:0",
            GENERATE,
            "https://bedrock-runtime.us-east-1.amazonaws.com/model/anthropic.claude-sonnet-4-5-20250929-v1%3A0/invoke",
        ),
        (
            "eu-central-1",
            "eu.anthropic.claude-sonnet-4-5-20250929-v1:0",
            STREAM,
            "https://bedrock-runtime.eu-central-1.amazonaws.com/model/eu.anthropic.claude-sonnet-4-5-20250929-v1%3A0/invoke-with-response-stream",
        ),
    ] {
        let config = json!({"region": region});
        let request = prepared(
            &config,
            None,
            &key_pair(),
            operation,
            messages_request(model),
            None,
        )
        .unwrap();
        assert_eq!(request.uri(), expected);
        assert_eq!(request.method(), Method::POST);
    }
}

#[test]
fn the_default_region_and_the_control_plane_are_separate_origins() {
    let config = json!({});
    let request = prepared(
        &config,
        None,
        &key_pair(),
        GENERATE,
        messages_request("anthropic.claude-haiku-4-5-20251001-v1:0"),
        None,
    )
    .unwrap();
    assert!(
        request
            .uri()
            .to_string()
            .starts_with("https://bedrock-runtime.us-east-1.amazonaws.com/"),
        "{}",
        request.uri()
    );

    let list = prepared(
        &config,
        None,
        &key_pair(),
        LIST_MODELS,
        WireRequest {
            method: Method::GET,
            path: "/v1/models".into(),
            query: Some("byProvider=Anthropic&limit=5".into()),
            headers: HeaderMap::new(),
            body: HttpBody::Bytes(Bytes::new()),
        },
        None,
    )
    .unwrap();
    // The control plane is `bedrock.`, not `bedrock-runtime.`, and only the
    // documented directory filters survive.
    assert_eq!(
        list.uri(),
        "https://bedrock.us-east-1.amazonaws.com/foundation-models?byProvider=Anthropic"
    );
    assert_eq!(list.method(), Method::GET);
}

#[test]
fn base_url_replaces_the_runtime_origin_and_endpoint_override_wins() {
    let config = json!({"region": "us-west-2"});
    let vpc = prepared(
        &config,
        Some("https://vpce-1234.bedrock-runtime.us-west-2.vpce.amazonaws.com/"),
        &key_pair(),
        GENERATE,
        messages_request("anthropic.claude-opus-4-1"),
        None,
    )
    .unwrap();
    assert_eq!(
        vpc.uri(),
        "https://vpce-1234.bedrock-runtime.us-west-2.vpce.amazonaws.com/model/anthropic.claude-opus-4-1/invoke"
    );

    let overridden = prepared(
        &config,
        Some("https://ignored.example"),
        &key_pair(),
        GENERATE,
        messages_request("anthropic.claude-opus-4-1"),
        Some("https://gateway.example/bedrock/{model}/go"),
    )
    .unwrap();
    assert_eq!(
        overridden.uri(),
        "https://gateway.example/bedrock/anthropic.claude-opus-4-1/go"
    );
}

#[test]
fn a_body_without_a_model_cannot_address_bedrock() {
    let config = json!({});
    let error = prepared(
        &config,
        None,
        &key_pair(),
        GENERATE,
        WireRequest {
            method: Method::POST,
            path: "/v1/messages".into(),
            query: None,
            headers: HeaderMap::new(),
            body: HttpBody::Bytes(Bytes::from_static(br#"{"max_tokens":1}"#)),
        },
        None,
    )
    .unwrap_err();
    assert!(
        matches!(&error, ChannelError::InvalidConfig(message) if message.contains("model")),
        "{error}"
    );
}

#[test]
fn an_unrecognised_region_is_a_configuration_error() {
    let config = json!({"region": "us east 1"});
    let error = prepared(
        &config,
        None,
        &key_pair(),
        GENERATE,
        messages_request("anthropic.claude-opus-4-1"),
        None,
    )
    .unwrap_err();
    assert!(matches!(error, ChannelError::InvalidConfig(_)), "{error}");
}

// ---------------------------------------------------------------- credential

#[test]
fn the_security_token_header_appears_only_for_a_temporary_credential() {
    let config = json!({"region": "us-east-1"});
    let long_lived = prepared(
        &config,
        None,
        &key_pair(),
        GENERATE,
        messages_request("anthropic.claude-opus-4-1"),
        None,
    )
    .unwrap();
    assert!(!long_lived.headers().contains_key("x-amz-security-token"));
    assert!(
        long_lived.headers()["authorization"]
            .to_str()
            .unwrap()
            .starts_with("AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/")
    );
    assert!(!signed_headers(long_lived.headers()).contains("x-amz-security-token"));

    let temporary_request = prepared(
        &config,
        None,
        &temporary(),
        GENERATE,
        messages_request("anthropic.claude-opus-4-1"),
        None,
    )
    .unwrap();
    assert_eq!(
        temporary_request.headers()["x-amz-security-token"],
        "sts-session-token"
    );
    assert!(signed_headers(temporary_request.headers()).contains("x-amz-security-token"));
}

#[test]
fn a_bedrock_api_key_is_a_bearer_token_and_is_never_signed() {
    let config = json!({"region": "us-east-1"});
    let request = prepared(
        &config,
        None,
        &json!({"api_key": "bedrock-api-key"}),
        GENERATE,
        messages_request("anthropic.claude-opus-4-1"),
        None,
    )
    .unwrap();
    assert_eq!(request.headers()["authorization"], "Bearer bedrock-api-key");
    assert!(!request.headers().contains_key("x-amz-date"));
}

#[test]
fn a_half_filled_key_pair_is_an_invalid_credential() {
    let config = json!({"region": "us-east-1"});
    for secret in [
        json!({"access_key_id": VECTOR_KEY}),
        json!({"secret_access_key": VECTOR_SECRET}),
        json!({"access_key_id": "  ", "secret_access_key": VECTOR_SECRET}),
        json!({}),
    ] {
        let error = prepared(
            &config,
            None,
            &secret,
            GENERATE,
            messages_request("anthropic.claude-opus-4-1"),
            None,
        )
        .unwrap_err();
        assert!(
            matches!(error, ChannelError::InvalidCredential),
            "{secret}: {error}"
        );
    }
}

// --------------------------------------------------------------- the envelope

#[test]
fn the_body_becomes_bedrocks_invoke_envelope() {
    let config = json!({"region": "us-east-1"});
    let mut request = messages_request("anthropic.claude-opus-4-1");
    request
        .headers
        .insert("anthropic-beta", HeaderValue::from_static("a-beta, b-beta"));
    request
        .headers
        .insert("authorization", HeaderValue::from_static("Bearer client"));
    request
        .headers
        .insert("x-amz-date", HeaderValue::from_static("19700101T000000Z"));
    let prepared = prepared(&config, None, &key_pair(), GENERATE, request, None).unwrap();

    let HttpBody::Bytes(body) = prepared.body() else {
        panic!("a signed Bedrock body is always buffered")
    };
    let body: Value = serde_json::from_slice(body).unwrap();
    assert_eq!(body["anthropic_version"], "bedrock-2023-05-31");
    assert!(body.get("model").is_none(), "the model moved into the URL");
    assert!(body.get("stream").is_none(), "the path implies streaming");
    assert_eq!(body["anthropic_beta"], json!(["a-beta", "b-beta"]));
    assert_eq!(body["max_tokens"], 64);

    // The client's own authentication never reaches AWS, and it cannot
    // present the channel's signature material either.
    assert!(
        prepared.headers()["authorization"]
            .to_str()
            .unwrap()
            .starts_with("AWS4-HMAC-SHA256")
    );
    let stamped = prepared.headers()["x-amz-date"].to_str().unwrap();
    assert_ne!(
        stamped, "19700101T000000Z",
        "the client cannot set the date"
    );
    assert!(
        stamped.len() == 16 && stamped.ends_with('Z') && stamped.as_bytes()[8] == b'T',
        "{stamped}"
    );
    assert!(!prepared.headers().contains_key("anthropic-beta"));
    assert_eq!(prepared.headers()["content-type"], "application/json");
    assert_eq!(prepared.headers()["accept"], "application/json");
}

#[test]
fn the_envelope_version_is_configurable() {
    let config = json!({"region": "us-east-1", "anthropic_version": "bedrock-2099-01-01"});
    let prepared = prepared(
        &config,
        None,
        &key_pair(),
        GENERATE,
        messages_request("anthropic.claude-opus-4-1"),
        None,
    )
    .unwrap();
    let HttpBody::Bytes(body) = prepared.body() else {
        panic!("buffered")
    };
    let body: Value = serde_json::from_slice(body).unwrap();
    assert_eq!(body["anthropic_version"], "bedrock-2099-01-01");
}

#[test]
fn a_streamed_request_body_cannot_be_signed() {
    let config = json!({});
    let error = prepared(
        &config,
        None,
        &key_pair(),
        GENERATE,
        WireRequest {
            method: Method::POST,
            path: "/v1/messages".into(),
            query: None,
            headers: HeaderMap::new(),
            body: HttpBody::Stream(Box::pin(futures_util::stream::empty())),
        },
        None,
    )
    .unwrap_err();
    assert!(
        matches!(&error, ChannelError::InvalidConfig(message) if message.contains("SigV4")),
        "{error}"
    );
}

// ------------------------------------------------------------------ dialects

#[test]
fn each_declared_dialect_matches_what_the_upstream_takes() {
    let config = json!({});
    let view = provider(&config, None);
    for operation in [Operation::GenerateContent, Operation::StreamGenerateContent] {
        assert_eq!(
            AwsBedrock.native_dialects(view, operation),
            vec![Dialect::Claude],
            "{operation:?}"
        );
    }
    for operation in [Operation::ListModels, Operation::GetModel] {
        assert_eq!(
            AwsBedrock.native_dialects(view, operation),
            vec![Dialect::OpenAi],
            "{operation:?}"
        );
    }
    // Nothing else is claimed: Converse is not ported, so no other operation
    // has a native shape here.
    for operation in [
        Operation::CountTokens,
        Operation::CreateEmbedding,
        Operation::CreateVideo,
        Operation::CompactContent,
    ] {
        assert!(
            AwsBedrock.native_dialects(view, operation).is_empty(),
            "{operation:?}"
        );
    }
    // And an operation the channel does not serve is rejected in preparation.
    let error = prepared(
        &config,
        None,
        &key_pair(),
        OperationKey {
            operation: Operation::CountTokens,
            dialect: Dialect::Claude,
        },
        messages_request("anthropic.claude-opus-4-1"),
        None,
    )
    .unwrap_err();
    assert!(
        matches!(error, ChannelError::UnsupportedOperation(_)),
        "{error}"
    );
}

#[test]
fn the_descriptor_names_the_keys_the_channel_decodes() {
    let descriptor = AwsBedrock.descriptor();
    assert_eq!(descriptor.id, ID);
    assert_eq!(descriptor.display_name, "AWS Bedrock");
    assert_eq!(descriptor.login_modes, vec![LoginMode::ApiKey]);
    // No refresh: v3 never assumed a role and neither does this channel.
    assert!(!descriptor.capabilities.refresh);
    assert!(!descriptor.capabilities.quota_query);
    assert!(!descriptor.capabilities.websocket);
    for name in [
        "region",
        "base_url",
        "control_base_url",
        "anthropic_version",
    ] {
        assert_eq!(
            descriptor.config_key(name).map(|key| key.kind),
            Some(ConfigKeyKind::String),
            "{name}"
        );
    }
    assert!(descriptor.config_key("credential_strategy").is_some());
    // AWS fingerprints nothing about the client, so no profile is imposed.
    assert!(AwsBedrock.default_connection().is_none());
}

// ----------------------------------------------------------------- streaming

/// One `vnd.amazon.eventstream` frame, built the way Bedrock builds them.
fn event_frame(event_type: &str, payload: Value) -> Vec<u8> {
    let mut headers = Vec::new();
    for (name, value) in [
        (":message-type", "event"),
        (":event-type", event_type),
        (":content-type", "application/json"),
    ] {
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

/// The CRC-32 the event-stream format uses; computed here rather than pulled
/// in as a dependency so the test does not agree with the implementation by
/// construction.
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

/// A `chunk` frame, whose payload is base64 of one Messages event.
fn chunk_frame(event: Value) -> Vec<u8> {
    use base64::Engine as _;
    event_frame(
        "chunk",
        json!({
            "bytes": base64::engine::general_purpose::STANDARD.encode(event.to_string())
        }),
    )
}

fn messages_events() -> Vec<Value> {
    vec![
        json!({"type":"message_start","message":{"id":"msg_bedrock","type":"message",
            "role":"assistant","model":"anthropic.claude-opus-4-1","content":[],
            "stop_reason":null,"usage":{"input_tokens":11,"output_tokens":0,
            "cache_read_input_tokens":4}}}),
        json!({"type":"content_block_start","index":0,
            "content_block":{"type":"text","text":""}}),
        json!({"type":"content_block_delta","index":0,
            "delta":{"type":"text_delta","text":"hi"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},
            "usage":{"output_tokens":7}}),
        json!({"type":"message_stop"}),
    ]
}

fn operation_context<'a>(
    provider: ProviderView<'a>,
    credential: CredentialView<'a>,
    dialect: Dialect,
    request: WireRequest<HttpBody>,
    client: Arc<dyn OutboundClient>,
    state: Arc<dyn ChannelState>,
) -> OperationContext<'a> {
    OperationContext {
        provider,
        credential,
        dialect,
        request,
        client,
        state,
        instance_id: Arc::from("test-instance"),
        endpoint_override: None,
    }
}

async fn collect(body: HttpBody) -> String {
    match body {
        HttpBody::Bytes(bytes) => String::from_utf8(bytes.to_vec()).unwrap(),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                out.extend_from_slice(&chunk.expect("the translated stream is clean"));
            }
            String::from_utf8(out).unwrap()
        }
    }
}

#[tokio::test]
async fn the_event_stream_becomes_claude_messages_sse() {
    let wire = messages_events()
        .into_iter()
        .flat_map(chunk_frame)
        .collect::<Vec<_>>();
    // Deliberately unaligned with frame boundaries.
    let chunks = wire
        .chunks(13)
        .map(Bytes::copy_from_slice)
        .collect::<Vec<_>>();
    let client = ScriptClient::new(vec![bytes_reply(chunks)]);
    let config = json!({"region": "us-east-1"});
    let secret = key_pair();
    let metadata = Value::Null;
    let response = AwsBedrock
        .stream_generate_content(operation_context(
            provider(&config, None),
            credential(&secret, &metadata),
            Dialect::Claude,
            messages_request("anthropic.claude-opus-4-1"),
            client.clone(),
            Arc::new(NoState::default()),
        ))
        .await
        .unwrap();

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.headers["content-type"], "text/event-stream");
    let sent = client.sent();
    assert_eq!(
        sent[0].uri(),
        "https://bedrock-runtime.us-east-1.amazonaws.com/model/anthropic.claude-opus-4-1/invoke-with-response-stream"
    );

    let text = collect(response.body).await;
    assert!(text.starts_with("event: message_start\ndata: {"), "{text}");
    assert!(text.contains("event: content_block_delta\ndata: "));
    assert!(text.contains("\"text_delta\""));
    assert!(text.ends_with("event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"));

    // The translated stream is what the host meters.
    let mut observer = AwsBedrock
        .usage_stream()
        .unwrap()
        .start(UsageStreamContext {
            operation: STREAM,
            request_body: None,
            status: StatusCode::OK,
            headers: &HeaderMap::new(),
            transport: UsageTransport::Http {
                framing: Some(StreamFraming::Sse),
            },
        })
        .unwrap();
    observer
        .observe(UsageFrame::HttpChunk(text.as_bytes()))
        .unwrap();
    let usage = observer.finish(UsageStreamEnd::Complete).unwrap().unwrap();
    assert_eq!(usage.tokens.input_tokens, Some(11));
    assert_eq!(usage.tokens.output_tokens, Some(7));
    assert_eq!(usage.tokens.cached_input_tokens, Some(4));
}

#[tokio::test]
async fn an_exception_frame_closes_the_stream_with_a_messages_error() {
    let mut wire = chunk_frame(messages_events()[0].clone());
    let mut exception = Vec::new();
    for (name, value) in [
        (":message-type", "exception"),
        (":exception-type", "modelStreamErrorException"),
        (":content-type", "application/json"),
    ] {
        exception.push(name.len() as u8);
        exception.extend_from_slice(name.as_bytes());
        exception.push(7);
        exception.extend_from_slice(&(value.len() as u16).to_be_bytes());
        exception.extend_from_slice(value.as_bytes());
    }
    let payload = serde_json::to_vec(&json!({"message": "the model stream failed"})).unwrap();
    let total = 12 + exception.len() + payload.len() + 4;
    let mut frame = Vec::with_capacity(total);
    frame.extend_from_slice(&(total as u32).to_be_bytes());
    frame.extend_from_slice(&(exception.len() as u32).to_be_bytes());
    frame.extend_from_slice(&crc32(&frame).to_be_bytes());
    frame.extend_from_slice(&exception);
    frame.extend_from_slice(&payload);
    frame.extend_from_slice(&crc32(&frame).to_be_bytes());
    wire.extend_from_slice(&frame);

    let client = ScriptClient::new(vec![bytes_reply(vec![Bytes::from(wire)])]);
    let config = json!({});
    let secret = key_pair();
    let metadata = Value::Null;
    let response = AwsBedrock
        .stream_generate_content(operation_context(
            provider(&config, None),
            credential(&secret, &metadata),
            Dialect::Claude,
            messages_request("anthropic.claude-opus-4-1"),
            client,
            Arc::new(NoState::default()),
        ))
        .await
        .unwrap();
    let text = collect(response.body).await;
    assert!(text.contains("event: error\n"), "{text}");
    assert!(text.contains("modelStreamErrorException"), "{text}");
    assert!(text.contains("the model stream failed"), "{text}");
}

#[tokio::test]
async fn a_corrupt_frame_and_a_truncated_stream_both_fail() {
    for corrupt in [true, false] {
        let mut wire = chunk_frame(messages_events()[0].clone());
        if corrupt {
            *wire.last_mut().unwrap() ^= 1;
        } else {
            wire.truncate(wire.len() - 3);
        }
        let client = ScriptClient::new(vec![bytes_reply(vec![Bytes::from(wire)])]);
        let config = json!({});
        let secret = key_pair();
        let metadata = Value::Null;
        let response = AwsBedrock
            .stream_generate_content(operation_context(
                provider(&config, None),
                credential(&secret, &metadata),
                Dialect::Claude,
                messages_request("anthropic.claude-opus-4-1"),
                client,
                Arc::new(NoState::default()),
            ))
            .await
            .unwrap();
        let HttpBody::Stream(mut stream) = response.body else {
            panic!("a translated stream")
        };
        let mut failed = false;
        while let Some(item) = stream.next().await {
            failed |= item.is_err();
        }
        assert!(failed, "corrupt={corrupt}");
    }
}

#[tokio::test]
async fn an_upstream_failure_is_returned_as_it_arrived() {
    let client = ScriptClient::new(vec![json_reply(
        StatusCode::FORBIDDEN,
        json!({"message": "not authorized"}),
    )]);
    let config = json!({});
    let secret = key_pair();
    let metadata = Value::Null;
    let response = AwsBedrock
        .stream_generate_content(operation_context(
            provider(&config, None),
            credential(&secret, &metadata),
            Dialect::Claude,
            messages_request("anthropic.claude-opus-4-1"),
            client,
            Arc::new(NoState::default()),
        ))
        .await
        .unwrap();
    assert_eq!(response.status, StatusCode::FORBIDDEN);
    assert!(!response.headers.contains_key("content-type"));
    assert!(collect(response.body).await.contains("not authorized"));
}

// ------------------------------------------------------------------- usage

#[test]
fn the_buffered_invoke_reply_is_already_a_messages_body() {
    let body = serde_json::to_vec(&json!({
        "id": "msg_1", "type": "message", "role": "assistant",
        "content": [{"type": "text", "text": "hi"}],
        "usage": {"input_tokens": 12, "output_tokens": 5,
            "cache_read_input_tokens": 3,
            "cache_creation": {"ephemeral_5m_input_tokens": 2,
                "ephemeral_1h_input_tokens": 1}}
    }))
    .unwrap();
    let usage = AwsBedrock
        .usage_extractor()
        .unwrap()
        .extract(UsageContext {
            operation: GENERATE,
            request_body: None,
            response: ResponseView {
                status: StatusCode::OK,
                headers: &HeaderMap::new(),
                body: &body,
            },
        })
        .unwrap()
        .unwrap();
    assert_eq!(usage.tokens.input_tokens, Some(12));
    assert_eq!(usage.tokens.output_tokens, Some(5));
    assert_eq!(usage.tokens.cached_input_tokens, Some(3));
    assert_eq!(usage.tokens.cache_creation_5m_tokens, Some(2));
    assert_eq!(usage.tokens.cache_creation_1h_tokens, Some(1));

    // A rejection consumed nothing to meter.
    assert!(
        AwsBedrock
            .usage_extractor()
            .unwrap()
            .extract(UsageContext {
                operation: GENERATE,
                request_body: None,
                response: ResponseView {
                    status: StatusCode::TOO_MANY_REQUESTS,
                    headers: &HeaderMap::new(),
                    body: &body,
                },
            })
            .unwrap()
            .is_none()
    );
}

// ------------------------------------------------------------------- models

#[tokio::test]
async fn the_foundation_model_directory_becomes_an_openai_list() {
    let client = ScriptClient::new(vec![json_reply(
        StatusCode::OK,
        json!({"modelSummaries": [
            {"modelId": "anthropic.claude-opus-4-1", "modelName": "Claude Opus 4.1",
             "providerName": "Anthropic", "outputModalities": ["TEXT"],
             "modelLifecycle": {"status": "ACTIVE"}},
            {"modelId": "amazon.retired-v1", "providerName": "Amazon",
             "modelLifecycle": {"status": "LEGACY"}},
            {"modelId": "stability.image-v1", "providerName": "Stability AI",
             "outputModalities": ["IMAGE"]}
        ]}),
    )]);
    let config = json!({});
    let secret = key_pair();
    let metadata = Value::Null;
    let response = AwsBedrock
        .list_models(operation_context(
            provider(&config, None),
            credential(&secret, &metadata),
            Dialect::OpenAi,
            WireRequest {
                method: Method::GET,
                path: "/v1/models".into(),
                query: None,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::new()),
            },
            client,
            Arc::new(NoState::default()),
        ))
        .await
        .unwrap();
    let body: Value = serde_json::from_str(&collect(response.body).await).unwrap();
    assert_eq!(body["object"], "list");
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["id"], "anthropic.claude-opus-4-1");
    assert_eq!(body["data"][0]["object"], "model");
    assert_eq!(body["data"][0]["owned_by"], "Anthropic");
    assert_eq!(body["data"][0]["display_name"], "Claude Opus 4.1");
}

#[tokio::test]
async fn one_foundation_model_becomes_an_openai_model() {
    let client = ScriptClient::new(vec![json_reply(
        StatusCode::OK,
        json!({"modelDetails": {"modelId": "anthropic.claude-opus-4-1",
            "modelName": "Claude Opus 4.1", "providerName": "Anthropic"}}),
    )]);
    let config = json!({});
    let secret = key_pair();
    let metadata = Value::Null;
    let response = AwsBedrock
        .get_model(operation_context(
            provider(&config, None),
            credential(&secret, &metadata),
            Dialect::OpenAi,
            WireRequest {
                method: Method::GET,
                path: "/v1/models/anthropic.claude-opus-4-1".into(),
                query: None,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::new()),
            },
            client.clone(),
            Arc::new(NoState::default()),
        ))
        .await
        .unwrap();
    assert_eq!(
        client.sent()[0].uri(),
        "https://bedrock.us-east-1.amazonaws.com/foundation-models/anthropic.claude-opus-4-1"
    );
    let body: Value = serde_json::from_str(&collect(response.body).await).unwrap();
    assert_eq!(body["id"], "anthropic.claude-opus-4-1");
    assert_eq!(body["object"], "model");
    assert_eq!(body["owned_by"], "Anthropic");
}

#[test]
fn raw_bedrock_stream_usage_records_refusal_and_actual_model() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        "application/vnd.amazon.eventstream".parse().unwrap(),
    );
    let mut observer = AwsBedrock
        .usage_stream()
        .unwrap()
        .start(UsageStreamContext {
            operation: OperationKey {
                operation: Operation::StreamGenerateContent,
                dialect: Dialect::Claude,
            },
            request_body: None,
            status: StatusCode::OK,
            headers: &headers,
            transport: UsageTransport::Http { framing: None },
        })
        .unwrap();
    let frames = [
        chunk_frame(json!({"type":"message_start","message":{"model":"claude-fable-5","usage":{"input_tokens":20,"output_tokens":0}}})),
        chunk_frame(json!({"type":"message_delta","delta":{"stop_reason":"refusal"},"usage":{"output_tokens":0}})),
        chunk_frame(json!({"type":"message_stop"})),
    ].concat();
    for bytes in frames.chunks(3) {
        observer.observe(UsageFrame::HttpChunk(bytes)).unwrap();
    }
    let usage = observer.finish(UsageStreamEnd::Complete).unwrap().unwrap();
    assert_eq!(usage.tokens.input_tokens, Some(20));
    assert_eq!(usage.attempts[0].model, "claude-fable-5");
    assert_eq!(usage.attempts[0].billable, Some(false));
}

#[test]
fn a_magic_cache_string_is_stripped_and_marked_only_when_asked() {
    const TOKEN: &str = "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_\
                         49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF";
    let body = |config: Value| {
        let mut request = messages_request("anthropic.claude-opus-4-1");
        request.body = HttpBody::Bytes(Bytes::from(
            json!({"model": "anthropic.claude-opus-4-1", "max_tokens": 64, "messages": [
                {"role": "user", "content": [{"type": "text", "text": format!("ctx {TOKEN}")}]}
            ]})
            .to_string(),
        ));
        let prepared = prepared(&config, None, &key_pair(), GENERATE, request, None).unwrap();
        let HttpBody::Bytes(body) = prepared.body() else {
            panic!("buffered")
        };
        serde_json::from_slice::<Value>(body).unwrap()
    };
    let off = body(json!({"region": "us-east-1"}));
    let block = &off["messages"][0]["content"][0];
    assert!(
        !block["text"].as_str().unwrap().contains("GPROXY_MAGIC"),
        "stripped either way"
    );
    assert!(block["cache_control"].is_null());
    let on = body(json!({"region": "us-east-1", "enable_claude_magic_cache": true}));
    assert_eq!(
        on["messages"][0]["content"][0]["cache_control"]["type"],
        "ephemeral"
    );
}

#[test]
fn the_wire_follows_the_model() {
    let config = json!({});
    let native = |model: &str| {
        AwsBedrock.native_dialects_for_model(
            provider(&config, None),
            Operation::GenerateContent,
            Some(model),
        )
    };
    for messages in [
        "anthropic.claude-opus-4-8",
        "us.anthropic.claude-sonnet-5",
        "arn:aws:bedrock:us-east-1::foundation-model/anthropic.claude-haiku-4-5",
        "arn:aws:bedrock:us-east-1:123456789012:application-inference-profile/abc123",
    ] {
        assert_eq!(native(messages), [Dialect::Claude], "{messages}");
    }
    for chat in [
        "openai.gpt-5.5",
        "xai.grok-4-3",
        "qwen.qwen3-32b-v1:0",
        "deepseek.v3-v1:0",
    ] {
        assert_eq!(native(chat), [Dialect::OpenAiChat], "{chat}");
    }
    // Without a model, the provider-wide answer stays Messages.
    assert_eq!(
        AwsBedrock.native_dialects(provider(&config, None), Operation::GenerateContent),
        [Dialect::Claude]
    );
}

#[test]
fn a_chat_completions_model_is_sent_to_the_openai_surface() {
    const TOKEN: &str = "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_\
                         49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF";
    let config = json!({"region": "us-west-2", "enable_openai_magic_cache": true});
    let request = WireRequest {
        method: Method::POST,
        path: "/v1/chat/completions".into(),
        query: None,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(
            json!({"model": "openai.gpt-5.5", "stream": true, "messages": [
                {"role": "user", "content": [{"type": "text", "text": format!("ctx {TOKEN}")}]}
            ]})
            .to_string(),
        )),
    };
    let operation = OperationKey {
        operation: Operation::StreamGenerateContent,
        dialect: Dialect::OpenAiChat,
    };
    let prepared = prepared(&config, None, &key_pair(), operation, request, None).unwrap();
    assert_eq!(
        prepared.uri(),
        "https://bedrock-runtime.us-west-2.amazonaws.com/openai/v1/chat/completions"
    );
    assert_eq!(prepared.headers()["accept"], "text/event-stream");
    assert!(
        prepared.headers()["authorization"]
            .to_str()
            .unwrap()
            .starts_with("AWS4-HMAC-SHA256"),
        "signed as the bedrock service like every other call"
    );
    let HttpBody::Bytes(body) = prepared.body() else {
        panic!("buffered")
    };
    let body: Value = serde_json::from_slice(body).unwrap();
    assert_eq!(
        body["model"], "openai.gpt-5.5",
        "the model stays in the body"
    );
    assert!(
        body.get("stream_options").is_none(),
        "core asks a Chat stream for its usage; the channel forwards the body"
    );
    let text = body["messages"][0]["content"][0]["text"].as_str().unwrap();
    assert!(!text.contains("GPROXY_MAGIC"));
    assert!(!body["messages"][0]["content"][0]["prompt_cache_breakpoint"].is_null());
}
