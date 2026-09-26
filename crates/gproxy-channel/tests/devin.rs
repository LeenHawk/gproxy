#![cfg(feature = "devin")]
//! Scripted upstream only: nothing here reaches server.codeium.com. The
//! frames are built with this channel's own encoder and read back through its
//! own parser, which is the strongest check available without a live account.

use futures_util::StreamExt;
use gproxy_channel::{
    BaseChannel, ChannelError, OutboundClient,
    channel::{
        ChannelState, CredentialContext, CredentialView, LoginMode, NoState, OperationContext,
        ProviderView, QuotaScope, QuotaSubject, QuotaValue, ResponseView, UsageContext, UsageFrame,
        UsageStreamContext, UsageStreamEnd, UsageTransport,
    },
    channels::devin::{self, Devin},
};
use gproxy_protocol::{
    Dialect, HttpBody, WireRequest, WireResponse,
    capability::{CapabilityError, CapabilityFuture},
    connection::{Bytes, StreamFraming},
};
use http::{HeaderMap, Method, StatusCode};
use rust_decimal::Decimal;
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

fn provider(config: &Value) -> ProviderView<'_> {
    ProviderView {
        id: "devin-1",
        channel: "devin",
        base_url: None,
        config,
    }
}

fn credential<'a>(secret: &'a Value, metadata: &'a Value) -> CredentialView<'a> {
    CredentialView {
        id: "cred-1",
        provider_id: "devin-1",
        auth_kind: "api_key",
        secret,
        metadata,
        version: 1,
        expires_at_ms: None,
    }
}

fn context<'a>(
    provider: ProviderView<'a>,
    credential: CredentialView<'a>,
    client: Arc<ScriptClient>,
    body: Value,
) -> OperationContext<'a> {
    OperationContext {
        provider,
        credential,
        dialect: Dialect::OpenAiChat,
        request: WireRequest {
            method: Method::POST,
            path: "/v1/chat/completions".into(),
            query: None,
            headers: HeaderMap::new(),
            body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&body).unwrap())),
        },
        client,
        state: Arc::new(NoState::default()) as Arc<dyn ChannelState>,
        instance_id: Arc::from("local"),
        endpoint_override: None,
    }
}

fn secret() -> Value {
    json!({"session_token": "devin-session-token$abc"})
}

fn request_body() -> Value {
    json!({
        "model": "swe-1-6-slow",
        "max_tokens": 512,
        "messages": [
            {"role": "system", "content": "Be terse."},
            {"role": "user", "content": "Hello?"}
        ]
    })
}

// ── Scripted upstream frames ───────────────────────────────────────────────

/// One response frame carrying a single length-delimited field.
fn text_frame(field: u32, text: &str) -> Vec<u8> {
    let mut message = devin::proto::Message::new();
    message.string(field, text);
    devin::connect::request_frame(message.as_bytes())
}

fn bytes_frame(field: u32, bytes: &[u8]) -> Vec<u8> {
    let mut message = devin::proto::Message::new();
    message.bytes(field, bytes);
    devin::connect::request_frame(message.as_bytes())
}

/// The terminal frame: stop enum plus the metadata sub-message.
fn final_frame(prompt: u64, completion: u64, cached: Option<u64>) -> Vec<u8> {
    final_frame_served_by(prompt, completion, cached, None)
}

/// The same, plus `#7.9 actual_model_uid`.
fn final_frame_served_by(
    prompt: u64,
    completion: u64,
    cached: Option<u64>,
    actual_model: Option<&str>,
) -> Vec<u8> {
    let mut metadata = devin::proto::Message::new();
    metadata.varint(2, prompt).varint(3, completion);
    if let Some(cached) = cached {
        metadata.varint(5, cached);
    }
    if let Some(model) = actual_model {
        metadata.string(9, model);
    }
    let mut message = devin::proto::Message::new();
    message.varint(5, 2).message(7, &metadata);
    devin::connect::request_frame(message.as_bytes())
}

fn trailer() -> Vec<u8> {
    devin::connect::end_of_stream_frame(b"{}")
}

fn error_trailer(code: &str, message: &str) -> Vec<u8> {
    let body = json!({"error": {"code": code, "message": message}});
    devin::connect::end_of_stream_frame(&serde_json::to_vec(&body).unwrap())
}

/// A gzip member wrapping `data` in one stored deflate block. The channel's
/// reader walks the RFC 1952 header itself, so this exercises that walk.
fn gzip_member(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x1f, 0x8b, 0x08, 0x00, 0, 0, 0, 0, 0x00, 0xff];
    out.push(0x01);
    let len = u16::try_from(data.len()).unwrap();
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(data);
    out.extend_from_slice(&0_u32.to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out
}

fn upstream(frames: Vec<Vec<u8>>) -> WireResponse {
    WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(frames.concat())),
    }
}

/// The same frames delivered as several transport chunks, so frame and
/// character boundaries fall in the wrong places on purpose.
fn chunked(chunks: Vec<Vec<u8>>) -> WireResponse {
    let stream = futures_util::stream::iter(
        chunks
            .into_iter()
            .map(|chunk| Ok(Bytes::from(chunk)))
            .collect::<Vec<_>>(),
    );
    WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        body: HttpBody::Stream(Box::pin(stream)),
    }
}

fn json_reply(status: StatusCode, value: Value) -> WireResponse {
    WireResponse {
        status,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&value).unwrap())),
    }
}

async fn drain(response: WireResponse) -> String {
    match response.body {
        HttpBody::Bytes(bytes) => String::from_utf8(bytes.to_vec()).unwrap(),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                out.extend_from_slice(&chunk.unwrap());
            }
            String::from_utf8(out).unwrap()
        }
    }
}

fn chunks(sse: &str) -> Vec<Value> {
    sse.split("\n\n")
        .filter_map(|block| block.strip_prefix("data: "))
        .filter(|data| *data != "[DONE]")
        .map(|data| serde_json::from_str(data).unwrap())
        .collect()
}

// ── Envelope and protobuf ──────────────────────────────────────────────────

