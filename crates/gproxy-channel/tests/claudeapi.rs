#![cfg(feature = "claudeapi")]

mod support;

use gproxy_channel::{
    BaseChannel, ChannelError, OutboundClient,
    channel::{
        CredentialContext, CredentialView, PrepareContext, ProviderView, QuotaHeaderContext,
        QuotaValue,
    },
    channels::claudeapi::Claudeapi,
};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse,
    capability::{CapabilityError, CapabilityFuture, UpstreamConnection},
    connection::Bytes,
};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use std::{collections::VecDeque, sync::Mutex};

// ------------------------------------------------------------------ harness

type Sent = (Method, String, HeaderMap);

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
            let (parts, _) = request.into_parts();
            self.requests.lock().unwrap().push((
                parts.method,
                parts.uri.to_string(),
                parts.headers,
            ));
            Ok(self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected upstream call"))
        })
    }

    /// This channel opens no socket; the handshake is recorded as refused.
    fn connect<'a>(
        &'a self,
        _request: http::Request<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        Box::pin(async {
            Ok(UpstreamConnection::Rejected(WireResponse {
                status: StatusCode::BAD_REQUEST,
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

fn provider<'a>(config: &'a Value, base_url: Option<&'a str>) -> ProviderView<'a> {
    ProviderView {
        id: "p",
        channel: "claudeapi",
        base_url,
        config,
    }
}

fn credential(secret: &Value) -> CredentialView<'_> {
    CredentialView {
        id: "c",
        provider_id: "p",
        auth_kind: "api_key",
        secret,
        metadata: &Value::Null,
        version: 0,
        expires_at_ms: None,
    }
}

/// A client request with source authentication, hop-by-hop noise, a vendor
/// header the channel must keep and a repeated header.
fn request(path: &str, query: Option<&str>, body: &[u8]) -> WireRequest<HttpBody> {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", HeaderValue::from_static("Bearer client"));
    headers.insert("x-api-key", HeaderValue::from_static("client-key"));
    headers.insert("host", HeaderValue::from_static("gproxy.local"));
    headers.insert("content-length", HeaderValue::from_static("2"));
    headers.insert(
        "anthropic-user-profile-id",
        HeaderValue::from_static("profile-1"),
    );
    headers.insert("anthropic-version", HeaderValue::from_static("1999-01-01"));
    headers.append("x-multi", HeaderValue::from_static("1"));
    headers.append("x-multi", HeaderValue::from_static("2"));
    WireRequest {
        method: Method::POST,
        path: path.into(),
        query: query.map(str::to_owned),
        headers,
        body: HttpBody::Bytes(Bytes::copy_from_slice(body)),
    }
}

fn prepare<'a>(
    config: &'a Value,
    base_url: Option<&'a str>,
    secret: &'a Value,
    operation: Operation,
    dialect: Dialect,
    request: WireRequest<HttpBody>,
    endpoint_override: Option<&'a str>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    Claudeapi.prepare(PrepareContext {
        provider: provider(config, base_url),
        credential: credential(secret),
        operation: OperationKey { operation, dialect },
        request,
        endpoint_override,
    })
}

fn shaped(request: http::Request<HttpBody>) -> Value {
    let HttpBody::Bytes(bytes) = request.into_body() else {
        panic!("buffered");
    };
    serde_json::from_slice(&bytes).unwrap()
}

// ------------------------------------------------------------------- routes

#[test]
fn routes_every_native_dialect_and_injects_the_api_key() {
    let config = json!({});
    let secret = json!({"api_key": "  sk-ant-upstream  "});
    let messages = prepare(
        &config,
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::Claude,
        request(
            "/anything/the/client/sent",
            Some("beta=true&key=leak&api_key=leak"),
            br#"{"model":"claude-opus-4-8","messages":[]}"#,
        ),
        None,
    )
    .unwrap();
    assert_eq!(
        messages.uri(),
        "https://api.anthropic.com/v1/messages?beta=true",
        "the route comes from the operation, not from the client path"
    );
    let headers = messages.headers();
    assert_eq!(headers["x-api-key"], "sk-ant-upstream", "trimmed");
    assert_eq!(headers["anthropic-version"], "2023-06-01");
    assert!(
        headers.get("authorization").is_none(),
        "source authentication never reaches the upstream"
    );
    assert!(headers.get("host").is_none());
    assert!(headers.get("content-length").is_none());
    assert_eq!(
        headers["anthropic-user-profile-id"], "profile-1",
        "a vendor header the API reads is forwarded"
    );
    assert!(headers.get("x-multi").is_none());

    let route = |operation, dialect, path| {
        prepare(
            &config,
            Some("https://relay.example/anthropic/"),
            &secret,
            operation,
            dialect,
            request(path, None, b"{}"),
            None,
        )
        .unwrap()
        .uri()
        .to_string()
    };
    assert_eq!(
        route(Operation::ListModels, Dialect::Claude, "/v1/models"),
        "https://relay.example/anthropic/v1/models"
    );
    assert_eq!(
        route(
            Operation::GetModel,
            Dialect::Claude,
            "/v1/models/claude-opus-4-8"
        ),
        "https://relay.example/anthropic/v1/models/claude-opus-4-8",
        "GetModel keeps the client's model segment: it lives nowhere else"
    );
    assert_eq!(
        route(Operation::CountTokens, Dialect::Claude, "/x"),
        "https://relay.example/anthropic/v1/messages/count_tokens"
    );
    assert_eq!(
        route(
            Operation::StreamGenerateContent,
            Dialect::OpenAiChat,
            "/v1/chat/completions"
        ),
        "https://relay.example/anthropic/v1/chat/completions",
        "the OpenAI SDK compatibility layer"
    );

    let overridden = prepare(
        &config,
        Some("https://unused.example"),
        &secret,
        Operation::GenerateContent,
        Dialect::Claude,
        request("/v1/messages", None, b"{}"),
        Some("https://exact.example/native?fixed=1"),
    )
    .unwrap();
    assert_eq!(overridden.uri(), "https://exact.example/native?fixed=1");

    let view = provider(&config, None);
    assert_eq!(
        Claudeapi.native_dialects(view, Operation::GenerateContent),
        [Dialect::Claude, Dialect::OpenAiChat]
    );
    assert_eq!(
        Claudeapi.native_dialects(view, Operation::CountTokens),
        [Dialect::Claude]
    );
    assert!(
        Claudeapi
            .native_dialects(view, Operation::CreateEmbedding)
            .is_empty(),
        "Anthropic serves no embeddings"
    );
    assert_eq!(Claudeapi.descriptor().id, "claudeapi");
    assert!(Claudeapi.descriptor().config_key("fallback_mode").is_some());
    assert!(
        Claudeapi.default_connection().is_none(),
        "a plain HTTPS API has no client identity to impersonate"
    );

    assert!(matches!(
        prepare(
            &config,
            None,
            &json!({"api_key": "  "}),
            Operation::GenerateContent,
            Dialect::Claude,
            request("/v1/messages", None, b"{}"),
            None
        ),
        Err(ChannelError::InvalidCredential)
    ));
    assert!(matches!(
        prepare(
            &config,
            None,
            &secret,
            Operation::CreateImage,
            Dialect::Claude,
            request("/v1/images", None, b"{}"),
            None
        ),
        Err(ChannelError::UnsupportedOperation(_))
    ));
}

// ------------------------------------------------------------------ hygiene

#[test]
fn shapes_messages_bodies_and_derives_the_body_triggered_betas() {
    let secret = json!({"api_key": "k"});
    let prepared = prepare(
        &json!({}),
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::Claude,
        request(
            "/v1/messages",
            None,
            json!({
                "model": "claude-fable-5",
                "speed": "fast",
                "thinking": {"type": "adaptive", "display": "updates"},
                "temperature": 0.7,
                "top_p": 0.9,
                "top_k": 40,
                "system": [{"type": "text", "text": " policy "}],
                "messages": [{"role": "assistant", "content": "prefix"}],
                "future_request_field": {"kept": true}
            })
            .to_string()
            .as_bytes(),
        ),
        None,
    )
    .unwrap();
    assert_eq!(
        prepared.headers()["anthropic-beta"],
        "fast-mode-2026-02-01,thinking-display-updates-2026-08-18",
        "both betas come from the body, and the client's anthropic-version does not survive"
    );
    let body = shaped(prepared);
    assert_eq!(body["system"][0]["text"], "policy", "trimmed");
    assert_eq!(
        body["messages"][0]["role"], "user",
        "a trailing assistant text turn is a prefill this model refuses"
    );
    assert!(body.get("temperature").is_none());
    assert!(body.get("top_p").is_none());
    assert!(body.get("top_k").is_none());
    assert_eq!(body["future_request_field"]["kept"], true);

    let tolerant = shaped(
        prepare(
            &json!({}),
            None,
            &secret,
            Operation::GenerateContent,
            Dialect::Claude,
            request(
                "/v1/messages",
                None,
                br#"{"model":"claude-opus-4-5","temperature":0.5,"top_p":0.9,"top_k":8,"messages":[{"role":"assistant","content":"keep"}]}"#,
            ),
            None,
        )
        .unwrap(),
    );
    assert_eq!(tolerant["temperature"], 0.5);
    assert_eq!(tolerant["top_k"], 8);
    assert!(
        tolerant.get("top_p").is_none(),
        "temperature and top_p together are still refused"
    );
    assert_eq!(
        tolerant["messages"][0]["role"], "assistant",
        "this model still takes a prefill"
    );

    let mut with_beta = request("/v1/messages", None, br#"{"model":"claude-fable-5"}"#);
    with_beta.headers.insert(
        "anthropic-beta",
        HeaderValue::from_static("files-api-2025-04-14,context-1m-2025-08-07"),
    );
    let prepared = prepare(
        &json!({}),
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::Claude,
        with_beta,
        None,
    )
    .unwrap();
    assert_eq!(
        prepared.headers()["anthropic-beta"],
        "files-api-2025-04-14",
        "the 1M-context beta is not a first-party API feature"
    );
}

#[test]
fn installs_the_configured_server_side_fallback_only_where_it_means_something() {
    let secret = json!({"api_key": "k"});
    let body = br#"{"model":"claude-fable-5","messages":[]}"#;
    let prepare_with = |config: &Value, body: &[u8]| {
        prepare(
            config,
            None,
            &secret,
            Operation::GenerateContent,
            Dialect::Claude,
            request("/v1/messages", None, body),
            None,
        )
        .unwrap()
    };

    let defaulted = prepare_with(&json!({"fallback_mode": "default"}), body);
    assert_eq!(
        defaulted.headers()["anthropic-beta"],
        "server-side-fallback-2026-07-01"
    );
    assert_eq!(shaped(defaulted)["fallbacks"], "default");

    let listed = prepare_with(
        &json!({
            "fallback_mode": "models",
            "fallback_models": ["claude-opus-4-8", "claude-opus-4-8", "claude-fable-5", "gpt-5"]
        }),
        body,
    );
    assert_eq!(
        listed.headers()["anthropic-beta"],
        "server-side-fallback-2026-06-01"
    );
    assert_eq!(
        shaped(listed)["fallbacks"],
        json!([{"model": "claude-opus-4-8"}, {"model": "gpt-5"}]),
        "duplicates and the model itself drop out; a foreign id passes through"
    );

    let off = prepare_with(&json!({}), body);
    assert!(shaped(off).get("fallbacks").is_none(), "off by default");

    let unsupported = prepare_with(
        &json!({"fallback_mode": "default"}),
        br#"{"model":"claude-opus-4-8","messages":[]}"#,
    );
    assert!(
        shaped(unsupported).get("fallbacks").is_none(),
        "a model that already falls back server-side is left alone"
    );

    let credited = prepare_with(
        &json!({"fallback_mode": "default"}),
        br#"{"model":"claude-fable-5","fallback_credit_token":"tok","messages":[{"role":"assistant","content":"x"}]}"#,
    );
    assert_eq!(
        shaped(credited),
        json!({
            "model": "claude-fable-5",
            "fallback_credit_token": "tok",
            "messages": [{"role": "assistant", "content": "x"}]
        }),
        "a fallback credit replays the body Anthropic already priced"
    );
}

#[test]
fn the_compatibility_layer_keeps_the_stream_options_it_came_with() {
    let secret = json!({"api_key": "k"});
    let streamed = shaped(
        prepare(
            &json!({}),
            None,
            &secret,
            Operation::StreamGenerateContent,
            Dialect::OpenAiChat,
            request(
                "/v1/chat/completions",
                None,
                br#"{"model":"claude-fable-5","stream":true,"stream_options":{"other":1},"messages":[{"role":"assistant","content":"prefix"}]}"#,
            ),
            None,
        )
        .unwrap(),
    );
    assert_eq!(
        streamed["stream_options"],
        json!({"other": 1}),
        "core asks a Chat stream for its usage; the channel forwards the body"
    );
    assert_eq!(
        streamed["messages"][0]["role"], "user",
        "the prefill rule is the model's, not the surface's"
    );

    let buffered = shaped(
        prepare(
            &json!({}),
            None,
            &secret,
            Operation::GenerateContent,
            Dialect::OpenAiChat,
            request(
                "/v1/chat/completions",
                None,
                br#"{"model":"claude-fable-5","messages":[]}"#,
            ),
            None,
        )
        .unwrap(),
    );
    assert!(
        buffered.get("stream_options").is_none(),
        "nothing is added to a buffered body"
    );
}

// -------------------------------------------------------------------- quota

/// No `QuotaModel`: per-model rate limits and the organization cost report
/// are observed, never charged.
const OBSERVE_ONLY: &[&str] = &["rate:*", "usage:*"];

#[test]
fn observes_the_rate_limit_headers_of_a_reply() {
    let quota = Claudeapi.quota_headers().expect("declared");
    let mut headers = HeaderMap::new();
    headers.insert("anthropic-ratelimit-requests-limit", "50".parse().unwrap());
    headers.insert(
        "anthropic-ratelimit-requests-remaining",
        "49".parse().unwrap(),
    );
    headers.insert(
        "anthropic-ratelimit-requests-reset",
        "2026-09-21T00:00:10Z".parse().unwrap(),
    );
    headers.insert(
        "anthropic-ratelimit-input-tokens-remaining",
        "9000".parse().unwrap(),
    );
    // Reported by neither field: not a dimension at all.
    headers.insert(
        "anthropic-ratelimit-output-tokens-reset",
        "2026-09-21T00:00:10Z".parse().unwrap(),
    );
    let entries = quota
        .observe(QuotaHeaderContext {
            operation: OperationKey {
                operation: Operation::GenerateContent,
                dialect: Dialect::Claude,
            },
            upstream_model: "claude-fable-5",
            status: StatusCode::OK,
            headers: &headers,
        })
        .unwrap();
    assert_eq!(
        entries.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        [
            "rate:requests:claude-fable-5",
            "rate:input-tokens:claude-fable-5"
        ]
    );
    support::assert_quota_contract(None, &[], &entries, OBSERVE_ONLY);
    let QuotaValue::RateLimit(requests) = &entries[0].value else {
        panic!("a rate limit");
    };
    assert_eq!(requests.limit, Some(Decimal::from(50)));
    assert_eq!(requests.remaining, Some(Decimal::from(49)));
    assert_eq!(
        requests.period_end_ms,
        Some(1_789_948_810_000),
        "the reset is an instant, not a duration"
    );
    assert_eq!(requests.unit.as_deref(), Some("requests"));
    assert_eq!(entries[1].label.as_deref(), Some("input tokens"));

    assert!(
        quota
            .observe(QuotaHeaderContext {
                operation: OperationKey {
                    operation: Operation::GenerateContent,
                    dialect: Dialect::Claude,
                },
                upstream_model: "",
                status: StatusCode::OK,
                headers: &HeaderMap::new(),
            })
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn reads_the_organization_cost_report_with_the_admin_key() {
    let query = Claudeapi.quota_query().expect("declared");
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({
            "has_more": false,
            "data": [{
                "starting_at": "2026-09-14T00:00:00Z",
                "ending_at": "2026-09-15T00:00:00Z",
                "results": [
                    {"currency": "USD", "amount": "125"},
                    {"currency": "USD", "amount": 75}
                ]
            }]
        }),
    )]);
    let config = json!({"quota_base_url": "https://admin.example/v1"});
    let secret = json!({"api_key": "sk-inference", "quota_api_key": "sk-ant-admin"});
    let snapshot = query
        .query(CredentialContext {
            provider: provider(&config, Some("https://unused.example")),
            credential: credential(&secret),
            client: &client,
        })
        .await
        .unwrap();
    let (method, url, headers) = client.sent().into_iter().next().unwrap();
    assert_eq!(method, Method::GET);
    assert!(
        url.starts_with("https://admin.example/v1/organizations/cost_report?starting_at="),
        "the configured origin keeps its version segment exactly once: {url}"
    );
    assert!(url.contains("&bucket_width=1d&limit=7"), "{url}");
    assert_eq!(
        headers["x-api-key"], "sk-ant-admin",
        "the Admin key, not the inference key"
    );
    assert_eq!(headers["anthropic-version"], "2023-06-01");

    assert_eq!(snapshot.entries.len(), 1);
    support::assert_quota_contract(None, &[], &snapshot.entries, OBSERVE_ONLY);
    let entry = &snapshot.entries[0];
    assert_eq!(entry.id, "usage:1789344000");
    assert_eq!(entry.source_id, "organization_usage");
    let QuotaValue::Budget(spend) = &entry.value else {
        panic!("a budget");
    };
    assert_eq!(spend.used, Some(Decimal::new(200, 2)), "minor units");
    assert_eq!(spend.unit.as_deref(), Some("USD"));
    assert_eq!(spend.period_start_ms, Some(1_789_344_000_000));

    // A truncated report is refused rather than under-reported.
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"has_more": true, "data": []}),
    )]);
    assert!(matches!(
        query
            .query(CredentialContext {
                provider: provider(&config, None),
                credential: credential(&secret),
                client: &client,
            })
            .await,
        Err(ChannelError::InvalidResponse(_))
    ));

    // Without any key at all there is nothing to ask with.
    let client = ScriptClient::new(Vec::new());
    assert!(matches!(
        query
            .query(CredentialContext {
                provider: provider(&json!({}), None),
                credential: credential(&json!({})),
                client: &client,
            })
            .await,
        Err(ChannelError::InvalidCredential)
    ));
}

