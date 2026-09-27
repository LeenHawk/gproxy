#![cfg(feature = "vercel")]
mod support;
use gproxy_channel::{
    BaseChannel, ChannelError,
    channel::{CredentialContext, PrepareContext, QuotaValue},
    channels::vercel::{BALANCE_DIMENSION, DEFAULT_BASE_URL, Vercel},
};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, connection::Bytes};
use http::StatusCode;
use serde_json::{Value, json};
use support::{OneShot, credential, provider, request};

fn prepare(
    operation: Operation,
    dialect: Dialect,
    body: Value,
    config: Value,
    base: Option<&str>,
    endpoint: Option<&str>,
) -> http::Request<HttpBody> {
    let path = match (operation, dialect) {
        (Operation::ListModels, _) => "/v1/models",
        (Operation::GetModel, _) => "/v1/models/openai%2Fgpt%205",
        (Operation::CountTokens, _) => "/v1/messages/count_tokens",
        (Operation::CreateEmbedding, _) => "/v1/embeddings",
        (_, Dialect::Claude) => "/v1/messages",
        (_, Dialect::OpenAiChat) => "/v1/chat/completions",
        _ => "/v1/responses",
    };
    let mut wire = request(path, Some("key=client-secret&test=1"));
    wire.body = HttpBody::Bytes(Bytes::from(body.to_string()));
    Vercel
        .prepare(PrepareContext {
            provider: provider("vercel", &config, base),
            credential: credential("api_key", &json!({"api_key":"vercel-key"}), &Value::Null),
            operation: OperationKey { operation, dialect },
            request: wire,
            endpoint_override: endpoint,
        })
        .unwrap()
}
fn body(request: http::Request<HttpBody>) -> Value {
    let HttpBody::Bytes(bytes) = request.into_body() else {
        panic!("buffered")
    };
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn registers_the_v3_native_surfaces_and_gateway_fallback() {
    assert!(
        gproxy_channel::channels::compiled_in()
            .iter()
            .any(|c| c.id() == "vercel")
    );
    let p = provider("vercel", &Value::Null, None);
    for operation in [Operation::GenerateContent, Operation::StreamGenerateContent] {
        assert_eq!(
            Vercel.native_dialects(p, operation),
            vec![Dialect::OpenAi, Dialect::OpenAiChat, Dialect::Claude]
        );
    }
    assert_eq!(
        Vercel.native_dialects(p, Operation::CountTokens),
        vec![Dialect::Claude]
    );
    assert_eq!(
        Vercel.native_dialects(p, Operation::CreateEmbedding),
        vec![Dialect::OpenAi]
    );
    assert!(Vercel.native_dialects(p, Operation::CreateVideo).is_empty());
    assert_eq!(
        Vercel
            .default_conversion_target(
                p,
                OperationKey {
                    operation: Operation::GenerateContent,
                    dialect: Dialect::Gemini
                }
            )
            .unwrap()
            .dialect,
        Dialect::OpenAi
    );
    assert!(!Vercel.claude_fallback().unwrap().credit);
    let descriptor = Vercel.descriptor();
    for name in [
        "base_url",
        "headers",
        "fallback_mode",
        "fallback_models",
        "enable_claude_magic_cache",
        "enable_openai_magic_cache",
    ] {
        assert!(
            descriptor.config_keys.iter().any(|key| key.name == name),
            "{name}"
        );
    }
    assert!(descriptor.capabilities.quota_query);
}

#[test]
fn default_origin_exact_override_and_dual_auth_match_v3() {
    for dialect in [Dialect::Claude, Dialect::OpenAiChat, Dialect::OpenAi] {
        let prepared = prepare(
            Operation::GenerateContent,
            dialect,
            json!({"model":"anthropic/claude-fable-5","messages":[]}),
            json!({}),
            None,
            None,
        );
        assert!(prepared.uri().to_string().starts_with(DEFAULT_BASE_URL));
        assert!(prepared.uri().to_string().ends_with("?test=1"));
        assert_eq!(prepared.headers()["authorization"], "Bearer vercel-key");
        assert_eq!(prepared.headers()["x-api-key"], "vercel-key");
        assert!(!prepared.headers().contains_key("host"));
        assert!(!prepared.headers().contains_key("content-length"));
    }
    let exact = prepare(
        Operation::GetModel,
        Dialect::OpenAi,
        json!({}),
        json!({}),
        Some("https://ignored.example"),
        Some("https://override.example/models/{model}?region=1"),
    );
    assert_eq!(
        exact.uri(),
        "https://override.example/models/openai%2Fgpt%205?region=1&test=1"
    );
    let custom = prepare(
        Operation::CreateEmbedding,
        Dialect::OpenAi,
        json!({"model":"openai/text-embedding-3-small","input":"hello"}),
        json!({}),
        Some("https://mirror.example/"),
        None,
    );
    assert_eq!(custom.uri(), "https://mirror.example/v1/embeddings?test=1");
}

#[test]
fn claude_hygiene_and_gateway_only_fallback() {
    let mut wire = request("/v1/messages", None);
    wire.headers.insert(
        "anthropic-beta",
        "context-1m-2025-08-07,files-api".parse().unwrap(),
    );
    wire.body = HttpBody::Bytes(Bytes::from(json!({
        "model":"anthropic/claude-fable-5","temperature":0.7,"top_p":0.9,"top_k":40,
        "messages":[{"role":"assistant","content":"prefix"}],"fallbacks":[{"model":"anthropic/claude-opus-4-8"}]
    }).to_string()));
    let prepared = Vercel
        .prepare(PrepareContext {
            provider: provider(
                "vercel",
                &json!({"fallback_mode":"models","fallback_models":["anthropic/claude-opus-4-8"]}),
                None,
            ),
            credential: credential("api_key", &json!({"api_key":"key"}), &Value::Null),
            operation: OperationKey {
                operation: Operation::GenerateContent,
                dialect: Dialect::Claude,
            },
            request: wire,
            endpoint_override: None,
        })
        .unwrap();
    assert_eq!(prepared.headers()["anthropic-beta"], "files-api");
    let body = body(prepared);
    assert!(body.get("fallbacks").is_none());
    for name in ["temperature", "top_p", "top_k"] {
        assert!(body.get(name).is_none());
    }
    assert_eq!(body["messages"][0]["role"], "user");
    let chat = body_of_chat();
    assert!(
        chat.get("stream_options").is_none(),
        "core asks a Chat stream for its usage; the channel forwards the body"
    );
    assert_eq!(chat["messages"][0]["content"], "hello");
}
fn body_of_chat() -> Value {
    body(prepare(
        Operation::StreamGenerateContent,
        Dialect::OpenAiChat,
        json!({"model":"openai/gpt-5","stream":true,"messages":[{"role":"user","content":"hello"}]}),
        json!({}),
        None,
        None,
    ))
}

#[tokio::test]
async fn team_balance_uses_free_credits_endpoint() {
    let client = OneShot::new(
        StatusCode::OK,
        json!({"balance":"12.34","total_used":"2.00"}).to_string(),
    );
    let snapshot = Vercel
        .quota_query()
        .unwrap()
        .query(CredentialContext {
            provider: provider("vercel", &json!({}), Some("https://mirror.example/v1")),
            credential: credential("api_key", &json!({"api_key":"key"}), &Value::Null),
            client: &client,
        })
        .await
        .unwrap();
    assert_eq!(client.call(0).0, "https://mirror.example/v1/credits");
    assert_eq!(client.call(0).1["authorization"], "Bearer key");
    assert_eq!(snapshot.entries[0].id, BALANCE_DIMENSION);
    let QuotaValue::Balance(balance) = &snapshot.entries[0].value else {
        panic!("balance")
    };
    assert_eq!(balance.remaining.unwrap().to_string(), "12.34");
    assert_eq!(balance.unit.as_deref(), Some("USD"));
    let config = json!({});
    let secret = json!({"api_key":"key"});
    let model = Vercel.quota_model().unwrap();
    let declared = model.dimensions(
        provider("vercel", &config, None),
        credential("api_key", &secret, &Value::Null),
    );
    support::assert_quota_contract(Some(model), &declared, &snapshot.entries, &[]);
}

#[test]
fn rejects_missing_credentials_and_unsupported_operations() {
    for (secret, operation) in [
        (json!({}), Operation::GenerateContent),
        (json!({"api_key":"key"}), Operation::CreateVideo),
    ] {
        let result = Vercel.prepare(PrepareContext {
            provider: provider("vercel", &json!({}), None),
            credential: credential("api_key", &secret, &Value::Null),
            operation: OperationKey {
                operation,
                dialect: Dialect::Claude,
            },
            request: request("/v1/messages", None),
            endpoint_override: None,
        });
        assert!(matches!(
            result,
            Err(ChannelError::InvalidCredential | ChannelError::UnsupportedOperation(_))
        ));
    }
}