#[test]
fn envelope_round_trips_through_the_frame_reader() {
    let mut inner = devin::proto::Message::new();
    inner.string(1, "chisel").varint(2, 7);
    let mut message = devin::proto::Message::new();
    message
        .message(1, &inner)
        .string(2, "system")
        .varint(20, 1)
        .double(5, 0.5)
        .bytes(9, &[0xff, 0x00, 0xff]);
    let frame = devin::connect::request_frame(message.as_bytes());
    assert_eq!(frame[0], 0, "the request envelope is never compressed");
    assert_eq!(
        u32::from_be_bytes([frame[1], frame[2], frame[3], frame[4]]) as usize,
        message.as_bytes().len()
    );

    // Split mid-header and mid-payload: chunk boundaries mean nothing here.
    let mut reader = devin::connect::FrameReader::new();
    assert!(reader.push(&frame[..3]).unwrap().is_empty());
    assert!(reader.push(&frame[3..9]).unwrap().is_empty());
    let frames = reader.push(&frame[9..]).unwrap();
    assert_eq!(frames.len(), 1);
    assert!(!frames[0].end_stream);
    assert_eq!(reader.pending(), 0);

    let fields = devin::proto::parse(&frames[0].payload).unwrap();
    assert_eq!(devin::proto::text_of(&fields, 2), Some("system"));
    assert_eq!(devin::proto::varint_of(&fields, 20), Some(1));
    assert_eq!(
        devin::proto::bytes_of(&fields, 9),
        Some(&[0xff, 0x00, 0xff][..])
    );
    let nested = devin::proto::parse(devin::proto::bytes_of(&fields, 1).unwrap()).unwrap();
    assert_eq!(devin::proto::text_of(&nested, 1), Some("chisel"));
    assert_eq!(devin::proto::varint_of(&nested, 2), Some(7));

    // A truncated message is an error, not a panic and not a short read.
    let truncated = &message.as_bytes()[..message.as_bytes().len() - 2];
    assert!(matches!(
        devin::proto::parse(truncated).unwrap_err(),
        ChannelError::InvalidResponse(_)
    ));
}

