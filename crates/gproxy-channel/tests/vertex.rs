#![cfg(feature = "vertex")]

//! Vertex's project-scoped endpoint layout, its publisher surfaces, and the
//! service-account token exchange. The token endpoint is scripted; no real
//! Google service is contacted.

use std::collections::VecDeque;
use std::sync::Mutex;

use gproxy_channel::channel::{CredentialContext, CredentialView, PrepareContext, ProviderView};
use gproxy_channel::{BaseChannel, ChannelError, OutboundClient, channels::vertex::Vertex};
use gproxy_protocol::capability::{CapabilityError, CapabilityFuture};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse, connection::Bytes,
};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::{Value, json};

/// A throwaway RSA key generated for this test file alone. It signs nothing
/// that exists: no Google project has ever seen it.
const TEST_PRIVATE_KEY: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvAIBADANBgkqhkiG9w0BAQEFAASCBKYwggSiAgEAAoIBAQCy8jsp+a0kNCa+\n189JGlwhzRc7stF8Vi/5XqxUeqXzmLq0NsxcqFTYbM3Vh1O+XuAFORfII2aPVg+u\n8WASynaCvAVjAYmSCxt2zzsItBnvzh591u/2uu7Ao+geJ+xIZ0aE/RmKK1kTmeKv\n/it3ZrgEWb55d+blTQm6n2a1fT6g+LN9IggrjYhJFyIZyG+FEP2BYGvRk8Omd0bE\nFOLhPut90Rxc/BLBc+ij0aVePvBC9mSdeIGy0TCA6dQDeARTockFQSikKuFoUoPP\nekqgv35Er70ZJ5jUCTtZbPhxOTUtFdPjHmrUfr6ggJvVA5aJKSvQmyWzPEA60Ylt\nVNH03DD/AgMBAAECggEAD6ijD7xsRRJBNQr/ywxWSpOT+GKLR/TndCFaNXbuuv8U\nO2feKy+EY9SQOyKr69PeiOZ4PnMGlagOGEdr755YOilyKTnoYWdnwFZIUAMpOohQ\nDRVNYKBL+ynR9S2WKgx2KctXZvVTG/l9J4cMzlZ753mORuUnRyJzev6u3KvN2q+z\nHh/2edfUnxiA8Fnb/ysJjUVBUwEgCS1UKMROzKD+hGtiucfSiTLHHSADYb8ZB7OC\n4vqAV5UQtpuGurgyP/xmo/rCIzzCiqocFpW5vvFOqUzH4KZiZSJMRTYiGrbylyIx\n/a2kSmZwc5jtmo5LF16h7/1paT3Zdnt0tUaMdp3VJQKBgQDi38J9lZ+wsAmMcfLU\nCF/WLXiF2q/XL7xwqwMKUfIGXgDH9m+395aFy2EVfm3faeL4AeYKyoPbiFDbTYT7\nMb/HfHCVWFvLYohQOe5IRHJMZrQaZiXWp6ZrbPeNmtg85wPrtdbRpbx2H9YkvDeG\nhYf7fKLz0mfEpwXCwZQa3/ZCnQKBgQDJ61FN5kYeS5NeGc7Z7dYxz83NYuAgphN3\nxyKqJRy8xD9msBrDRhUcrTF4oqG4ggjTbYJZpmN5i6Eaat57BZjUK9IBjGRXL0w9\nayIBZK8XjB/JHVdgAL4Fuoxv7PizYGEuXPxlqMzp/eKPhddR7K6xgltObQBRJ67L\n6cSUqD9RSwKBgFfzInSI0nUuaSU270nfTTe8POK3Gj+zU7vhr7YKemaZfngGQtzw\ncDvB0gsBDhrz83btVX6Nb3xlZeL+NDUk3hG5XfOnYz5/HhTrwEHntt+DWQJ64uRJ\n7avrfDQ6+OTzMYPo5DQ1qc+pG9z10himH0cQ1CLtSCjmDsenP4EDnXXJAoGAHiNn\nkU8LrD3vkx4bB+A+FlVEDKHzfiwLv9cTT34Wmf5Y0ET82aS+Rfd76Nutc9LE6nnv\n+N2i/2Nd+ol1B7vAIfsgb2a7G2BN6uTwwHB8yfD6VZRxlDzIICbGC3a9cFi0aK0s\nZygY3dwtUurRRsMjGA+y/TO71mEr7/fGhcHPIZ0CgYA+bnnnSer6Vgq8vDP8T7Hn\n5vtVSDMMjwzIGcGAq/kmfMpIFD47QmKXuYm0imbiTmIYMnXHtcTT/qavN/FupJAH\nuTXhj+agp1OmpCekDl2Nj/wV74eGF/YJDsGLPSSjxWqsOKlF1YYipAinQLgiwu2i\nxpuGaenJ0ML6asab6pCNyA==\n-----END PRIVATE KEY-----";

