#![cfg(feature = "grokbuild")]
//! Grok Build against a scripted client: no real upstream is called.

mod support;

use gproxy_channel::channel::{
    BaseChannel, ChannelError, CredentialContext, CredentialRefresh, CredentialView, DevicePoll,
    LoginContext, OAuthDeviceCode, PrepareContext, ProviderView, QuotaQuery, QuotaValue,
};
use gproxy_channel::channels::grokbuild::{
    CLI_USER_AGENT, COST_TICKS_METRIC, DEFAULT_CLIENT_ID, GrokBuild, UPSTREAM_COST_METRIC,
    UPSTREAM_PRICED_DIMENSION, USAGE_DIMENSION,
};
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
        id: "g",
        channel: "grokbuild",
        base_url,
        config,
    }
}

fn credential<'a>(secret: &'a Value, metadata: &'a Value) -> CredentialView<'a> {
    CredentialView {
        id: "c",
        provider_id: "g",
        auth_kind: "oauth",
        secret,
        metadata,
        version: 2,
        expires_at_ms: None,
    }
}

fn secret() -> Value {
    json!({"access_token": "grok-access", "refresh_token": "grok-refresh"})
}

/// What the login read out of the id token, as the host publishes it.
fn metadata() -> Value {
    json!({"sub": "user-1", "user_email": "a@b.c"})
}

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