#[test]
fn a_compressed_frame_is_inflated_and_the_trailer_is_recognized() {
    let mut message = devin::proto::Message::new();
    message.string(3, "compressed answer");
    let compressed = devin::connect::frame_with_flags(0x01, &gzip_member(message.as_bytes()));
    let mut reader = devin::connect::FrameReader::new();
    let mut frames = reader.push(&compressed).unwrap();
    frames.extend(reader.push(&trailer()).unwrap());
    assert_eq!(frames.len(), 2);
    let fields = devin::proto::parse(&frames[0].payload).unwrap();
    assert_eq!(devin::proto::text_of(&fields, 3), Some("compressed answer"));
    assert!(frames[1].end_stream);
    assert_eq!(devin::connect::trailer_error(&frames[1].payload), None);
    let failure =
        devin::connect::trailer_error(br#"{"error":{"code":"permission_denied","message":"no"}}"#)
            .expect("an error trailer");
    assert_eq!(failure.code.as_deref(), Some("permission_denied"));
    assert_eq!(failure.message, "no");
}

// ── Authentication and identity ────────────────────────────────────────────

#[tokio::test]
async fn the_authorization_header_doubles_the_token_and_the_body_copy_stays_single() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(vec![upstream(vec![
        text_frame(3, "Hi"),
        final_frame(11, 2, None),
        trailer(),
    ])]);
    let response = Devin
        .stream_generate_content(context(
            provider(&config),
            credential(&secret, &metadata),
            client.clone(),
            request_body(),
        ))
        .await
        .unwrap();
    drain(response).await;

    let sent = client.sent();
    assert_eq!(sent.len(), 1);
    let (method, url, headers, body) = &sent[0];
    assert_eq!(method, Method::POST);
    assert_eq!(
        url,
        "https://server.codeium.com/exa.api_server_pb.ApiServerService/GetChatMessage"
    );
    assert_eq!(
        headers.get("authorization").unwrap(),
        "Basic devin-session-token$abc-devin-session-token$abc"
    );
    assert_eq!(
        headers.get("content-type").unwrap(),
        "application/connect+proto"
    );
    assert_eq!(headers.get("connect-protocol-version").unwrap(), "1");
    assert_eq!(headers.get("connect-accept-encoding").unwrap(), "gzip");
    assert_eq!(headers.get("user-agent").unwrap(), "connect-es/2.0.0");

    // The body is one uncompressed envelope; its ClientMetadata carries a
    // single copy of the token and a full-length fingerprint.
    assert_eq!(body[0], 0);
    let fields = devin::proto::parse(&body[5..]).unwrap();
    let metadata = devin::proto::parse(devin::proto::bytes_of(&fields, 1).unwrap()).unwrap();
    assert_eq!(
        devin::proto::text_of(&metadata, 3),
        Some("devin-session-token$abc"),
        "the proto body's token is never doubled"
    );
    let fingerprint = devin::proto::text_of(&metadata, 31).unwrap();
    assert_eq!(fingerprint.len(), devin::FINGERPRINT_HEX_CHARS);
    assert!(fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(
        devin::proto::text_of(&metadata, 1),
        Some(devin::CLIENT_NAME)
    );
    assert_eq!(
        devin::proto::text_of(&metadata, 2),
        Some(devin::CLIENT_VERSION)
    );
    // The selector, the system prompt and the turns.
    assert_eq!(devin::proto::text_of(&fields, 21), Some("swe-1-6-slow"));
    assert_eq!(devin::proto::text_of(&fields, 2), Some("Be terse."));
    let turn = devin::proto::parse(devin::proto::bytes_of(&fields, 3).unwrap()).unwrap();
    assert_eq!(devin::proto::varint_of(&turn, 2), Some(1), "source=user");
    assert_eq!(devin::proto::text_of(&turn, 3), Some("Hello?"));
    // The caller's cap reaches CompletionConfig #2, not #3.
    let completion = devin::proto::parse(devin::proto::bytes_of(&fields, 8).unwrap()).unwrap();
    assert_eq!(devin::proto::varint_of(&completion, 2), Some(512));
    assert_eq!(devin::proto::varint_of(&completion, 3), Some(128_000));
}

#[test]
fn the_fingerprint_is_stable_per_credential_and_shape_checked() {
    let first = devin::derive_fingerprint("token-one");
    assert_eq!(first, devin::derive_fingerprint("token-one"));
    assert_ne!(first, devin::derive_fingerprint("token-two"));
    assert_eq!(first.len(), devin::FINGERPRINT_HEX_CHARS);
    assert!(devin::validate_fingerprint(&first).is_ok());
    assert!(matches!(
        devin::validate_fingerprint(&"ab".repeat(100)).unwrap_err(),
        ChannelError::InvalidCredential
    ));
    assert!(
        matches!(
            devin::validate_fingerprint(&"z".repeat(devin::FINGERPRINT_HEX_CHARS)).unwrap_err(),
            ChannelError::InvalidCredential
        ),
        "732 characters that are not hexadecimal are still the wrong value"
    );
}

#[tokio::test]
async fn a_short_fingerprint_is_refused_before_anything_is_sent() {
    let config = json!({});
    let secret = json!({"session_token": "abc", "fingerprint": "deadbeef"});
    let metadata = json!({});
    let client = ScriptClient::new(Vec::new());
    let error = Devin
        .stream_generate_content(context(
            provider(&config),
            credential(&secret, &metadata),
            client.clone(),
            request_body(),
        ))
        .await
        .unwrap_err();
    assert!(matches!(error, ChannelError::InvalidCredential), "{error}");
    assert!(
        client.sent().is_empty(),
        "a malformed fingerprint must never reach the upstream"
    );
}

#[tokio::test]
async fn a_missing_session_token_is_an_invalid_credential() {
    let config = json!({});
    let secret = json!({"password": "not a session token"});
    let metadata = json!({});
    let client = ScriptClient::new(Vec::new());
    let error = Devin
        .generate_content(context(
            provider(&config),
            credential(&secret, &metadata),
            client.clone(),
            request_body(),
        ))
        .await
        .unwrap_err();
    assert!(matches!(error, ChannelError::InvalidCredential), "{error}");
    assert!(client.sent().is_empty());
}

// ── Request shaping ────────────────────────────────────────────────────────

/// The `(source, text)` of every `ChatMessage` in the one request that was
/// sent, read back through this channel's own parser.
async fn sent_turns(messages: Value) -> Vec<(u64, String)> {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(vec![upstream(vec![
        text_frame(3, "ok"),
        final_frame(1, 1, None),
        trailer(),
    ])]);
    let body = json!({"model": "swe-1-6-slow", "messages": messages});
    let response = Devin
        .stream_generate_content(context(
            provider(&config),
            credential(&secret, &metadata),
            client.clone(),
            body,
        ))
        .await
        .unwrap();
    drain(response).await;
    let sent = client.sent();
    let fields = devin::proto::parse(&sent[0].3[5..]).unwrap();
    fields
        .iter()
        .filter_map(|field| match field.value {
            devin::proto::Value::Bytes(bytes) if field.number == 3 => Some(bytes),
            _ => None,
        })
        .map(|bytes| {
            let turn = devin::proto::parse(bytes).unwrap();
            (
                devin::proto::varint_of(&turn, 2).unwrap(),
                devin::proto::text_of(&turn, 3)
                    .unwrap_or_default()
                    .to_owned(),
            )
        })
        .collect()
}

#[tokio::test]
async fn a_same_source_run_is_merged_only_once_it_would_reach_three() {
    // Two consecutive assistant turns are tolerated by the upstream
    // validator, so they stay two: rewriting a conversation more than the
    // wire requires changes what the model is shown.
    let pair = sent_turns(json!([
        {"role": "user", "content": "one"},
        {"role": "assistant", "content": "a"},
        {"role": "assistant", "content": "b"},
        {"role": "user", "content": "two"},
    ]))
    .await;
    assert_eq!(
        pair,
        vec![
            (1, "one".to_owned()),
            (2, "a".to_owned()),
            (2, "b".to_owned()),
            (1, "two".to_owned()),
        ]
    );

    // A third crosses the threshold the validator rejects, so it folds into
    // the second and the run stays at two.
    let run = sent_turns(json!([
        {"role": "user", "content": "one"},
        {"role": "assistant", "content": "a"},
        {"role": "assistant", "content": "b"},
        {"role": "assistant", "content": "c"},
        {"role": "assistant", "content": "d"},
    ]))
    .await;
    assert_eq!(
        run,
        vec![
            (1, "one".to_owned()),
            (2, "a".to_owned()),
            (2, "b\n\nc\n\nd".to_owned()),
        ]
    );
}

#[tokio::test]
async fn an_image_only_assistant_turn_is_dropped_like_an_empty_one() {
    let turns = sent_turns(json!([
        {"role": "user", "content": "look"},
        {"role": "assistant", "content": [
            {"type": "image_url", "image_url": {"url": "data:image/png;base64,aGk="}}
        ]},
        {"role": "user", "content": "well?"},
    ]))
    .await;
    assert_eq!(
        turns,
        vec![(1, "look".to_owned()), (1, "well?".to_owned())],
        "an assistant turn with no text is excluded whether or not it carries images"
    );
}

#[tokio::test]
async fn a_request_that_declares_tools_is_refused_rather_than_stripped() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(Vec::new());
    let body = json!({
        "model": "swe-1-6-slow",
        "messages": [{"role": "user", "content": "what is the weather"}],
        "tools": [{"type": "function", "function": {"name": "weather", "parameters": {}}}],
    });
    let error = Devin
        .generate_content(context(
            provider(&config),
            credential(&secret, &metadata),
            client.clone(),
            body,
        ))
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("tool calling"),
        "the refusal says what is missing: {error}"
    );
    assert!(
        client.sent().is_empty(),
        "nothing goes upstream with the tools silently dropped"
    );
}

/// The `ClientMetadata` #31 one credential presents.
async fn sent_fingerprint(secret: &Value, credential_id: &'static str) -> String {
    let config = json!({});
    let metadata = json!({});
    let client = ScriptClient::new(vec![upstream(vec![
        text_frame(3, "ok"),
        final_frame(1, 1, None),
        trailer(),
    ])]);
    let credential = CredentialView {
        id: credential_id,
        provider_id: "devin-1",
        auth_kind: "api_key",
        secret,
        metadata: &metadata,
        version: 1,
        expires_at_ms: None,
    };
    let response = Devin
        .stream_generate_content(context(
            provider(&config),
            credential,
            client.clone(),
            request_body(),
        ))
        .await
        .unwrap();
    drain(response).await;
    let sent = client.sent();
    let fields = devin::proto::parse(&sent[0].3[5..]).unwrap();
    let meta = devin::proto::parse(devin::proto::bytes_of(&fields, 1).unwrap()).unwrap();
    devin::proto::text_of(&meta, 31).unwrap().to_owned()
}

#[tokio::test]
async fn the_fingerprint_is_not_derived_from_the_session_token() {
    let token = secret();
    let first = sent_fingerprint(&token, "cred-1").await;
    assert_eq!(first, sent_fingerprint(&token, "cred-1").await, "stable");
    assert_ne!(
        first,
        sent_fingerprint(&token, "cred-2").await,
        "two installations pasting one token must not present the same device"
    );
    assert_ne!(
        first,
        devin::derive_fingerprint("devin-session-token$abc"),
        "a value public to the upstream is never a function of the secret"
    );
    let seeded = json!({"session_token": "devin-session-token$abc", "device_seed": "my-seed"});
    assert_eq!(
        sent_fingerprint(&seeded, "cred-1").await,
        devin::derive_fingerprint("my-seed"),
        "an explicit seed is what survives re-adding the account"
    );
}