struct ScriptClient {
    replies: Mutex<VecDeque<WireResponse>>,
    requests: Mutex<Vec<(http::request::Parts, Bytes)>>,
}

impl ScriptClient {
    fn new(replies: Vec<WireResponse>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
        }
    }
    fn sent(&self) -> Vec<(http::request::Parts, Bytes)> {
        std::mem::take(&mut self.requests.lock().unwrap())
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
                HttpBody::Bytes(bytes) => bytes,
                HttpBody::Stream(_) => Bytes::new(),
            };
            self.requests.lock().unwrap().push((parts, body));
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
    WireResponse {
        status,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&value).unwrap())),
    }
}

fn provider<'a>(config: &'a Value, base_url: Option<&'a str>) -> ProviderView<'a> {
    ProviderView {
        id: "p",
        channel: "vertex",
        base_url,
        config,
    }
}

fn credential(secret: &Value) -> CredentialView<'_> {
    CredentialView {
        id: "c",
        provider_id: "p",
        auth_kind: "service_account",
        secret,
        metadata: &Value::Null,
        version: 3,
        expires_at_ms: None,
    }
}

fn request(path: &str, query: Option<&str>, body: Value) -> WireRequest<HttpBody> {
    let mut headers = HeaderMap::new();
    headers.insert("authorization", HeaderValue::from_static("Bearer client"));
    headers.insert("x-goog-api-key", HeaderValue::from_static("client-key"));
    headers.insert("host", HeaderValue::from_static("gproxy.local"));
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    WireRequest {
        method: Method::POST,
        path: path.into(),
        query: query.map(str::to_owned),
        headers,
        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&body).unwrap())),
    }
}

/// A credential a refresh has already filled in.
fn active_secret() -> Value {
    json!({
        "client_email": "robot@example.iam.gserviceaccount.com",
        "private_key": TEST_PRIVATE_KEY,
        "project_id": "demo-project",
        "access_token": "ya29.minted",
        "expires_at_ms": 4_000_000_000_000i64,
    })
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
    Vertex.prepare(PrepareContext {
        provider: provider(config, base_url),
        credential: credential(secret),
        operation: OperationKey { operation, dialect },
        request,
        endpoint_override,
    })
}

/// `CredentialUpdate` is deliberately not `Debug`, so a refusal is unwrapped
/// by hand rather than through `unwrap_err`.
fn refusal(
    result: Result<gproxy_channel::channel::CredentialUpdate, ChannelError>,
) -> ChannelError {
    match result {
        Ok(_) => panic!("expected the upstream to refuse"),
        Err(error) => error,
    }
}

fn body_of(request: http::Request<HttpBody>) -> Value {
    match request.into_body() {
        HttpBody::Bytes(bytes) => serde_json::from_slice(&bytes).unwrap(),
        HttpBody::Stream(_) => panic!("expected a buffered body"),
    }
}

#[test]
fn google_models_are_addressed_under_the_project_and_region() {
    let config = json!({"location": "europe-west4"});
    let secret = active_secret();

    let generate = prepare(
        &config,
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::Gemini,
        request(
            "/v1beta/models/gemini-2.5-pro:generateContent",
            None,
            json!({}),
        ),
        None,
    )
    .unwrap();
    assert_eq!(
        generate.uri(),
        "https://europe-west4-aiplatform.googleapis.com/v1beta1/projects/demo-project/locations/europe-west4/publishers/google/models/gemini-2.5-pro:generateContent"
    );
    assert_eq!(generate.headers()["authorization"], "Bearer ya29.minted");
    assert!(generate.headers().get("x-goog-api-key").is_none());
    assert!(generate.headers().get("host").is_none());

    let streamed = prepare(
        &config,
        None,
        &secret,
        Operation::StreamGenerateContent,
        Dialect::Gemini,
        request(
            "/v1beta/models/gemini-2.5-pro:streamGenerateContent",
            Some("alt=sse"),
            json!({}),
        ),
        None,
    )
    .unwrap();
    assert_eq!(
        streamed.uri(),
        "https://europe-west4-aiplatform.googleapis.com/v1beta1/projects/demo-project/locations/europe-west4/publishers/google/models/gemini-2.5-pro:streamGenerateContent?alt=sse"
    );

    let count = prepare(
        &config,
        None,
        &secret,
        Operation::CountTokens,
        Dialect::Gemini,
        request("/v1beta/models/gemini-2.5-pro:countTokens", None, json!({})),
        None,
    )
    .unwrap();
    assert!(count.uri().to_string().ends_with(":countTokens"));

    let embed = prepare(
        &config,
        None,
        &secret,
        Operation::CreateEmbedding,
        Dialect::Gemini,
        request(
            "/v1beta/models/text-embedding-005:embedContent",
            None,
            json!({}),
        ),
        None,
    )
    .unwrap();
    assert!(
        embed
            .uri()
            .to_string()
            .ends_with("/publishers/google/models/text-embedding-005:embedContent")
    );
}