#[test]
fn files_preserve_multipart_and_use_native_item_routes() {
    let config = json!({"headers": {"anthropic-workspace-id": "wrkspc_owner"}});
    let secret = json!({"api_key": "sk-ant-upstream"});
    let multipart = b"--boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"data.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n\x00\xff\r\n--boundary--\r\n";
    for (operation, method, path, expected) in [
        (
            Operation::CreateFile,
            Method::POST,
            "/client/upload",
            "/v1/files",
        ),
        (
            Operation::ListFiles,
            Method::GET,
            "/client/list",
            "/v1/files",
        ),
        (
            Operation::RetrieveFile,
            Method::GET,
            "/client/files/file_123",
            "/v1/files/file_123",
        ),
        (
            Operation::DeleteFile,
            Method::DELETE,
            "/client/files/file_123",
            "/v1/files/file_123",
        ),
        (
            Operation::RetrieveFileContent,
            Method::GET,
            "/client/files/file_123/content",
            "/v1/files/file_123/content",
        ),
    ] {
        assert_eq!(
            Claudeapi.native_dialects(provider(&config, None), operation),
            [Dialect::Claude]
        );
        let mut input = request(
            path,
            Some("limit=2&after_id=file_1&key=client-secret"),
            multipart,
        );
        input.method = method.clone();
        input.headers.insert(
            "content-type",
            HeaderValue::from_static("multipart/form-data; boundary=boundary"),
        );
        input.headers.insert(
            "anthropic-workspace-id",
            HeaderValue::from_static("wrkspc_client"),
        );
        let output = prepare(
            &config,
            None,
            &secret,
            operation,
            Dialect::Claude,
            input,
            None,
        )
        .unwrap();
        assert_eq!(output.method(), method);
        assert_eq!(
            output.uri().to_string(),
            format!("https://api.anthropic.com{expected}?limit=2&after_id=file_1")
        );
        assert_eq!(
            output.headers()["content-type"],
            "multipart/form-data; boundary=boundary"
        );
        assert_eq!(output.headers()["anthropic-workspace-id"], "wrkspc_owner");
        assert_eq!(output.headers()["x-api-key"], "sk-ant-upstream");
        assert!(output.headers().get("authorization").is_none());
        assert!(
            output.headers().get("anthropic-beta").is_none(),
            "stable Files needs no forced beta"
        );
        let HttpBody::Bytes(bytes) = output.into_body() else {
            panic!("buffered upload")
        };
        assert_eq!(bytes.as_ref(), multipart);
    }
}