// ── Models ─────────────────────────────────────────────────────────────────

#[test]
fn an_unknown_model_is_refused_rather_than_downgraded() {
    let extra = std::collections::BTreeMap::new();
    let error = devin::resolve("gpt-4o", &extra).unwrap_err();
    let ChannelError::InvalidConfig(message) = &error else {
        panic!("expected a configuration refusal: {error}");
    };
    assert!(message.contains("gpt-4o"), "{message}");
    assert!(message.contains("catalogue"), "{message}");
    assert!(
        message.contains(devin::FREE_SELECTOR),
        "the refusal explains why it does not fall back: {message}"
    );
}

#[test]
fn catalogued_aliases_resolve_and_dotted_forms_keep_their_own_target() {
    let extra = std::collections::BTreeMap::new();
    assert_eq!(
        devin::resolve("swe-1-6-slow", &extra).unwrap(),
        "swe-1-6-slow"
    );
    assert_eq!(
        devin::resolve("swe-1.5", &extra).unwrap(),
        "MODEL_SWE_1_5_SLOW"
    );
    assert_eq!(
        devin::resolve("claude-sonnet-4.6", &extra).unwrap(),
        "claude-sonnet-4-6-thinking",
        "the dotted family alias carries the curated thinking default"
    );
    assert_eq!(
        devin::resolve("claude-sonnet-4-6", &extra).unwrap(),
        "claude-sonnet-4-6",
        "the dashed form is itself a catalogue selector"
    );
    assert_eq!(
        devin::resolve("anthropic/claude-opus-4.8", &extra).unwrap(),
        "claude-opus-4-8-medium",
        "a vendor prefix is normalized away"
    );
    assert_eq!(
        devin::resolve("gemini-3-5-flash-medium", &extra).unwrap(),
        "gemini-3-5-flash-medium",
        "a verbatim catalogue target passes through"
    );
    let extra = std::collections::BTreeMap::from([("house-model".to_owned(), "swe-9".to_owned())]);
    assert_eq!(devin::resolve("house-model", &extra).unwrap(), "swe-9");
    assert_eq!(devin::resolve("swe-9", &extra).unwrap(), "swe-9");
    assert!(devin::catalogue(&extra).contains(&"swe-9".to_owned()));
}

#[test]
fn the_captured_snapshot_reaches_every_effort_and_variant_selector() {
    let extra = std::collections::BTreeMap::new();
    // The hand-written alias table names 44 targets; the snapshot carries 123,
    // two of which it does not (`swe-1-6-slow`, captured on an account whose
    // entitlement view omitted it, and `subagent-default`).
    assert_eq!(devin::catalogue(&extra).len(), 125);
    // Everything the alias table used to hard-block, because the channel
    // refuses a name it does not know.
    for selector in [
        "claude-opus-4-8-medium-fast",
        "claude-opus-4-8-xhigh",
        "claude-opus-4-6-thinking",
        "claude-sonnet-4-6-1m",
        "gpt-5-5-medium",
        "gpt-5-4-high-priority",
        "glm-5-2-max",
        "gemini-3-5-flash-minimal",
        "claude-5-fable-max",
        "gpt-5-6-luna-none",
    ] {
        assert_eq!(
            devin::resolve(selector, &extra).unwrap(),
            selector,
            "`{selector}` is in the captured catalogue"
        );
    }
}

#[test]
fn a_router_selector_is_refused_with_its_own_reason() {
    let extra = std::collections::BTreeMap::new();
    for name in ["adaptive", "arena-alpha"] {
        let error = devin::resolve(name, &extra).unwrap_err();
        let ChannelError::InvalidConfig(message) = &error else {
            panic!("expected a configuration refusal: {error}");
        };
        assert!(
            message.contains("AssignModel"),
            "a router fails for a reason of its own: {message}"
        );
    }
    assert!(devin::ROUTER_SELECTORS.contains(&"adaptive"));
}

#[tokio::test]
async fn models_are_listed_locally() {
    let config = json!({"models": {"house-model": "swe-9"}});
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(Vec::new());
    let response = Devin
        .list_models(context(
            provider(&config),
            credential(&secret, &metadata),
            client.clone(),
            json!({}),
        ))
        .await
        .unwrap();
    let body: Value = serde_json::from_str(&drain(response).await).unwrap();
    let ids: Vec<&str> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|model| model["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&devin::FREE_SELECTOR));
    assert!(ids.contains(&"swe-9"));
    assert!(client.sent().is_empty());
}

// ── Streaming ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_multi_frame_response_becomes_ordered_chat_completions_sse() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(vec![upstream(vec![
        text_frame(9, "thinking about it"),
        text_frame(3, "Hello"),
        text_frame(3, ", world"),
        final_frame(40, 7, Some(3)),
        trailer(),
    ])]);
    let response = Devin
        .stream_generate_content(context(
            provider(&config),
            credential(&secret, &metadata),
            client,
            request_body(),
        ))
        .await
        .unwrap();
    assert_eq!(
        response.headers.get("content-type").unwrap(),
        "text/event-stream"
    );
    let sse = drain(response).await;
    assert!(sse.ends_with("data: [DONE]\n\n"), "{sse}");
    let chunks = chunks(&sse);

    assert_eq!(chunks[0]["choices"][0]["delta"]["role"], "assistant");
    assert_eq!(
        chunks[1]["choices"][0]["delta"]["reasoning_content"], "thinking about it",
        "field #9 is the thinking stream, not the answer"
    );
    assert_eq!(chunks[2]["choices"][0]["delta"]["content"], "Hello");
    assert_eq!(chunks[3]["choices"][0]["delta"]["content"], ", world");
    let last = chunks.last().unwrap();
    assert_eq!(last["choices"][0]["finish_reason"], "stop");
    assert_eq!(
        last["usage"]["prompt_tokens"], 43,
        "fresh input plus cache reads"
    );
    assert_eq!(last["usage"]["completion_tokens"], 7);
    assert_eq!(last["usage"]["prompt_tokens_details"]["cached_tokens"], 3);
    for chunk in &chunks {
        assert_eq!(chunk["object"], "chat.completion.chunk");
        assert_eq!(chunk["model"], "swe-1-6-slow");
        assert_eq!(chunk["id"], chunks[0]["id"]);
    }
}