#[test]
fn the_model_directory_is_project_free_and_global_has_no_regional_host() {
    let config = json!({"project": "demo-project", "location": "global"});
    let secret = active_secret();
    let list = prepare(
        &config,
        None,
        &secret,
        Operation::ListModels,
        Dialect::Gemini,
        request("/v1beta/models", None, json!({})),
        None,
    )
    .unwrap();
    assert_eq!(
        list.uri(),
        "https://aiplatform.googleapis.com/v1beta1/publishers/google/models"
    );

    let get = prepare(
        &config,
        None,
        &secret,
        Operation::GetModel,
        Dialect::Gemini,
        request("/v1beta/models/gemini-2.5-flash", None, json!({})),
        None,
    )
    .unwrap();
    assert_eq!(
        get.uri(),
        "https://aiplatform.googleapis.com/v1beta1/publishers/google/models/gemini-2.5-flash"
    );
}

#[test]
fn anthropic_models_move_to_raw_predict_and_lose_the_body_model() {
    let config = json!({});
    let secret = active_secret();
    let generate = prepare(
        &config,
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::Claude,
        request(
            "/v1/messages",
            None,
            json!({"model": "claude-sonnet-4-5@20250929", "max_tokens": 16}),
        ),
        None,
    )
    .unwrap();
    assert_eq!(
        generate.uri(),
        "https://us-central1-aiplatform.googleapis.com/v1/projects/demo-project/locations/us-central1/publishers/anthropic/models/claude-sonnet-4-5@20250929:rawPredict"
    );
    // Vertex's Anthropic publisher rejects the direct API's version header and
    // requires its own version in the body instead.
    assert!(generate.headers().get("anthropic-version").is_none());
    let body = body_of(generate);
    assert_eq!(body["anthropic_version"], "vertex-2023-10-16");
    assert!(body.get("model").is_none(), "the URL already names it");
    assert_eq!(body["max_tokens"], 16);

    let streamed = prepare(
        &config,
        None,
        &secret,
        Operation::StreamGenerateContent,
        Dialect::Claude,
        request(
            "/v1/messages",
            None,
            json!({"model": "claude-sonnet-4-5@20250929", "stream": true}),
        ),
        None,
    )
    .unwrap();
    assert!(streamed.uri().to_string().ends_with(":streamRawPredict"));

    // Counting tokens is one endpoint for every model, so the body keeps its.
    let count = prepare(
        &config,
        None,
        &secret,
        Operation::CountTokens,
        Dialect::Claude,
        request(
            "/v1/messages/count_tokens",
            None,
            json!({"model": "claude-sonnet-4-5@20250929"}),
        ),
        None,
    )
    .unwrap();
    assert_eq!(
        count.uri(),
        "https://us-central1-aiplatform.googleapis.com/v1/projects/demo-project/locations/us-central1/publishers/anthropic/models/count-tokens:rawPredict"
    );
    assert_eq!(body_of(count)["model"], "claude-sonnet-4-5@20250929");
}

#[test]
fn the_openai_compatible_surface_has_its_own_endpoint() {
    let chat = prepare(
        &json!({}),
        None,
        &active_secret(),
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        request(
            "/v1/chat/completions",
            None,
            json!({"model": "google/gemini-2.5-pro"}),
        ),
        None,
    )
    .unwrap();
    assert_eq!(
        chat.uri(),
        "https://us-central1-aiplatform.googleapis.com/v1beta1/projects/demo-project/locations/us-central1/endpoints/openapi/chat/completions"
    );
    // The body is the caller's own; this surface names no model in the URL.
    assert_eq!(body_of(chat)["model"], "google/gemini-2.5-pro");
}

