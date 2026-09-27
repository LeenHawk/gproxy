#![cfg(feature = "vertexexpress")]

//! Vertex Express mode: one global origin, a Gemini-only method surface and
//! the API key in the query. No upstream is contacted.

use gproxy_channel::channel::{CredentialView, PrepareContext, ProviderView};
use gproxy_channel::{BaseChannel, ChannelError, channels::vertexexpress::VertexExpress};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, connection::Bytes};
use http::{HeaderMap, HeaderValue, Method};
use serde_json::{Value, json};

fn provider<'a>(config: &'a Value, base_url: Option<&'a str>) -> ProviderView<'a> {
    ProviderView {
        id: "p",
        channel: "vertexexpress",
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
        version: 1,
        expires_at_ms: None,
    }
}

fn request(path: &str, query: Option<&str>) -> WireRequest<HttpBody> {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", HeaderValue::from_static("Bearer client"));
    headers.insert("x-goog-api-key", HeaderValue::from_static("client-key"));
    headers.insert("host", HeaderValue::from_static("gproxy.local"));
    headers.insert("x-vendor", HeaderValue::from_static("kept"));
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
    operation: Operation,
    dialect: Dialect,
    request: WireRequest<HttpBody>,
    endpoint_override: Option<&'a str>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    VertexExpress.prepare(PrepareContext {
        provider: provider(config, base_url),
        credential: credential(secret),
        operation: OperationKey { operation, dialect },
        request,
        endpoint_override,
    })
}

fn key() -> Value {
    json!({"api_key": "express/key+1"})
}

#[test]
fn methods_are_publisher_scoped_and_the_key_rides_in_the_query() {
    let config = json!({});
    let secret = key();

    let generate = prepare(
        &config,
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::Gemini,
        request("/v1beta/models/gemini-2.5-flash:generateContent", None),
        None,
    )
    .unwrap();
    assert_eq!(
        generate.uri(),
        "https://aiplatform.googleapis.com/v1/publishers/google/models/gemini-2.5-flash:generateContent?key=express%2Fkey%2B1"
    );
    // The key is never a header here, and the caller's own auth is gone.
    assert!(generate.headers().get("authorization").is_none());
    assert!(generate.headers().get("x-goog-api-key").is_none());
    assert!(generate.headers().get("host").is_none());
    assert_eq!(generate.headers()["x-vendor"], "kept");

    let streamed = prepare(
        &config,
        None,
        &secret,
        Operation::StreamGenerateContent,
        Dialect::Gemini,
        request(
            "/v1beta/models/gemini-2.5-flash:streamGenerateContent",
            Some("alt=sse&key=leak"),
        ),
        None,
    )
    .unwrap();
    assert_eq!(
        streamed.uri(),
        "https://aiplatform.googleapis.com/v1/publishers/google/models/gemini-2.5-flash:streamGenerateContent?alt=sse&key=express%2Fkey%2B1"
    );

    let count = prepare(
        &config,
        None,
        &secret,
        Operation::CountTokens,
        Dialect::Gemini,
        request("/v1beta/models/gemini-2.5-flash:countTokens", None),
        None,
    )
    .unwrap();
    assert_eq!(
        count.uri(),
        "https://aiplatform.googleapis.com/v1/publishers/google/models/gemini-2.5-flash:countTokens?key=express%2Fkey%2B1"
    );
}

#[test]
fn base_url_replaces_the_global_origin() {
    let prepared = prepare(
        &json!({}),
        Some("https://express.proxy.example/"),
        &key(),
        Operation::GenerateContent,
        Dialect::Gemini,
        request("/v1beta/models/gemini-2.5-flash:generateContent", None),
        None,
    )
    .unwrap();
    assert!(
        prepared
            .uri()
            .to_string()
            .starts_with("https://express.proxy.example/v1/publishers/google/models/"),
        "{}",
        prepared.uri()
    );
}

#[test]
fn endpoint_override_wins_but_must_not_carry_a_key() {
    let overridden = prepare(
        &json!({}),
        None,
        &key(),
        Operation::GenerateContent,
        Dialect::Gemini,
        request("/v1beta/models/gemini-2.5-flash:generateContent", None),
        Some("https://tuned.example/v1/models/mine:generateContent?alt=json"),
    )
    .unwrap();
    assert_eq!(
        overridden.uri(),
        "https://tuned.example/v1/models/mine:generateContent?alt=json&key=express%2Fkey%2B1"
    );

    let error = prepare(
        &json!({}),
        None,
        &key(),
        Operation::GenerateContent,
        Dialect::Gemini,
        request("/v1beta/models/gemini-2.5-flash:generateContent", None),
        Some("https://tuned.example/v1/models/mine:generateContent?key=pasted"),
    )
    .unwrap_err();
    assert!(
        matches!(&error, ChannelError::InvalidConfig(message) if message.contains("API key")),
        "{error}"
    );
}

#[test]
fn a_request_without_a_model_names_what_is_missing() {
    let error = prepare(
        &json!({}),
        None,
        &key(),
        Operation::GenerateContent,
        Dialect::Gemini,
        request("/v1beta/models/", None),
        None,
    )
    .unwrap_err();
    assert!(
        matches!(&error, ChannelError::InvalidConfig(message) if message.contains("model")),
        "{error}"
    );
}

#[test]
fn a_credential_without_a_key_is_refused() {
    let error = prepare(
        &json!({}),
        None,
        &json!({}),
        Operation::GenerateContent,
        Dialect::Gemini,
        request("/v1beta/models/gemini-2.5-flash:generateContent", None),
        None,
    )
    .unwrap_err();
    assert!(matches!(error, ChannelError::InvalidCredential), "{error}");
}

#[test]
fn every_declared_dialect_can_be_prepared_and_nothing_else_is_served() {
    let config = json!({});
    let secret = key();
    let view = provider(&config, None);
    let mut declared = 0;
    for operation in [
        Operation::GenerateContent,
        Operation::StreamGenerateContent,
        Operation::CountTokens,
    ] {
        for dialect in VertexExpress.native_dialects(view, operation) {
            declared += 1;
            assert_eq!(dialect, Dialect::Gemini);
            prepare(
                &config,
                None,
                &secret,
                operation,
                dialect,
                request("/v1beta/models/gemini-2.5-flash:generateContent", None),
                None,
            )
            .unwrap_or_else(|error| panic!("declared {operation:?} must prepare: {error}"));
        }
    }
    assert_eq!(declared, 3, "the declared surface changed");

    // Express mode has no Anthropic publisher and no embeddings.
    assert!(
        VertexExpress
            .native_dialects(view, Operation::CreateEmbedding)
            .is_empty()
    );
    let error = prepare(
        &config,
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::Claude,
        request("/v1/messages", None),
        None,
    )
    .unwrap_err();
    assert!(
        matches!(error, ChannelError::UnsupportedOperation(_)),
        "{error}"
    );
}

#[test]
fn the_descriptor_declares_an_api_key_and_no_project_keys() {
    let descriptor = VertexExpress.descriptor();
    assert_eq!(descriptor.id, "vertexexpress");
    assert!(descriptor.config_key("base_url").is_some());
    // The whole point of express mode: no project, no region.
    assert!(descriptor.config_key("project").is_none());
    assert!(descriptor.config_key("location").is_none());
    assert!(!descriptor.capabilities.refresh);
    assert!(VertexExpress.default_connection().is_none());
}
