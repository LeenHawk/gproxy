#![cfg(feature = "azure")]

//! Azure's two endpoint layouts, its per-family authentication and the
//! configuration errors an operator sees. No upstream is contacted: every
//! assertion is about the request this channel builds.

use gproxy_channel::channel::{CredentialView, PrepareContext, ProviderView};
use gproxy_channel::{BaseChannel, ChannelError, channels::azure::Azure};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, connection::Bytes};
use http::{HeaderMap, HeaderValue, Method};
use serde_json::{Value, json};

fn provider<'a>(config: &'a Value, base_url: Option<&'a str>) -> ProviderView<'a> {
    ProviderView {
        id: "p",
        channel: "azure",
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

/// A client request carrying its own authentication, which must never reach
/// the upstream, plus a vendor header that must.
fn request(path: &str, query: Option<&str>) -> WireRequest<HttpBody> {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", HeaderValue::from_static("Bearer client"));
    headers.insert("api-key", HeaderValue::from_static("client-key"));
    headers.insert("x-api-key", HeaderValue::from_static("client-key"));
    headers.insert("host", HeaderValue::from_static("gproxy.local"));
    headers.insert("anthropic-beta", HeaderValue::from_static("files-api"));
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
    Azure.prepare(PrepareContext {
        provider: provider(config, base_url),
        credential: credential(secret),
        operation: OperationKey { operation, dialect },
        request,
        endpoint_override,
    })
}

fn key() -> Value {
    json!({"api_key": "azure-secret"})
}

#[test]
fn v1_surface_mounts_each_family_under_its_prefix() {
    let config = json!({"resource": "contoso"});
    let secret = key();

    let responses = prepare(
        &config,
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::OpenAi,
        request("/v1/responses", Some("stream=true&key=leak")),
        None,
    )
    .unwrap();
    assert_eq!(
        responses.uri(),
        "https://contoso.openai.azure.com/openai/v1/responses?stream=true"
    );
    assert_eq!(responses.headers()["api-key"], "azure-secret");
    assert!(responses.headers().get("authorization").is_none());
    assert!(responses.headers().get("host").is_none());

    let chat = prepare(
        &config,
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        request("/v1/chat/completions", None),
        None,
    )
    .unwrap();
    assert_eq!(
        chat.uri(),
        "https://contoso.openai.azure.com/openai/v1/chat/completions"
    );

    let models = prepare(
        &config,
        None,
        &secret,
        Operation::ListModels,
        Dialect::OpenAi,
        request("/v1/models", None),
        None,
    )
    .unwrap();
    assert_eq!(
        models.uri(),
        "https://contoso.openai.azure.com/openai/v1/models"
    );

    let messages = prepare(
        &config,
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::Claude,
        request("/v1/messages", None),
        None,
    )
    .unwrap();
    assert_eq!(
        messages.uri(),
        "https://contoso.openai.azure.com/anthropic/v1/messages"
    );
    assert_eq!(messages.headers()["x-api-key"], "azure-secret");
    assert_eq!(messages.headers()["anthropic-version"], "2023-06-01");
    assert!(messages.headers().get("api-key").is_none());
    // A vendor header the client sent still rides along.
    assert_eq!(messages.headers()["anthropic-beta"], "files-api");
}

#[test]
fn base_url_replaces_the_resource_and_images_carry_their_own_version() {
    let config = json!({});
    let secret = key();
    let image = prepare(
        &config,
        Some("https://project.services.ai.azure.com/"),
        &secret,
        Operation::CreateImage,
        Dialect::OpenAi,
        request("/v1/images/generations", None),
        None,
    )
    .unwrap();
    assert_eq!(
        image.uri(),
        "https://project.services.ai.azure.com/openai/v1/images/generations?api-version=preview"
    );
}

#[test]
fn deployment_surface_rewrites_the_path_and_requires_a_version() {
    let config =
        json!({"resource": "contoso", "deployment": "gpt-4o", "api_version": "2024-10-21"});
    let secret = key();
    let chat = prepare(
        &config,
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        request("/v1/chat/completions", None),
        None,
    )
    .unwrap();
    assert_eq!(
        chat.uri(),
        "https://contoso.openai.azure.com/openai/deployments/gpt-4o/chat/completions?api-version=2024-10-21"
    );

    // Claude is not deployment-scoped even when a deployment is configured.
    let messages = prepare(
        &config,
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::Claude,
        request("/v1/messages", None),
        None,
    )
    .unwrap();
    assert_eq!(
        messages.uri(),
        "https://contoso.openai.azure.com/anthropic/v1/messages?api-version=2024-10-21"
    );

    // Neither is the model directory: it belongs to the resource.
    let models = prepare(
        &config,
        None,
        &secret,
        Operation::ListModels,
        Dialect::OpenAi,
        request("/v1/models", None),
        None,
    )
    .unwrap();
    assert_eq!(
        models.uri(),
        "https://contoso.openai.azure.com/openai/models?api-version=2024-10-21"
    );

    let missing = prepare(
        &json!({"resource": "contoso", "deployment": "gpt-4o"}),
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        request("/v1/chat/completions", None),
        None,
    )
    .unwrap_err();
    assert!(
        matches!(&missing, ChannelError::InvalidConfig(message) if message.contains("`api_version`")),
        "{missing}"
    );
}

#[test]
fn endpoint_override_wins_over_the_computed_layout() {
    let config = json!({"resource": "contoso", "api_version": "2024-10-21"});
    let secret = key();
    let overridden = prepare(
        &config,
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::OpenAi,
        request("/v1/responses", Some("stream=true")),
        Some("https://private.example/route?api-version=2025-01-01"),
    )
    .unwrap();
    assert_eq!(
        overridden.uri(),
        "https://private.example/route?api-version=2025-01-01&stream=true"
    );
    assert_eq!(overridden.headers()["api-key"], "azure-secret");
}

#[test]
fn a_missing_resource_names_the_configuration_key() {
    let error = prepare(
        &json!({}),
        None,
        &key(),
        Operation::GenerateContent,
        Dialect::OpenAi,
        request("/v1/responses", None),
        None,
    )
    .unwrap_err();
    assert!(
        matches!(&error, ChannelError::InvalidConfig(message) if message.contains("`resource`")),
        "{error}"
    );
}

#[test]
fn gemini_is_not_served() {
    let error = prepare(
        &json!({"resource": "contoso"}),
        None,
        &key(),
        Operation::GenerateContent,
        Dialect::Gemini,
        request("/v1beta/models/x:generateContent", None),
        None,
    )
    .unwrap_err();
    assert!(
        matches!(error, ChannelError::UnsupportedOperation(_)),
        "Gemini has no Azure surface"
    );
}

#[test]
fn a_credential_without_a_key_is_refused() {
    let error = prepare(
        &json!({"resource": "contoso"}),
        None,
        &json!({"api_key": "  "}),
        Operation::GenerateContent,
        Dialect::OpenAi,
        request("/v1/responses", None),
        None,
    )
    .unwrap_err();
    assert!(matches!(error, ChannelError::InvalidCredential), "{error}");
}

#[test]
fn every_declared_dialect_can_be_prepared() {
    let config = json!({"resource": "contoso"});
    let secret = key();
    let view = provider(&config, None);
    let paths = |dialect: Dialect, operation: Operation| match (operation, dialect) {
        (Operation::ListModels, _) => "/v1/models",
        (Operation::GetModel, _) => "/v1/models/gpt-4o",
        (Operation::CountTokens, _) => "/v1/messages/count_tokens",
        (Operation::CreateEmbedding, _) => "/v1/embeddings",
        (Operation::CreateImage, _) => "/v1/images/generations",
        (Operation::EditImage, _) => "/v1/images/edits",
        (Operation::CompactContent, _) => "/v1/responses",
        (Operation::CreateVideo | Operation::ListVideos, _) => "/v1/videos",
        (Operation::RetrieveVideo | Operation::DeleteVideo, _) => "/v1/videos/vid_1",
        (Operation::DownloadVideoContent, _) => "/v1/videos/vid_1/content",
        (_, Dialect::Claude) => "/v1/messages",
        (_, Dialect::OpenAiChat) => "/v1/chat/completions",
        (_, _) => "/v1/responses",
    };
    let mut declared = 0;
    for operation in [
        Operation::ListModels,
        Operation::GetModel,
        Operation::CountTokens,
        Operation::GenerateContent,
        Operation::StreamGenerateContent,
        Operation::CompactContent,
        Operation::CreateEmbedding,
        Operation::CreateImage,
        Operation::EditImage,
        Operation::CreateVideo,
        Operation::RetrieveVideo,
        Operation::ListVideos,
        Operation::DeleteVideo,
        Operation::DownloadVideoContent,
    ] {
        for dialect in Azure.native_dialects(view, operation) {
            declared += 1;
            let prepared = prepare(
                &config,
                None,
                &secret,
                operation,
                dialect,
                request(paths(dialect, operation), None),
                None,
            )
            .unwrap_or_else(|error| {
                panic!("declared {operation:?}/{dialect:?} must prepare: {error}")
            });
            let uri = prepared.uri().to_string();
            assert!(
                uri.starts_with("https://contoso.openai.azure.com/openai/")
                    || uri.starts_with("https://contoso.openai.azure.com/anthropic/"),
                "{operation:?}/{dialect:?} produced {uri}"
            );
        }
    }
    assert_eq!(declared, 18, "the declared surface changed");
    // Nothing else is declared, and an undeclared operation is refused.
    assert!(
        Azure
            .native_dialects(view, Operation::CreateSpeech)
            .is_empty()
    );
}

#[test]
fn the_descriptor_names_the_keys_the_channel_reads() {
    let descriptor = Azure.descriptor();
    assert_eq!(descriptor.id, "azure");
    for key in ["resource", "api_version", "deployment", "base_url"] {
        assert!(descriptor.config_key(key).is_some(), "missing {key}");
    }
    assert!(!descriptor.capabilities.refresh);
    // Azure fingerprints nothing, so the operator's profile is the only input.
    assert!(Azure.default_connection().is_none());
}

#[test]
fn a_magic_cache_string_is_stripped_and_marked_only_when_asked() {
    const TOKEN: &str = "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_\
                         49VA1S5V19GR4G89W2V695G9W9GV52W95V198WV5W2FC9DF";
    let secret = key();
    let body = |config: Value, dialect, path| {
        let mut caller = request(path, None);
        caller.body = HttpBody::Bytes(Bytes::from(
            json!({"model": "m", "messages": [
                {"role": "user", "content": [{"type": "text", "text": format!("ctx {TOKEN}")}]}
            ]})
            .to_string(),
        ));
        let prepared = prepare(
            &config,
            None,
            &secret,
            Operation::GenerateContent,
            dialect,
            caller,
            None,
        )
        .unwrap();
        let HttpBody::Bytes(bytes) = prepared.into_body() else {
            panic!("buffered")
        };
        serde_json::from_slice::<Value>(&bytes).unwrap()
    };
    let off = body(json!({"resource": "r"}), Dialect::Claude, "/v1/messages");
    let block = &off["messages"][0]["content"][0];
    assert!(
        !block["text"].as_str().unwrap().contains("GPROXY_MAGIC"),
        "stripped either way"
    );
    assert!(block["cache_control"].is_null());
    let on = body(
        json!({"resource": "r", "enable_claude_magic_cache": true}),
        Dialect::Claude,
        "/v1/messages",
    );
    assert_eq!(
        on["messages"][0]["content"][0]["cache_control"]["type"],
        "ephemeral"
    );
}