#[test]
fn endpoint_override_wins_over_the_computed_layout() {
    let overridden = prepare(
        &json!({}),
        None,
        &active_secret(),
        Operation::StreamGenerateContent,
        Dialect::Gemini,
        request(
            "/v1beta/models/gemini-2.5-pro:streamGenerateContent",
            Some("alt=sse"),
            json!({}),
        ),
        Some("https://tuned.example/v1/projects/x/locations/y/endpoints/123:streamGenerateContent"),
    )
    .unwrap();
    assert_eq!(
        overridden.uri(),
        "https://tuned.example/v1/projects/x/locations/y/endpoints/123:streamGenerateContent?alt=sse"
    );
    assert_eq!(overridden.headers()["authorization"], "Bearer ya29.minted");
}

#[test]
fn a_missing_project_names_the_configuration_key() {
    let secret = json!({
        "client_email": "robot@example.iam.gserviceaccount.com",
        "private_key": TEST_PRIVATE_KEY,
        "access_token": "ya29.minted",
    });
    let error = prepare(
        &json!({}),
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::Gemini,
        request(
            "/v1beta/models/gemini-2.5-pro:generateContent",
            None,
            json!({}),
        ),
        None,
    )
    .unwrap_err();
    assert!(
        matches!(&error, ChannelError::InvalidConfig(message) if message.contains("`project`")),
        "{error}"
    );
}

#[test]
fn prepare_never_mints_a_token_and_says_so_when_none_exists() {
    let secret = json!({
        "client_email": "robot@example.iam.gserviceaccount.com",
        "private_key": TEST_PRIVATE_KEY,
        "project_id": "demo-project",
    });
    let error = prepare(
        &json!({}),
        None,
        &secret,
        Operation::GenerateContent,
        Dialect::Gemini,
        request(
            "/v1beta/models/gemini-2.5-pro:generateContent",
            None,
            json!({}),
        ),
        None,
    )
    .unwrap_err();
    assert!(matches!(error, ChannelError::InvalidCredential), "{error}");
}

#[test]
fn every_declared_dialect_can_be_prepared() {
    let config = json!({});
    let secret = active_secret();
    let view = provider(&config, None);
    let mut declared = 0;
    for operation in [
        Operation::ListModels,
        Operation::GetModel,
        Operation::CountTokens,
        Operation::GenerateContent,
        Operation::StreamGenerateContent,
        Operation::CreateEmbedding,
        Operation::BatchCreateEmbedding,
    ] {
        for dialect in Vertex.native_dialects(view, operation) {
            declared += 1;
            let (path, body) = match (operation, dialect) {
                (Operation::ListModels, _) => ("/v1beta/models".to_owned(), json!({})),
                (Operation::GetModel, _) => ("/v1beta/models/gemini-2.5-pro".to_owned(), json!({})),
                (_, Dialect::Claude) => (
                    "/v1/messages".to_owned(),
                    json!({"model": "claude-sonnet-4-5@20250929"}),
                ),
                (_, Dialect::OpenAiChat) => ("/v1/chat/completions".to_owned(), json!({})),
                (operation, _) => (
                    format!("/v1beta/models/gemini-2.5-pro:{}", operation.id()),
                    json!({}),
                ),
            };
            let prepared = prepare(
                &config,
                None,
                &secret,
                operation,
                dialect,
                request(&path, None, body),
                None,
            )
            .unwrap_or_else(|error| {
                panic!("declared {operation:?}/{dialect:?} must prepare: {error}")
            });
            assert!(
                prepared
                    .uri()
                    .to_string()
                    .starts_with("https://us-central1-aiplatform.googleapis.com/"),
                "{operation:?}/{dialect:?} produced {}",
                prepared.uri()
            );
        }
    }
    assert_eq!(declared, 12, "the declared surface changed");
    assert!(
        Vertex
            .native_dialects(view, Operation::CreateImage)
            .is_empty()
    );
}