#[tokio::test]
async fn a_character_split_across_frames_and_chunks_survives() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    // "日" is three bytes; the first frame carries two of them.
    let first = "日".as_bytes();
    let frames = [
        bytes_frame(3, &first[..2]),
        bytes_frame(3, &first[2..]),
        final_frame(1, 1, None),
        trailer(),
    ]
    .concat();
    let split = frames.len() / 2;
    let client = ScriptClient::new(vec![chunked(vec![
        frames[..split].to_vec(),
        frames[split..].to_vec(),
    ])]);
    let response = Devin
        .stream_generate_content(context(
            provider(&config),
            credential(&secret, &metadata),
            client,
            request_body(),
        ))
        .await
        .unwrap();
    let text: String = chunks(&drain(response).await)
        .iter()
        .filter_map(|chunk| chunk["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert_eq!(text, "日");
}

#[tokio::test]
async fn an_answer_landing_on_the_callers_cap_reads_as_truncated() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(vec![upstream(vec![
        text_frame(3, "as much as fits"),
        final_frame(10, 512, None),
        trailer(),
    ])]);
    let response = Devin
        .stream_generate_content(context(
            provider(&config),
            credential(&secret, &metadata),
            client,
            request_body(),
        ))
        .await
        .unwrap();
    let chunks = chunks(&drain(response).await);
    assert_eq!(
        chunks.last().unwrap()["choices"][0]["finish_reason"],
        "length"
    );
}

#[tokio::test]
async fn a_stop_sequence_is_enforced_locally_because_the_wire_has_no_field_for_it() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    // The sequence straddles two frames, which is why the translator holds a
    // tail back rather than matching frame by frame.
    let client = ScriptClient::new(vec![upstream(vec![
        text_frame(3, "before END"),
        text_frame(3, "-OF-TEXT and everything after"),
        final_frame(5, 9, None),
        trailer(),
    ])]);
    let mut body = request_body();
    body["stop"] = json!(["END-OF-TEXT"]);
    let response = Devin
        .stream_generate_content(context(
            provider(&config),
            credential(&secret, &metadata),
            client,
            body,
        ))
        .await
        .unwrap();
    let chunks = chunks(&drain(response).await);
    let text: String = chunks
        .iter()
        .filter_map(|chunk| chunk["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert_eq!(
        text, "before ",
        "the answer is cut at the sequence, which is not itself returned"
    );
    assert_eq!(
        chunks.last().unwrap()["choices"][0]["finish_reason"],
        "stop"
    );
}

#[tokio::test]
async fn an_unmatched_stop_sequence_releases_the_held_tail() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(vec![upstream(vec![
        text_frame(3, "nothing to cut here"),
        final_frame(5, 5, None),
        trailer(),
    ])]);
    let mut body = request_body();
    body["stop"] = json!("STOP");
    let response = Devin
        .generate_content(context(
            provider(&config),
            credential(&secret, &metadata),
            client,
            body,
        ))
        .await
        .unwrap();
    let completion: Value = serde_json::from_str(&drain(response).await).unwrap();
    assert_eq!(
        completion["choices"][0]["message"]["content"], "nothing to cut here",
        "the bytes held back against a straddling match are still the answer"
    );
}

#[tokio::test]
async fn the_model_that_actually_served_the_turn_travels_as_usage() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(vec![upstream(vec![
        text_frame(3, "Hello"),
        final_frame_served_by(40, 7, Some(3), Some("claude-opus-4-8-medium")),
        trailer(),
    ])]);
    let response = Devin
        .generate_content(context(
            provider(&config),
            credential(&secret, &metadata),
            client,
            request_body(),
        ))
        .await
        .unwrap();
    let body = drain(response).await;
    let completion: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        completion["model"], "swe-1-6-slow",
        "the response model stays the name the client asked for"
    );
    assert_eq!(
        completion["usage"][devin::ACTUAL_MODEL_KEY],
        "claude-opus-4-8-medium"
    );

    let headers = HeaderMap::new();
    let usage = Devin
        .usage_extractor()
        .unwrap()
        .extract(UsageContext {
            operation: gproxy_protocol::OperationKey {
                operation: gproxy_protocol::Operation::GenerateContent,
                dialect: Dialect::OpenAiChat,
            },
            request_body: None,
            response: ResponseView {
                status: StatusCode::OK,
                headers: &headers,
                body: body.as_bytes(),
            },
        })
        .unwrap()
        .expect("the completion reports usage");
    assert_eq!(
        usage
            .dimensions
            .get(devin::ACTUAL_MODEL_KEY)
            .map(String::as_str),
        Some("claude-opus-4-8-medium"),
        "metering sees what actually ran, and therefore what is billed"
    );
}

#[tokio::test]
async fn an_error_trailer_fails_the_stream_instead_of_ending_it() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    // Delivered as two transport chunks, so the content really is on the wire
    // before the trailer arrives and the status is genuinely spent.
    let client = ScriptClient::new(vec![chunked(vec![
        text_frame(3, "partial"),
        error_trailer("resource_exhausted", "quota exceeded"),
    ])]);
    let response = Devin
        .stream_generate_content(context(
            provider(&config),
            credential(&secret, &metadata),
            client,
            request_body(),
        ))
        .await
        .unwrap();
    let HttpBody::Stream(mut stream) = response.body else {
        panic!("a streamed turn");
    };
    let mut failure = None;
    while let Some(item) = stream.next().await {
        if let Err(error) = item {
            failure = Some(error.to_string());
        }
    }
    // Content was already on the wire, so the status is spent and the
    // classified refusal can only fail the stream — but it keeps its text.
    let failure = failure.expect("the trailer error reaches the caller");
    assert!(failure.contains("resource_exhausted"), "{failure}");
    assert!(failure.contains("quota exceeded"), "{failure}");
    assert!(
        failure.contains("429"),
        "and the class it was given: {failure}"
    );
}