fn prepare(
    config: &Value,
    base_url: Option<&str>,
    metadata: &Value,
    operation: Operation,
    dialect: Dialect,
    request: WireRequest<HttpBody>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    let secret = secret();
    GrokBuild.prepare(PrepareContext {
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

// ------------------------------------------------------------------ prepare

#[test]
fn generation_stays_on_the_chat_proxy_and_carries_the_cli_identity() {
    let prepared = prepare(
        &json!({}),
        None,
        &metadata(),
        Operation::StreamGenerateContent,
        Dialect::OpenAi,
        request(
            HeaderMap::new(),
            "/v1/responses",
            json!({"model": "grok-4", "input": "hi", "stream": true}),
        ),
    )
    .expect("prepared");
    assert_eq!(
        prepared.uri(),
        "https://cli-chat-proxy.grok.com/v1/responses"
    );
    let headers = prepared.headers();
    assert_eq!(headers["authorization"], "Bearer grok-access");
    assert_eq!(headers["x-xai-token-auth"], "xai-grok-cli");
    assert_eq!(headers["x-authenticateresponse"], "authenticate-response");
    assert_eq!(headers["x-grok-client-version"], "1.0.45");
    assert_eq!(headers["x-grok-client-identifier"], "grok-shell");
    assert_eq!(headers["x-grok-client-mode"], "headless");
    assert_eq!(
        headers["user-agent"],
        format!(
            "{CLI_USER_AGENT} ({}; {})",
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    );
    assert_eq!(
        headers["x-grok-user-id"], "user-1",
        "the subject the login read out of the id token"
    );
    assert_eq!(
        headers["accept"], "text/event-stream",
        "a streamed answer is asked for as one"
    );
    assert!(headers.get("host").is_none());
}

#[test]
fn media_leaves_the_chat_proxy_for_xais_own_origin() {
    for (operation, path, expected) in [
        (
            Operation::CreateSpeech,
            "/v1/audio/speech",
            "https://api.x.ai/v1/tts",
        ),
        (
            Operation::CreateTranscription,
            "/v1/audio/transcriptions",
            "https://api.x.ai/v1/stt",
        ),
        (
            Operation::CreateVideo,
            "/v1/videos",
            "https://api.x.ai/v1/videos/generations",
        ),
        (
            Operation::CreateImage,
            "/v1/images/generations",
            "https://api.x.ai/v1/images/generations",
        ),
    ] {
        let prepared = prepare(
            &json!({}),
            None,
            &metadata(),
            operation,
            Dialect::OpenAi,
            request(HeaderMap::new(), path, json!({"input": "hi"})),
        )
        .expect("prepared");
        assert_eq!(prepared.uri(), expected, "{operation:?}");
    }

    let listed = prepare(
        &json!({}),
        None,
        &metadata(),
        Operation::ListModels,
        Dialect::OpenAi,
        request(HeaderMap::new(), "/v1/models", json!({})),
    )
    .expect("prepared");
    assert_eq!(listed.uri(), "https://cli-chat-proxy.grok.com/v1/models");
}

#[test]
fn a_speech_request_is_renamed_and_asked_for_as_audio() {
    let prepared = prepare(
        &json!({}),
        None,
        &metadata(),
        Operation::CreateSpeech,
        Dialect::OpenAi,
        request(
            HeaderMap::new(),
            "/v1/audio/speech",
            json!({"model": "grok-voice", "input": "hello", "voice": "ara",
                   "response_format": "mp3"}),
        ),
    )
    .expect("prepared");
    assert_eq!(prepared.headers()["accept"], "audio/*");
    assert_eq!(
        body_json(&prepared),
        json!({"text": "hello", "voice_id": "ara", "output_format": {"codec": "mp3"}})
    );
}

#[test]
fn the_session_the_ladder_reads_survives_and_the_conversation_id_is_the_channels() {
    let mut caller = HeaderMap::new();
    caller.insert("x-grok-session-id", HeaderValue::from_static("sess-42"));
    caller.insert("x-grok-conv-id", HeaderValue::from_static("forged"));
    caller.insert("x-grok-user-id", HeaderValue::from_static("someone-else"));
    let prepared = prepare(
        &json!({}),
        None,
        &metadata(),
        Operation::GenerateContent,
        Dialect::OpenAi,
        request(
            caller,
            "/v1/responses",
            json!({"model": "grok-4", "input": "hi", "prompt_cache_key": "conv-9"}),
        ),
    )
    .expect("prepared");
    let headers = prepared.headers();
    assert_eq!(
        headers["x-grok-session-id"], "sess-42",
        "the session id the ladder reads is neither set nor dropped here"
    );
    assert_eq!(
        headers["x-grok-conv-id"], "conv-9",
        "the conversation id mirrors the body, not the client's header"
    );
    assert_eq!(headers["x-grok-user-id"], "user-1");
}

#[test]
fn a_provider_allow_list_cannot_hide_the_session_but_narrows_the_rest() {
    let mut caller = HeaderMap::new();
    caller.insert("x-grok-session-id", HeaderValue::from_static("sess-42"));
    caller.insert("x-trace", HeaderValue::from_static("keep-me"));
    caller.insert("x-other", HeaderValue::from_static("drop-me"));
    let prepared = prepare(
        &json!({"allowed_headers": ["x-trace"]}),
        None,
        &metadata(),
        Operation::GenerateContent,
        Dialect::OpenAi,
        request(
            caller,
            "/v1/responses",
            json!({"model": "grok-4", "input": "hi"}),
        ),
    )
    .expect("prepared");
    let headers = prepared.headers();
    assert_eq!(headers["x-grok-session-id"], "sess-42");
    assert_eq!(headers["x-trace"], "keep-me");
    assert!(headers.get("x-other").is_none());
}

#[test]
fn the_responses_body_is_narrowed_to_what_the_proxy_accepts() {
    let prepared = prepare(
        &json!({}),
        None,
        &metadata(),
        Operation::GenerateContent,
        Dialect::OpenAi,
        request(
            HeaderMap::new(),
            "/v1/responses",
            json!({
                "model": "grok-4", "previous_response_id": "resp_1",
                "metadata": {"a": 1}, "top_p": 0.0, "temperature": 0.3,
                "include": ["reasoning.encrypted_content"],
                "reasoning": {"effort": "high"},
                "tools": [{"type": "tool_search"}, {"type": "function", "name": "f"}],
                "input": [
                    {"type": "compaction", "encrypted_content": "gAAAAnope"},
                    {"role": "user", "content": "hi"},
                ],
            }),
        ),
    )
    .expect("prepared");
    let body = body_json(&prepared);
    assert!(body.get("previous_response_id").is_none());
    assert!(body.get("metadata").is_none());
    assert!(body.get("top_p").is_none());
    assert_eq!(body["include"], json!(["reasoning.encrypted_content"]));
    assert_eq!(body["reasoning"]["effort"], "high");
    assert_eq!(body["temperature"], 0.3);
    let tools = body["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "f");
    let input = body["input"].as_array().unwrap();
    assert_eq!(input.len(), 1, "the compaction blob had nothing behind it");
    assert_eq!(input[0]["role"], "user");
}

#[test]
fn a_chat_body_reaches_the_proxy_as_the_caller_wrote_it() {
    let prepared = prepare(
        &json!({}),
        None,
        &metadata(),
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        request(
            HeaderMap::new(),
            "/v1/chat/completions",
            json!({"model": "grok-4", "messages": [], "metadata": {"a": 1}}),
        ),
    )
    .expect("prepared");
    assert_eq!(
        prepared.uri(),
        "https://cli-chat-proxy.grok.com/v1/chat/completions"
    );
    assert_eq!(
        body_json(&prepared)["metadata"],
        json!({"a": 1}),
        "the Responses narrowing is not applied to a Chat body"
    );
}

#[test]
fn a_credential_without_an_access_token_is_refused() {
    let secret = json!({"refresh_token": "r"});
    assert!(matches!(
        GrokBuild.prepare(PrepareContext {
            provider: provider(&json!({}), None),
            credential: credential(&secret, &metadata()),
            operation: OperationKey {
                operation: Operation::GenerateContent,
                dialect: Dialect::OpenAi,
            },
            request: request(HeaderMap::new(), "/v1/responses", json!({"input": "hi"})),
            endpoint_override: None,
        }),
        Err(ChannelError::InvalidCredential)
    ));
}

#[test]
fn the_declared_dialects_follow_the_surface() {
    let config = json!({});
    assert_eq!(
        GrokBuild.native_dialects(provider(&config, None), Operation::GenerateContent),
        [Dialect::OpenAi, Dialect::OpenAiChat]
    );
    assert_eq!(
        GrokBuild.native_dialects(provider(&config, None), Operation::CreateSpeech),
        [Dialect::OpenAi]
    );
    assert!(
        GrokBuild
            .native_dialects(provider(&config, None), Operation::CountTokens)
            .is_empty()
    );
    assert!(matches!(
        prepare(
            &config,
            None,
            &metadata(),
            Operation::CountTokens,
            Dialect::OpenAi,
            request(HeaderMap::new(), "/v1/responses", json!({})),
        ),
        Err(ChannelError::UnsupportedOperation(_))
    ));
}

// -------------------------------------------------------------------- usage

#[test]
fn xais_metering_unit_and_a_jobs_own_price_are_both_recorded() {
    let body = json!({"usage": {"input_tokens": 100, "output_tokens": 20,
                                "input_tokens_details": {"cached_tokens": 30,
                                                         "image_tokens": 12},
                                "cost_in_usd_ticks": 4200}})
    .to_string();
    let headers = HeaderMap::new();
    let usage = support::settled(
        &GrokBuild,
        Operation::GenerateContent,
        Dialect::OpenAi,
        &headers,
        body.as_bytes(),
    )
    .expect("usage");
    assert_eq!(usage.tokens.input_tokens, Some(70));
    assert_eq!(usage.tokens.cached_input_tokens, Some(30));
    assert_eq!(usage.metrics[COST_TICKS_METRIC], 4200.into());
    assert_eq!(usage.metrics["image_input_tokens"], 12.into());

    let job = json!({"cost_usd": "0.42", "duration": 6}).to_string();
    let usage = support::settled(
        &GrokBuild,
        Operation::RetrieveVideo,
        Dialect::OpenAi,
        &headers,
        job.as_bytes(),
    )
    .expect("usage");
    assert_eq!(usage.metrics[UPSTREAM_COST_METRIC], "0.42".parse().unwrap());
    assert_eq!(usage.dimensions[UPSTREAM_PRICED_DIMENSION], "true");
    assert_eq!(usage.metrics["video_seconds"], 6.into());
}

#[test]
fn a_responses_stream_settles_on_its_completed_event() {
    let wire = concat!(
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"usage\":",
        "{\"input_tokens\":9,\"output_tokens\":3,\"cost_in_usd_ticks\":7}}}\n\n"
    );
    let usage = support::settled_stream(
        &GrokBuild,
        Operation::StreamGenerateContent,
        Dialect::OpenAi,
        &HeaderMap::new(),
        wire.as_bytes(),
    )
    .expect("usage");
    assert_eq!(usage.tokens.input_tokens, Some(9));
    assert_eq!(usage.tokens.output_tokens, Some(3));
    assert_eq!(usage.metrics[COST_TICKS_METRIC], 7.into());
}

// -------------------------------------------------------------------- login

#[tokio::test]
async fn the_device_login_reads_the_account_out_of_the_id_token() {
    use base64::Engine as _;
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(br#"{"sub":"user-9","email":"nine@example.com"}"#);
    let id_token = format!("header.{payload}.signature");
    let config = json!({});

    let start = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"device_code": "dc-1", "user_code": "ABCD-EFGH",
               "verification_uri": "https://auth.x.ai/device",
               "verification_uri_complete": "https://auth.x.ai/device?code=ABCD",
               "interval": 5, "expires_in": 900}),
    )]);
    let started = OAuthDeviceCode::start(
        &GrokBuild,
        LoginContext {
            provider: provider(&config, None),
            client: start.as_ref(),
        },
    )
    .await
    .expect("a device authorization");
    let (method, url, headers, body) = start.sent().into_iter().next().unwrap();
    assert_eq!(method, Method::POST);
    assert_eq!(url, "https://auth.x.ai/oauth2/device/code");
    assert_eq!(headers["content-type"], "application/x-www-form-urlencoded");
    let form = String::from_utf8(body).unwrap();
    assert!(
        form.contains(&format!("client_id={DEFAULT_CLIENT_ID}")),
        "{form}"
    );
    assert!(form.contains("grok-cli%3Aaccess"), "{form}");
    assert_eq!(started.user_code, "ABCD-EFGH");
    assert_eq!(started.interval_secs, 5);

    for (error, expected) in [
        ("authorization_pending", "pending"),
        ("slow_down", "slow"),
        ("access_denied", "denied"),
        ("expired_token", "expired"),
    ] {
        let client = ScriptClient::new(vec![reply(
            StatusCode::BAD_REQUEST,
            json!({ "error": error }),
        )]);
        let polled = GrokBuild
            .poll(
                LoginContext {
                    provider: provider(&config, None),
                    client: client.as_ref(),
                },
                &started,
            )
            .await
            .unwrap();
        let named = match polled {
            DevicePoll::Pending => "pending",
            DevicePoll::SlowDown { interval_secs } => {
                assert_eq!(interval_secs, 10);
                "slow"
            }
            DevicePoll::Denied => "denied",
            DevicePoll::Expired => "expired",
            DevicePoll::Ready(_) => "ready",
        };
        assert_eq!(named, expected, "{error}");
    }

    let granted = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"access_token": "at", "refresh_token": "rt", "expires_in": 7200,
               "id_token": id_token}),
    )]);
    let DevicePoll::Ready(acquired) = GrokBuild
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
        acquired.provider_fields["sub"], "user-9",
        "the subject prepare needs is a login fact, not a per-request decode"
    );
    assert_eq!(acquired.provider_fields["user_email"], "nine@example.com");
    let (_, url, _, body) = granted.sent().into_iter().next().unwrap();
    assert_eq!(url, "https://auth.x.ai/oauth2/token");
    assert!(
        String::from_utf8(body)
            .unwrap()
            .contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code")
    );
}

