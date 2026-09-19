#![cfg(feature = "custom")]

use gproxy_channel::{
    BaseChannel, ChannelError,
    channel::{CredentialView, PrepareContext, ProviderView},
    channels::custom::Custom,
};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, connection::Bytes};
use http::{HeaderMap, HeaderValue, Method};
use serde_json::{Value, json};

fn provider<'a>(config: &'a Value, base_url: Option<&'a str>) -> ProviderView<'a> {
    ProviderView {
        id: "p",
        channel: "custom",
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
        metadata: &serde_json::Value::Null,
        version: 0,
        expires_at_ms: None,
    }
}
fn request(path: &str, query: Option<&str>) -> WireRequest<HttpBody> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer client-secret"),
    );
    headers.insert("x-api-key", HeaderValue::from_static("client-secret"));
    headers.insert("host", HeaderValue::from_static("gproxy.local"));
    headers.insert("content-length", HeaderValue::from_static("2"));
    headers.insert("anthropic-beta", HeaderValue::from_static("files-api"));
    headers.append("x-multi", HeaderValue::from_static("1"));
    headers.append("x-multi", HeaderValue::from_static("2"));
    WireRequest {
        method: Method::POST,
        path: path.into(),
        query: query.map(str::to_owned),
        headers,
        body: HttpBody::Bytes(Bytes::from_static(b"{}")),
    }
}
fn prepare<'a>(
    config: &'a Value,
    base_url: Option<&'a str>,
    secret: &'a Value,
    dialect: Dialect,
    request: WireRequest<HttpBody>,
    endpoint_override: Option<&'a str>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    Custom.prepare(PrepareContext {
        provider: provider(config, base_url),
        credential: credential(secret),
        operation: OperationKey {
            operation: Operation::GenerateContent,
            dialect,
        },
        request,
        endpoint_override,
    })
}

#[test]
fn joins_base_url_and_path_and_authenticates_per_family() {
    let config = json!({});
    let secret = json!({"api_key": "sk-upstream"});
    let openai = prepare(
        &config,
        Some("https://api.example/v1/"),
        &secret,
        Dialect::OpenAi,
        request("/v1/responses", Some("key=leak&stream=true&access_token=x")),
        None,
    )
    .unwrap();
    assert_eq!(
        openai.uri(),
        "https://api.example/v1/v1/responses?stream=true"
    );
    assert_eq!(openai.headers()["authorization"], "Bearer sk-upstream");
    assert!(openai.headers().get("x-api-key").is_none());
    assert!(openai.headers().get("host").is_none());
    assert!(openai.headers().get("content-length").is_none());
    assert_eq!(openai.headers()["anthropic-beta"], "files-api");
    assert_eq!(openai.headers().get_all("x-multi").iter().count(), 2);

    let claude = prepare(
        &config,
        Some("https://claude.example"),
        &secret,
        Dialect::Claude,
        request("v1/messages", None),
        None,
    )
    .unwrap();
    assert_eq!(claude.uri(), "https://claude.example/v1/messages");
    assert_eq!(claude.headers()["x-api-key"], "sk-upstream");
    assert_eq!(claude.headers()["anthropic-version"], "2023-06-01");
    assert!(claude.headers().get("authorization").is_none());

    let gemini = prepare(
        &config,
        Some("https://g.example"),
        &secret,
        Dialect::Gemini,
        request("/v1beta/models/x:generateContent", None),
        Some("https://override.example/gen"),
    )
    .unwrap();
    assert_eq!(gemini.uri(), "https://override.example/gen");
    assert_eq!(gemini.headers()["x-goog-api-key"], "sk-upstream");
}

#[test]
fn config_overrides_auth_and_adds_headers_and_declares_dialects() {
    let config = json!({
        "dialects": ["openai_chat"],
        "auth_header": "api-key",
        "headers": {"x-vendor": "yes"}
    });
    let secret = json!({"api_key": "k"});
    let prepared = prepare(
        &config,
        Some("https://azure.example"),
        &secret,
        Dialect::OpenAiChat,
        request(
            "/openai/deployments/x/chat/completions",
            Some("api-version=2024"),
        ),
        None,
    )
    .unwrap();
    assert_eq!(
        prepared.uri(),
        "https://azure.example/openai/deployments/x/chat/completions?api-version=2024"
    );
    assert_eq!(prepared.headers()["api-key"], "k");
    assert!(prepared.headers().get("authorization").is_none());
    assert_eq!(prepared.headers()["x-vendor"], "yes");
    assert_eq!(
        Custom.native_dialects(provider(&config, None), Operation::GenerateContent),
        [Dialect::OpenAiChat]
    );
    assert_eq!(
        Custom
            .native_dialects(provider(&json!({}), None), Operation::ListModels)
            .len(),
        4
    );

    assert!(matches!(
        prepare(
            &json!({}),
            None,
            &secret,
            Dialect::OpenAi,
            request("/x", None),
            None
        ),
        Err(ChannelError::InvalidConfig(_))
    ));
    assert!(matches!(
        prepare(
            &json!({}),
            Some("https://a"),
            &json!({}),
            Dialect::OpenAi,
            request("/x", None),
            None
        ),
        Err(ChannelError::InvalidCredential)
    ));
    let ws = Custom
        .prepare_connect(PrepareContext {
            provider: provider(&json!({}), Some("https://rt.example")),
            credential: credential(&secret),
            operation: OperationKey {
                operation: Operation::ConnectRealtime,
                dialect: Dialect::OpenAi,
            },
            request: WireRequest {
                method: Method::GET,
                path: "/v1/realtime".into(),
                query: Some("model=gpt-realtime".into()),
                headers: HeaderMap::new(),
                body: (),
            },
            endpoint_override: None,
        })
        .unwrap();
    assert_eq!(
        ws.uri(),
        "https://rt.example/v1/realtime?model=gpt-realtime"
    );
    assert_eq!(ws.headers()["authorization"], "Bearer k");
}