#[tokio::test]
async fn a_refusal_that_arrives_before_any_content_is_answered_with_its_status() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    // The free-tier wall: the trailer of an otherwise empty 200 stream. It is
    // the first thing on the wire, so it is still answerable with a status.
    let client = ScriptClient::new(vec![upstream(vec![error_trailer(
        "permission_denied",
        "Visit /upgrade to access this model",
    )])]);
    let response = Devin
        .stream_generate_content(context(
            provider(&config),
            credential(&secret, &metadata),
            client,
            request_body(),
        ))
        .await
        .unwrap();
    assert_eq!(
        response.status,
        StatusCode::FORBIDDEN,
        "a tier wall is not the permission_denied shell it arrived in"
    );
    let body: Value = serde_json::from_str(&drain(response).await).unwrap();
    assert_eq!(body["error"]["type"], "model_blocked");
    assert_eq!(body["error"]["code"], "permission_denied");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("/upgrade")
    );
}

#[tokio::test]
async fn a_non_2xx_reply_is_classified_rather_than_parsed_as_frames() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    // A transient backend fault, delivered inside the 401 shell this upstream
    // wraps everything in. Reading the status at face value would retire a
    // live session token.
    let client = ScriptClient::new(vec![json_reply(
        StatusCode::UNAUTHORIZED,
        json!({
            "code": "permission_denied",
            "message": "an internal error occurred (trace ID: 9f21c)",
        }),
    )]);
    let response = Devin
        .generate_content(context(
            provider(&config),
            credential(&secret, &metadata),
            client,
            request_body(),
        ))
        .await
        .unwrap();
    assert_eq!(
        response.status,
        StatusCode::BAD_GATEWAY,
        "a backend fault must not present as an authentication failure"
    );
    let body: Value = serde_json::from_str(&drain(response).await).unwrap();
    assert_eq!(body["error"]["type"], "upstream_internal");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("trace ID")
    );
}

// ── The error taxonomy ─────────────────────────────────────────────────────

/// Every branch, with the message strings the evidence records. The order of
/// the branches is the taxonomy: two of these assertions exist only to pin
/// that a later branch does not steal an earlier one's text.
#[test]
fn each_upstream_error_class_is_recognized_by_its_own_evidence() {
    use devin::ErrorClass;
    let case = |message: &str, code: Option<&str>, status: Option<u16>| {
        devin::classify(
            message,
            code,
            status.map(|status| StatusCode::from_u16(status).unwrap()),
        )
    };

    // `internal` is a permanent client mistake: a short fingerprint or a
    // gzipped request frame. It fails identically on every retry.
    let internal = case("an internal error occurred", Some("internal"), Some(500));
    assert_eq!(internal.class, ErrorClass::ClientRequest);
    assert_eq!(internal.class.status(), StatusCode::BAD_REQUEST);

    // The hard throttle, with its Go-duration window. It must win over the
    // capacity branch, whose `try again later` also matches this exact text.
    let throttled = case(
        "Reached message rate limit for this model. Please try again later. Resets in: 3h0m0s",
        Some("permission_denied"),
        Some(403),
    );
    assert_eq!(throttled.class, ErrorClass::RateLimited);
    assert_eq!(throttled.class.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(throttled.reset_ms, Some(3 * 3_600_000));

    // Capacity, as it is actually delivered: inside a 401/403 shell.
    let capacity = case(
        "We're currently facing high demand for this model. Please try again later.",
        Some("permission_denied"),
        Some(403),
    );
    assert_eq!(capacity.class, ErrorClass::Capacity);
    assert_eq!(capacity.class.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        case("the service is temporarily unavailable", None, None).class,
        ErrorClass::Capacity
    );
    assert_eq!(
        case("", Some("unavailable"), None).class,
        ErrorClass::Capacity
    );

    // A transient backend fault. Observed 3/3 while GetUserStatus kept
    // answering on the same token, which is what proves it is not an auth
    // failure — even though it arrives as one.
    let backend = case(
        "an internal error occurred (trace ID: 0c9f2a41)",
        Some("permission_denied"),
        Some(401),
    );
    assert_eq!(backend.class, ErrorClass::UpstreamInternal);
    assert_eq!(backend.class.status(), StatusCode::BAD_GATEWAY);

    // A content-policy block, before the auth branch its code belongs to.
    let blocked = case(
        "Your request was blocked by our content policy. Please remove sensitive or unsafe content from your prompt.",
        Some("permission_denied"),
        Some(403),
    );
    assert_eq!(blocked.class, ErrorClass::ContentBlocked);
    assert_eq!(
        blocked.class.status(),
        StatusCode::BAD_REQUEST,
        "a blocked prompt is a per-request rejection, not a credential fault"
    );

    // Out of money, before the tier wall so it never reads as an upgrade prompt.
    assert_eq!(
        case(
            "insufficient credit balance",
            Some("permission_denied"),
            None
        )
        .class,
        ErrorClass::QuotaExhausted
    );
    // The tier wall itself.
    assert_eq!(
        case("Visit /upgrade to access this model", None, Some(402)).class,
        ErrorClass::ModelBlocked
    );
    assert_eq!(
        case("this model requires a paid plan", None, None).class,
        ErrorClass::ModelBlocked
    );

    // Only now does permission_denied mean what it says.
    let dead = case("invalid session token", Some("unauthenticated"), Some(401));
    assert_eq!(dead.class, ErrorClass::Unauthorized);
    assert_eq!(dead.class.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        case("", None, Some(403)).class,
        ErrorClass::Unauthorized,
        "a bare 403 with nothing to read is an auth failure"
    );

    // A bare 429 outranks the text heuristics below it.
    assert_eq!(
        case("please try again later", None, Some(429)).class,
        ErrorClass::RateLimited
    );
    assert_eq!(
        case("too many requests", None, None).class,
        ErrorClass::RateLimited
    );

    // Nothing matched: transient, which is the reference's own fallback.
    let unknown = case("something else entirely", None, None);
    assert_eq!(unknown.class, ErrorClass::Unknown);
    assert_eq!(unknown.class.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(unknown.message, "something else entirely");
}

#[tokio::test]
async fn a_stream_without_its_trailer_is_not_a_finished_turn() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(vec![upstream(vec![
        text_frame(3, "half an answer"),
        final_frame(3, 3, None),
    ])]);
    let response = Devin
        .stream_generate_content(context(
            provider(&config),
            credential(&secret, &metadata),
            client,
            request_body(),
        ))
        .await
        .unwrap();
    let HttpBody::Stream(mut stream) = response.body else {
        panic!("a streamed turn");
    };
    let mut failed = false;
    while let Some(item) = stream.next().await {
        failed |= item.is_err();
    }
    assert!(
        failed,
        "a truncated Connect stream must not read as complete"
    );
}

// ── Non-streaming and usage ────────────────────────────────────────────────

#[tokio::test]
async fn a_buffered_turn_collects_into_one_completion_with_usage() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(vec![upstream(vec![
        text_frame(9, "think"),
        text_frame(3, "Hello"),
        text_frame(3, " there"),
        final_frame(40, 7, Some(3)),
        trailer(),
    ])]);
    let response = Devin
        .generate_content(context(
            provider(&config),
            credential(&secret, &metadata),
            client,
            request_body(),
        ))
        .await
        .unwrap();
    let body = drain(response).await;
    let completion: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(completion["object"], "chat.completion");
    assert_eq!(
        completion["choices"][0]["message"]["content"],
        "Hello there"
    );
    assert_eq!(
        completion["choices"][0]["message"]["reasoning_content"],
        "think"
    );
    assert_eq!(completion["choices"][0]["finish_reason"], "stop");

    let headers = HeaderMap::new();
    let usage = Devin
        .usage_extractor()
        .unwrap()
        .extract(UsageContext {
            operation: gproxy_protocol::OperationKey {
                operation: gproxy_protocol::Operation::GenerateContent,
                dialect: Dialect::OpenAiChat,
            },
            request_body: None,
            response: ResponseView {
                status: StatusCode::OK,
                headers: &headers,
                body: body.as_bytes(),
            },
        })
        .unwrap()
        .expect("the completion reports usage");
    assert_eq!(
        usage.tokens.input_tokens,
        Some(40),
        "input excludes the cached detail"
    );
    assert_eq!(usage.tokens.cached_input_tokens, Some(3));
    assert_eq!(usage.tokens.output_tokens, Some(7));
}

