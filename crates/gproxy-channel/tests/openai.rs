#![cfg(feature = "openai")]

mod support;

use gproxy_channel::{
    BaseChannel, ChannelError, OutboundClient,
    channel::{
        CredentialContext, CredentialView, PrepareContext, ProviderView, QuotaHeaderContext,
        QuotaValue,
    },
    channels::openai::{OpenAi, RESPONSES_MULTI_AGENT_BETA, RESPONSES_WS_BETA},
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
        channel: "openai",
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

fn request(path: &str, query: Option<&str>, body: &[u8]) -> WireRequest<HttpBody> {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", HeaderValue::from_static("Bearer client"));
    headers.insert("host", HeaderValue::from_static("gproxy.local"));
    headers.insert("content-length", HeaderValue::from_static("2"));
    headers.insert("openai-beta", HeaderValue::from_static("feature=v1"));
    headers.insert("openai-project", HeaderValue::from_static("proj_1"));
    headers.insert("x-vendor", HeaderValue::from_static("kept"));
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
    OpenAi.prepare(PrepareContext {
        provider: provider(config, base_url),
        credential: credential(secret),
        operation: OperationKey { operation, dialect },
        request,
        endpoint_override,
    })
}

fn connect(
    secret: &Value,
    operation: Operation,
    dialect: Dialect,
    path: &str,
    query: Option<&str>,
) -> Result<http::Request<()>, ChannelError> {
    let config = json!({});
    OpenAi.prepare_connect(PrepareContext {
        provider: provider(&config, None),
        credential: credential(secret),
        operation: OperationKey { operation, dialect },
        request: WireRequest {
            method: Method::GET,
            path: path.into(),
            query: query.map(str::to_owned),
            headers: HeaderMap::new(),
            body: (),
        },
        endpoint_override: None,
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
fn forwards_the_native_path_and_replaces_the_credential() {
    let config = json!({"headers": {"x-static": "yes"}});
    let secret = json!({"api_key": "  sk-upstream  "});
    let prepared = prepare(
        &config,
        None,
        &secret,
        Operation::StreamGenerateContent,
        Dialect::OpenAi,
        request(
            "/v1/responses",
            Some("purpose=assistants&key=leak&access_token=leak"),
            br#"{"model":"gpt-5","stream":true}"#,
        ),
        None,
    )
    .unwrap();
    assert_eq!(
        prepared.uri(),
        "https://api.openai.com/v1/responses?purpose=assistants",
        "the path is OpenAI's own, so the client's is forwarded"
    );
    let headers = prepared.headers();
    assert_eq!(headers["authorization"], "Bearer sk-upstream", "trimmed");
    assert_eq!(headers["openai-beta"], "feature=v1");
    assert_eq!(headers["openai-project"], "proj_1");
    assert_eq!(headers["x-vendor"], "kept");
    assert_eq!(headers["x-static"], "yes");
    assert!(headers.get("host").is_none());
    assert!(headers.get("content-length").is_none());
    assert!(
        shaped(prepared).get("stream_options").is_none(),
        "a Responses stream reports usage without being asked"
    );

    let narrowed = prepare(
        &json!({"allowed_headers": ["x-vendor"]}),
        Some("https://relay.example/openai/"),
        &secret,
        Operation::CreateEmbedding,
        Dialect::OpenAi,
        request("/v1/embeddings", None, b"{}"),
        None,
    )
    .unwrap();
    assert_eq!(narrowed.uri(), "https://relay.example/openai/v1/embeddings");
    assert_eq!(
        narrowed.headers()["openai-beta"],
        "feature=v1",
        "a provider allow-list never strips the vendor's own headers"
    );
    assert_eq!(narrowed.headers()["x-vendor"], "kept");

    let overridden = prepare(
        &json!({}),
        Some("https://unused.example"),
        &secret,
        Operation::RetrieveFileContent,
        Dialect::OpenAi,
        request("/v1/files/file-1/content", None, b""),
        Some("https://media.example/f/1"),
    )
    .unwrap();
    assert_eq!(overridden.uri(), "https://media.example/f/1");

    let view = provider(&config, None);
    assert_eq!(
        OpenAi.native_dialects(view, Operation::GenerateContent),
        [
            Dialect::OpenAi,
            Dialect::OpenAiChat,
            Dialect::OpenAiResponsesWebSocket
        ]
    );
    assert_eq!(
        OpenAi.native_dialects(view, Operation::CreateSpeech),
        [Dialect::OpenAi]
    );
    assert!(
        OpenAi
            .native_dialects(view, Operation::CountTokens)
            .is_empty(),
        "OpenAI has no token-counting endpoint"
    );
    assert!(OpenAi.descriptor().capabilities.websocket);
    assert!(
        OpenAi.default_connection().is_none(),
        "a plain HTTPS API has no client identity to impersonate"
    );

    assert!(matches!(
        prepare(
            &json!({}),
            None,
            &json!({"api_key": ""}),
            Operation::GenerateContent,
            Dialect::OpenAi,
            request("/v1/responses", None, b"{}"),
            None
        ),
        Err(ChannelError::InvalidCredential)
    ));
}

#[test]
fn a_chat_stream_keeps_the_stream_options_it_came_with() {
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
                br#"{"model":"gpt-5","stream":true,"stream_options":{"other":1}}"#,
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

    let buffered = shaped(
        prepare(
            &json!({}),
            None,
            &secret,
            Operation::GenerateContent,
            Dialect::OpenAiChat,
            request("/v1/chat/completions", None, br#"{"model":"gpt-5"}"#),
            None,
        )
        .unwrap(),
    );
    assert!(buffered.get("stream_options").is_none());
}

#[test]
fn opens_the_two_socket_surfaces_and_refuses_the_rest() {
    let secret = json!({"api_key": "k"});
    let responses = connect(
        &secret,
        Operation::StreamGenerateContent,
        Dialect::OpenAiResponsesWebSocket,
        "/v1/responses",
        None,
    )
    .unwrap();
    assert_eq!(responses.uri(), "wss://api.openai.com/v1/responses");
    assert_eq!(responses.headers()["authorization"], "Bearer k");
    let betas = responses
        .headers()
        .get_all("openai-beta")
        .iter()
        .map(|value| value.to_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(betas, [RESPONSES_WS_BETA, RESPONSES_MULTI_AGENT_BETA]);

    let realtime = connect(
        &secret,
        Operation::ConnectRealtime,
        Dialect::OpenAi,
        "/v1/realtime",
        Some("intent=transcription&model=client&key=leak"),
    )
    .unwrap();
    assert_eq!(
        realtime.uri(),
        "wss://api.openai.com/v1/realtime?intent=transcription&model=client",
        "the credential leaves the query; the selector is written last"
    );
    assert!(
        realtime.headers().get("openai-beta").is_none(),
        "the Responses betas belong to the Responses socket only"
    );

    let joined = connect(
        &secret,
        Operation::ConnectRealtime,
        Dialect::OpenAi,
        "/v1/realtime",
        Some("model=ignored&call_id=call_1"),
    )
    .unwrap();
    assert_eq!(
        joined.uri(),
        "wss://api.openai.com/v1/realtime?call_id=call_1",
        "a live call already has a model"
    );

    assert!(matches!(
        connect(
            &secret,
            Operation::ConnectRealtime,
            Dialect::OpenAi,
            "/v1/realtime",
            None
        ),
        Err(ChannelError::InvalidConfig(_))
    ));
    assert!(matches!(
        connect(
            &secret,
            Operation::GenerateContent,
            Dialect::OpenAi,
            "/v1/responses",
            None
        ),
        Err(ChannelError::WrongTransport(_))
    ));
}

// -------------------------------------------------------------------- quota

/// No `QuotaModel`: per-model rate limits and the organization cost report
/// are observed, never charged.
const OBSERVE_ONLY: &[&str] = &["rate:*", "usage:*"];

#[test]
fn observes_the_rate_limit_headers_of_a_reply() {
    let quota = OpenAi.quota_headers().expect("declared");
    let mut headers = HeaderMap::new();
    headers.insert("x-ratelimit-limit-requests", "500".parse().unwrap());
    headers.insert("x-ratelimit-remaining-requests", "499".parse().unwrap());
    headers.insert("x-ratelimit-reset-requests", "2m59.56s".parse().unwrap());
    headers.insert("x-ratelimit-remaining-tokens", "1000".parse().unwrap());
    let entries = quota
        .observe(QuotaHeaderContext {
            operation: OperationKey {
                operation: Operation::GenerateContent,
                dialect: Dialect::OpenAi,
            },
            upstream_model: "gpt-5",
            status: StatusCode::OK,
            headers: &headers,
        })
        .unwrap();
    assert_eq!(
        entries.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["rate:requests:gpt-5", "rate:tokens:gpt-5"]
    );
    support::assert_quota_contract(None, &[], &entries, OBSERVE_ONLY);
    let QuotaValue::RateLimit(requests) = &entries[0].value else {
        panic!("a rate limit");
    };
    assert_eq!(requests.limit, Some(Decimal::from(500)));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    let end = requests.period_end_ms.expect("a reset") / 1000;
    assert!(
        (179..=181).contains(&(end - now)),
        "a duration is read forward from now: {} seconds away",
        end - now
    );
    let QuotaValue::RateLimit(tokens) = &entries[1].value else {
        panic!("a rate limit");
    };
    assert_eq!(tokens.limit, None, "unreported is not zero");
    assert_eq!(tokens.remaining, Some(Decimal::from(1000)));
    assert_eq!(tokens.period_end_ms, None);
}

#[tokio::test]
async fn reads_the_organization_cost_report_across_its_pages() {
    let query = OpenAi.quota_query().expect("declared");
    let client = ScriptClient::new(vec![
        reply(
            StatusCode::OK,
            json!({
                "has_more": true,
                "next_page": "page one/two",
                "data": [{
                    "start_time": 1_789_344_000_i64,
                    "end_time": 1_789_430_400_i64,
                    "results": [
                        {"amount": {"value": 1.5, "currency": "usd"}},
                        {"amount": {"value": "0.25", "currency": "USD"}},
                        {"amount": {"value": 3, "currency": "EUR"}}
                    ]
                }]
            }),
        ),
        reply(StatusCode::OK, json!({"has_more": false, "data": []})),
    ]);
    let config = json!({"quota_base_url": "https://admin.example"});
    let secret = json!({"api_key": "sk-inference", "quota_api_key": "sk-admin"});
    let snapshot = query
        .query(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret),
            client: &client,
        })
        .await
        .unwrap();

    let sent = client.sent();
    assert_eq!(sent.len(), 2, "the cursor is followed");
    assert!(
        sent[0]
            .1
            .starts_with("https://admin.example/v1/organization/costs?start_time="),
        "{}",
        sent[0].1
    );
    assert!(sent[0].1.contains("&bucket_width=1d&limit=7"));
    assert_eq!(sent[0].2["authorization"], "Bearer sk-admin");
    assert!(
        sent[1].1.ends_with("&page=page%20one%2Ftwo"),
        "the cursor is opaque and encoded: {}",
        sent[1].1
    );

    let ids = snapshot
        .entries
        .iter()
        .map(|entry| entry.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["usage:1789344000:EUR", "usage:1789344000:USD"]);
    support::assert_quota_contract(None, &[], &snapshot.entries, OBSERVE_ONLY);
    let QuotaValue::Budget(usd) = &snapshot.entries[1].value else {
        panic!("a budget");
    };
    assert_eq!(
        usd.used,
        Some(Decimal::new(175, 2)),
        "a day's rows are summed per currency"
    );
    assert_eq!(usd.period_start_ms, Some(1_789_344_000_000));

    // An inference key is not an Admin key; there is nothing to ask with.
    let client = ScriptClient::new(Vec::new());
    assert!(matches!(
        query
            .query(CredentialContext {
                provider: provider(&json!({}), None),
                credential: credential(&json!({"api_key": "sk-inference"})),
                client: &client,
            })
            .await,
        Err(ChannelError::InvalidCredential)
    ));
}