#[tokio::test]
async fn file_download_preserves_binary_response_and_headers() {
    use gproxy_channel::channel::ChannelBinding;
    use std::sync::Arc;

    let config = json!({});
    let secret = json!({"api_key": "sk-ant-upstream"});
    let bytes = Bytes::from_static(b"\x00\xffbinary\r\n");
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        HeaderValue::from_static("application/octet-stream"),
    );
    headers.insert(
        "content-disposition",
        HeaderValue::from_static("attachment; filename=data.bin"),
    );
    let client = Arc::new(ScriptClient::new(vec![WireResponse {
        status: StatusCode::OK,
        headers: headers.clone(),
        body: HttpBody::Bytes(bytes.clone()),
    }]));
    let binding = ChannelBinding::new(
        &Claudeapi,
        provider(&config, None),
        credential(&secret),
        client.clone(),
    );
    let mut input = request("/v1/files/file_123/content", None, b"");
    input.method = Method::GET;
    let response = binding
        .send(
            OperationKey {
                operation: Operation::RetrieveFileContent,
                dialect: Dialect::Claude,
            },
            input,
        )
        .await
        .unwrap();
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.headers, headers);
    let HttpBody::Bytes(body) = response.body else {
        panic!("binary download")
    };
    assert_eq!(body, bytes);
    assert_eq!(
        client.sent()[0].1,
        "https://api.anthropic.com/v1/files/file_123/content"
    );
}