#[tokio::test]
async fn the_usage_observer_reads_the_terminal_chunk() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(vec![upstream(vec![
        text_frame(3, "Hello"),
        final_frame(40, 7, Some(3)),
        trailer(),
    ])]);
    let response = Devin
        .stream_generate_content(context(
            provider(&config),
            credential(&secret, &metadata),
            client,
            request_body(),
        ))
        .await
        .unwrap();
    let sse = drain(response).await;

    let headers = HeaderMap::new();
    let mut observer = Devin
        .usage_stream()
        .unwrap()
        .start(UsageStreamContext {
            operation: gproxy_protocol::OperationKey {
                operation: gproxy_protocol::Operation::StreamGenerateContent,
                dialect: Dialect::OpenAiChat,
            },
            request_body: None,
            status: StatusCode::OK,
            headers: &headers,
            transport: UsageTransport::Http {
                framing: Some(StreamFraming::Sse),
            },
        })
        .unwrap();
    for chunk in sse.as_bytes().chunks(17) {
        observer.observe(UsageFrame::HttpChunk(chunk)).unwrap();
    }
    let usage = observer
        .finish(UsageStreamEnd::Complete)
        .unwrap()
        .expect("the stream reported usage");
    assert_eq!(usage.tokens.input_tokens, Some(40));
    assert_eq!(usage.tokens.cached_input_tokens, Some(3));
    assert_eq!(usage.tokens.output_tokens, Some(7));
}

#[tokio::test]
async fn a_stream_that_reported_nothing_has_unknown_usage_rather_than_zero() {
    let headers = HeaderMap::new();
    let mut observer = Devin
        .usage_stream()
        .unwrap()
        .start(UsageStreamContext {
            operation: gproxy_protocol::OperationKey {
                operation: gproxy_protocol::Operation::StreamGenerateContent,
                dialect: Dialect::OpenAiChat,
            },
            request_body: None,
            status: StatusCode::OK,
            headers: &headers,
            transport: UsageTransport::Http {
                framing: Some(StreamFraming::Sse),
            },
        })
        .unwrap();
    observer
        .observe(UsageFrame::HttpChunk(
            b"data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n",
        ))
        .unwrap();
    assert!(observer.finish(UsageStreamEnd::Complete).unwrap().is_none());
}

// ── Quota ──────────────────────────────────────────────────────────────────

fn quota_payload(weekly_percent: Option<f64>) -> Value {
    let mut plan = json!({
        "planInfo": {"planName": "Free"},
        "planStart": "2026-09-01T00:00:00Z",
        "planEnd": "2026-10-01T00:00:00Z",
        "dailyQuotaRemainingPercent": 42.5,
        "dailyQuotaResetAtUnix": 1_790_000_000_i64,
        "weeklyQuotaResetAtUnix": "1790500000",
    });
    if let Some(percent) = weekly_percent {
        plan["weeklyQuotaRemainingPercent"] = json!(percent);
    }
    json!({"userStatus": {"planStatus": plan}})
}

#[tokio::test]
async fn the_quota_query_reads_both_windows_from_plan_status() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(vec![json_reply(StatusCode::OK, quota_payload(Some(80.0)))]);
    let provider = provider(&config);
    let credential = credential(&secret, &metadata);
    let snapshot = Devin
        .quota_query()
        .unwrap()
        .query(CredentialContext {
            provider,
            credential,
            client: &*client,
        })
        .await
        .unwrap();

    // The account call is Connect's JSON codec, with the token in the body.
    let sent = client.sent();
    let (_, url, headers, body) = &sent[0];
    assert_eq!(
        url,
        "https://server.codeium.com/exa.seat_management_pb.SeatManagementService/GetUserStatus"
    );
    assert_eq!(headers.get("content-type").unwrap(), "application/json");
    assert_eq!(headers.get("connect-protocol-version").unwrap(), "1");
    assert!(
        headers.get("authorization").is_none(),
        "this endpoint authenticates through the body"
    );
    let body: Value = serde_json::from_slice(body).unwrap();
    assert_eq!(body["metadata"]["apiKey"], "devin-session-token$abc");
    assert_eq!(
        body["metadata"]["ideName"], "windsurf",
        "one mirror's captured identity, verbatim, not a blend of the two"
    );
    assert_eq!(body["metadata"]["extensionName"], "windsurf");
    assert_eq!(body["metadata"]["ideVersion"], "1.9600.41");
    assert!(
        body["metadata"].get("clientName").is_none() && body["metadata"].get("os").is_none(),
        "the capture sends neither"
    );

    assert_eq!(snapshot.entries.len(), 2);
    let daily = &snapshot.entries[0];
    assert_eq!(daily.id, devin::DAILY_ID);
    assert_eq!(daily.source_id, devin::DAILY_ID);
    assert_eq!(daily.label.as_deref(), Some("Free"));
    assert_eq!(daily.subject, QuotaSubject::Account);
    assert_eq!(daily.model_scope, QuotaScope::All);
    let QuotaValue::Window(allowance) = &daily.value else {
        panic!("a window");
    };
    assert_eq!(allowance.remaining, Some(Decimal::try_from(42.5).unwrap()));
    assert_eq!(allowance.used, Some(Decimal::try_from(57.5).unwrap()));
    assert_eq!(allowance.unit.as_deref(), Some("percent"));
    assert_eq!(allowance.period_end_ms, Some(1_790_000_000_000));
    assert_eq!(
        allowance.period_start_ms,
        Some(1_790_000_000_000 - 24 * 60 * 60 * 1000)
    );
    let weekly = &snapshot.entries[1];
    assert_eq!(weekly.id, devin::WEEKLY_ID);
    let QuotaValue::Window(allowance) = &weekly.value else {
        panic!("a window");
    };
    assert_eq!(allowance.remaining, Some(Decimal::from(80)));
    assert_eq!(
        allowance.period_end_ms,
        Some(1_790_500_000_000),
        "a unix second may arrive as a string"
    );
    assert_eq!(
        allowance.period_start_ms,
        Some(1_790_500_000_000 - 7 * 24 * 60 * 60 * 1000)
    );
}