#[tokio::test]
async fn refresh_mints_an_access_token_and_reports_its_expiry() {
    let secret = json!({
        "client_email": "robot@example.iam.gserviceaccount.com",
        "private_key": TEST_PRIVATE_KEY,
        "project_id": "demo-project",
        "token_uri": "https://token.test/token",
    });
    let config = json!({});
    let client = ScriptClient::new(vec![reply(
        StatusCode::OK,
        json!({"access_token": "ya29.fresh", "expires_in": 3599, "token_type": "Bearer"}),
    )]);
    let refresh = Vertex.credential_refresh().expect("vertex refreshes");
    let update = refresh
        .refresh(CredentialContext {
            provider: provider(&config, None),
            credential: credential(&secret),
            client: &client,
        })
        .await
        .expect("mint");

    // The key endpoint is asked for a JWT bearer grant.
    let sent = client.sent();
    let (parts, body) = &sent[0];
    assert_eq!(parts.uri, "https://token.test/token");
    assert_eq!(parts.method, Method::POST);
    assert_eq!(
        parts.headers["content-type"],
        "application/x-www-form-urlencoded"
    );
    let form = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        form.starts_with(
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion="
        ),
        "{form}"
    );
    let assertion = form.split("assertion=").nth(1).unwrap();
    assert_eq!(assertion.matches('.').count(), 2, "a signed JWS");

    // The whole secret comes back, key material intact, token replaced.
    assert_eq!(update.secret["access_token"], "ya29.fresh");
    assert_eq!(update.secret["private_key"], TEST_PRIVATE_KEY);
    assert_eq!(update.secret["project_id"], "demo-project");
    let expires = update
        .expires_at_ms
        .expect("an expiry the host refreshes by");
    assert_eq!(update.secret["expires_at_ms"], json!(expires));
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    assert!(
        (now_ms + 3_500_000..now_ms + 3_601_000).contains(&expires),
        "expected about an hour ahead, got {expires} at {now_ms}"
    );

    // And the minted token is what `prepare` then presents.
    let prepared = prepare(
        &config,
        None,
        &update.secret,
        Operation::GenerateContent,
        Dialect::Gemini,
        request(
            "/v1beta/models/gemini-2.5-pro:generateContent",
            None,
            json!({}),
        ),
        None,
    )
    .unwrap();
    assert_eq!(prepared.headers()["authorization"], "Bearer ya29.fresh");
}

#[tokio::test]
async fn a_revoked_key_is_a_definitive_rejection_and_an_outage_is_not() {
    let secret = json!({
        "client_email": "robot@example.iam.gserviceaccount.com",
        "private_key": TEST_PRIVATE_KEY,
        "project_id": "demo-project",
        "token_uri": "https://token.test/token",
    });
    let config = json!({});
    let refresh = Vertex.credential_refresh().expect("vertex refreshes");

    // A deleted or disabled service account answers invalid_grant.
    let revoked = ScriptClient::new(vec![reply(
        StatusCode::BAD_REQUEST,
        json!({"error": "invalid_grant", "error_description": "Invalid JWT Signature."}),
    )]);
    let error = refusal(
        refresh
            .refresh(CredentialContext {
                provider: provider(&config, None),
                credential: credential(&secret),
                client: &revoked,
            })
            .await,
    );
    assert!(
        matches!(&error, ChannelError::RefreshRejected(reason) if reason == "invalid_grant"),
        "{error}"
    );

    // The token endpoint having a bad minute must stay retryable.
    let outage = ScriptClient::new(vec![reply(
        StatusCode::SERVICE_UNAVAILABLE,
        json!({"error": "backendError"}),
    )]);
    let error = refusal(
        refresh
            .refresh(CredentialContext {
                provider: provider(&config, None),
                credential: credential(&secret),
                client: &outage,
            })
            .await,
    );
    assert!(
        matches!(
            error,
            ChannelError::UpstreamResponse {
                status: StatusCode::SERVICE_UNAVAILABLE,
                ..
            }
        ),
        "a 503 is not a definitive refusal"
    );
}

#[tokio::test]
async fn a_key_that_cannot_sign_is_rejected_without_a_round_trip() {
    let secret = json!({
        "client_email": "robot@example.iam.gserviceaccount.com",
        "private_key": "-----BEGIN PRIVATE KEY-----\nnot a key\n-----END PRIVATE KEY-----",
        "project_id": "demo-project",
    });
    let config = json!({});
    let client = ScriptClient::new(Vec::new());
    let error = refusal(
        Vertex
            .credential_refresh()
            .expect("vertex refreshes")
            .refresh(CredentialContext {
                provider: provider(&config, None),
                credential: credential(&secret),
                client: &client,
            })
            .await,
    );
    assert!(
        matches!(&error, ChannelError::RefreshRejected(reason) if reason.contains("private_key")),
        "{error}"
    );
    assert!(client.sent().is_empty(), "nothing was sent");
}

#[test]
fn the_descriptor_names_the_keys_the_channel_reads() {
    let descriptor = Vertex.descriptor();
    assert_eq!(descriptor.id, "vertex");
    for key in ["project", "location", "base_url"] {
        assert!(descriptor.config_key(key).is_some(), "missing {key}");
    }
    assert!(descriptor.capabilities.refresh);
    // A service-account key is pasted, not logged in with.
    assert!(descriptor.login_modes.is_empty());
    // Vertex fingerprints nothing, so the operator's profile is the only input.
    assert!(Vertex.default_connection().is_none());
}