#[tokio::test]
async fn the_refresh_rotates_and_a_refusal_of_the_token_is_definitive() {
    let config = json!({});
    let secret = secret();
    let metadata = metadata();

    let rotated = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"access_token": "at2", "refresh_token": "rt2", "expires_in": 3600}),
    )]);
    let update = CredentialRefresh::refresh(
        &GrokBuild,
        CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            client: rotated.as_ref(),
        },
    )
    .await
    .expect("a rotation");
    let (_, url, _, body) = rotated.sent().into_iter().next().unwrap();
    assert_eq!(url, "https://auth.x.ai/oauth2/token");
    let form = String::from_utf8(body).unwrap();
    assert!(form.contains("grant_type=refresh_token"), "{form}");
    assert!(form.contains("refresh_token=grok-refresh"), "{form}");
    assert_eq!(update.secret["access_token"], "at2");
    assert_eq!(update.secret["refresh_token"], "rt2");
    assert_eq!(
        update.expires_at_ms,
        update.secret["expires_at_ms"].as_i64()
    );

    let rejected = ScriptClient::new(vec![reply(
        StatusCode::BAD_REQUEST,
        json!({"error": "invalid_grant"}),
    )]);
    assert!(matches!(
        CredentialRefresh::refresh(
            &GrokBuild,
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
                &GrokBuild,
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
            &GrokBuild,
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

/// No `QuotaModel`: the credit window and per-product readings are observed,
/// never charged.
const OBSERVE_ONLY: &[&str] = &["usage", "weekly_limit", "product:*"];

#[tokio::test]
async fn the_billing_probe_reads_the_chat_proxys_credit_window() {
    let client = ScriptClient::new(vec![
        reply(
            StatusCode::OK,
            json!({"config": {
                "currentPeriod": {"type": "USAGE_PERIOD_TYPE_WEEKLY",
                                  "start": "2026-07-08T18:30:33+00:00",
                                  "end": "2026-07-15T18:30:33+00:00"},
                "creditUsagePercent": 2.0,
                "productUsage": [{"product": "Api", "usagePercent": 2.0}],
            }}),
        ),
        reply(StatusCode::SERVICE_UNAVAILABLE, json!({})),
    ]);
    let config = json!({});
    let secret = secret();
    let metadata = metadata();
    let snapshot = GrokBuild
        .query(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            client: client.as_ref(),
        })
        .await
        .expect("a snapshot");
    let (method, url, headers, _) = client.sent().into_iter().next().unwrap();
    assert_eq!(method, Method::GET);
    assert_eq!(
        url,
        "https://cli-chat-proxy.grok.com/v1/billing?format=credits"
    );
    assert_eq!(headers["authorization"], "Bearer grok-access");
    assert_eq!(
        headers["x-userid"], "user-1",
        "the billing surface spells the account id its own way"
    );
    assert_eq!(headers["x-grok-user-id"], "user-1");
    assert_eq!(
        snapshot
            .entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        ["weekly_limit", "product:api"]
    );
    let QuotaValue::Window(window) = &snapshot.entries[0].value else {
        panic!("a window");
    };
    assert_eq!(window.used_percent, Some(2.into()));
    assert_eq!(window.period_start_ms, Some(1_783_535_433_000));
    support::assert_quota_contract(None, &[], &snapshot.entries, OBSERVE_ONLY);
}

#[tokio::test]
async fn a_billing_reply_with_nothing_in_it_is_an_error() {
    let client = ScriptClient::new(vec![reply(StatusCode::OK, json!({}))]);
    let config = json!({});
    let secret = secret();
    let metadata = metadata();
    assert!(matches!(
        GrokBuild
            .query(CredentialContext {
                provider: provider(&config, None),
                credential: credential(&secret, &metadata),
                client: client.as_ref(),
            })
            .await,
        Err(ChannelError::InvalidResponse(_))
    ));

    let refused = ScriptClient::new(vec![reply(
        StatusCode::UNAUTHORIZED,
        json!({"error": "no"}),
    )]);
    assert!(matches!(
        GrokBuild
            .query(CredentialContext {
                provider: provider(&config, None),
                credential: credential(&secret, &metadata),
                client: refused.as_ref(),
            })
            .await,
        Err(ChannelError::UpstreamResponse { .. })
    ));
}

#[tokio::test]
async fn a_bare_billing_payload_is_read_the_same_way() {
    let client = ScriptClient::new(vec![
        reply(
            StatusCode::OK,
            json!({"monthlyLimit": {"val": 2000}, "used": {"val": "500"},
               "billingPeriodEnd": "2026-09-01T00:00:00+00:00"}),
        ),
        reply(StatusCode::SERVICE_UNAVAILABLE, json!({})),
    ]);
    let config = json!({"usage_base_url": "https://billing.invalid/v1"});
    let secret = secret();
    let metadata = metadata();
    let snapshot = GrokBuild
        .query(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret, &metadata),
            client: client.as_ref(),
        })
        .await
        .expect("a snapshot");
    assert_eq!(
        client.sent()[0].1,
        "https://billing.invalid/v1/billing?format=credits"
    );
    assert_eq!(snapshot.entries[0].id, USAGE_DIMENSION);
    let QuotaValue::Window(window) = &snapshot.entries[0].value else {
        panic!("a window");
    };
    assert_eq!(window.used_percent, Some(25.into()));
    support::assert_quota_contract(None, &[], &snapshot.entries, OBSERVE_ONLY);
}

// --------------------------------------------------------------- descriptor

#[test]
fn the_descriptor_names_the_one_way_in_and_the_keys_a_form_needs() {
    let descriptor = GrokBuild.descriptor();
    assert_eq!(descriptor.id, "grokbuild");
    assert_eq!(descriptor.login_modes, [LoginMode::DeviceCode]);
    assert!(descriptor.capabilities.refresh);
    assert!(descriptor.capabilities.quota_query);
    assert!(!descriptor.capabilities.services);
    for key in [
        "media_base_url",
        "usage_base_url",
        "oauth_token_url",
        "allowed_headers",
    ] {
        assert!(descriptor.config_key(key).is_some(), "{key}");
    }
}

#[test]
fn there_is_no_cli_fingerprint_to_reproduce() {
    assert!(GrokBuild.default_connection().is_none());
}