#[tokio::test]
async fn a_spent_window_omits_its_zero_percentage() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(vec![json_reply(StatusCode::OK, quota_payload(None))]);
    let provider = provider(&config);
    let credential = credential(&secret, &metadata);
    let snapshot = Devin
        .quota_query()
        .unwrap()
        .query(CredentialContext {
            provider,
            credential,
            client: &*client,
        })
        .await
        .unwrap();
    let QuotaValue::Window(allowance) = &snapshot.entries[1].value else {
        panic!("a window");
    };
    assert_eq!(
        allowance.remaining,
        Some(Decimal::ZERO),
        "proto3-JSON drops a zero; the reset timestamp says the window is real"
    );
    assert_eq!(allowance.used, Some(Decimal::ONE_HUNDRED));
}

#[tokio::test]
async fn the_billing_ledger_beside_the_windows_is_reported_too() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let mut payload = quota_payload(Some(80.0));
    let plan = &mut payload["userStatus"]["planStatus"];
    plan["overageBalanceMicros"] = json!(2_500_000);
    plan["usedPromptCredits"] = json!(1250);
    plan["availablePromptCredits"] = json!("3750");
    plan["planInfo"]["monthlyPromptCredits"] = json!(5000);
    plan["usedFlexCredits"] = json!(100);
    let client = ScriptClient::new(vec![json_reply(StatusCode::OK, payload)]);
    let snapshot = Devin
        .quota_query()
        .unwrap()
        .query(CredentialContext {
            provider: provider(&config),
            credential: credential(&secret, &metadata),
            client: &*client,
        })
        .await
        .unwrap();
    let entry = |id: &str| {
        snapshot
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .unwrap_or_else(|| panic!("`{id}` is reported"))
    };
    let QuotaValue::Balance(balance) = &entry(devin::OVERAGE_ID).value else {
        panic!("a balance");
    };
    assert_eq!(
        balance.remaining,
        Some(Decimal::try_from(2.5).unwrap()),
        "micro-dollars become dollars"
    );
    assert_eq!(balance.unit.as_deref(), Some("usd"));
    let QuotaValue::Budget(prompt) = &entry(devin::PROMPT_CREDITS_ID).value else {
        panic!("a budget");
    };
    assert_eq!(
        prompt.used,
        Some(Decimal::try_from(12.5).unwrap()),
        "credits arrive in hundredths"
    );
    assert_eq!(prompt.remaining, Some(Decimal::try_from(37.5).unwrap()));
    assert_eq!(prompt.limit, Some(Decimal::from(50)));
    assert_eq!(
        prompt.period_end_ms,
        Some(1_790_812_800_000),
        "the plan period is ISO-8601, not the windows' unix seconds"
    );
    let QuotaValue::Budget(flex) = &entry(devin::FLEX_CREDITS_ID).value else {
        panic!("a budget");
    };
    assert_eq!(flex.used, Some(Decimal::ONE));
    assert_eq!(flex.remaining, None, "absent is unreported, not zero");
}

#[tokio::test]
async fn a_transient_fault_on_the_account_call_is_not_read_as_a_dead_token() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({});
    let client = ScriptClient::new(vec![json_reply(
        StatusCode::UNAUTHORIZED,
        json!({"code": "permission_denied", "message": "We're currently facing high demand."}),
    )]);
    let error = Devin
        .quota_query()
        .unwrap()
        .query(CredentialContext {
            provider: provider(&config),
            credential: credential(&secret, &metadata),
            client: &*client,
        })
        .await
        .unwrap_err();
    let ChannelError::UpstreamResponse { status, .. } = error else {
        panic!("the classified refusal");
    };
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn both_windows_are_declared_as_reported_percentages() {
    let config = json!({});
    let secret = secret();
    let metadata = json!({"plan": "Teams"});
    let dimensions = Devin
        .quota_model()
        .unwrap()
        .dimensions(provider(&config), credential(&secret, &metadata));
    let ids: Vec<&str> = dimensions.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids, vec![devin::DAILY_ID, devin::WEEKLY_ID]);
    assert_eq!(dimensions[0].label.as_deref(), Some("Teams daily window"));
    assert!(dimensions.iter().all(|d| {
        d.limit == Some(Decimal::ONE_HUNDRED)
            && d.tracking == gproxy_channel::channel::QuotaTracking::Reported
    }));
}

// ── Descriptor ─────────────────────────────────────────────────────────────

#[test]
fn the_descriptor_declares_a_pasted_token_and_a_quota_query() {
    let descriptor = Devin.descriptor();
    assert_eq!(descriptor.id, "devin");
    assert_eq!(descriptor.login_modes, vec![LoginMode::ApiKey]);
    assert!(descriptor.capabilities.quota_query);
    assert!(!descriptor.capabilities.refresh);
    assert!(!descriptor.capabilities.websocket);
    assert!(descriptor.config_key("models").is_some());
    assert!(descriptor.config_key("client_version").is_some());
    assert!(
        descriptor.config_key("context_window").is_some(),
        "CompletionConfig #3 is named for what the reference writes into it"
    );
    assert!(descriptor.config_key("allowed_headers").is_some());
    assert!(
        Devin.default_connection().is_none(),
        "connect-es over plain HTTPS has no fingerprint to reproduce"
    );
    assert_eq!(
        Devin.native_dialects(
            provider(&Value::Null),
            gproxy_protocol::Operation::GenerateContent
        ),
        vec![Dialect::OpenAiChat]
    );
}
