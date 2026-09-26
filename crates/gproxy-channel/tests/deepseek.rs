#![cfg(feature = "deepseek")]

//! DeepSeek puts three compatible surfaces on one origin at three different
//! paths, and names its cache hits its own way.

use gproxy_channel::channel::{
    PrepareContext, QuotaValue, ResponseView, UsageContext, UsageExtractor,
};
use gproxy_channel::channels::deepseek::{BALANCE_DIMENSION, DeepSeek};
use gproxy_channel::{BaseChannel, ChannelError, LoginMode};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest};
use http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

mod support;
use support::{credential, provider, quota, request};

fn prepare(
    config: &Value,
    base_url: Option<&str>,
    operation: Operation,
    dialect: Dialect,
    request: WireRequest<HttpBody>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    let secret = json!({"api_key": "sk-deepseek"});
    DeepSeek.prepare(PrepareContext {
        provider: provider("deepseek", config, base_url),
        credential: credential("api_key", &secret, &Value::Null),
        operation: OperationKey { operation, dialect },
        request,
        endpoint_override: None,
    })
}

#[test]
fn each_dialect_reaches_its_own_surface_with_the_auth_that_surface_wants() {
    let chat = prepare(
        &json!({}),
        None,
        Operation::GenerateContent,
        Dialect::OpenAiChat,
        request("/v1/chat/completions", None),
    )
    .unwrap();
    assert_eq!(chat.uri(), "https://api.deepseek.com/v1/chat/completions");
    assert_eq!(chat.headers()["authorization"], "Bearer sk-deepseek");

    let responses = prepare(
        &json!({}),
        None,
        Operation::StreamGenerateContent,
        Dialect::OpenAi,
        request("/v1/responses", None),
    )
    .unwrap();
    assert_eq!(
        responses.uri(),
        "https://api.deepseek.com/responses",
        "Responses sits at the origin root"
    );

    let claude = prepare(
        &json!({}),
        None,
        Operation::GenerateContent,
        Dialect::Claude,
        request("/v1/messages", None),
    )
    .unwrap();
    assert_eq!(
        claude.uri(),
        "https://api.deepseek.com/anthropic/v1/messages"
    );
    assert_eq!(claude.headers()["x-api-key"], "sk-deepseek");
    assert!(claude.headers().get("authorization").is_none());
    assert!(
        claude.headers().get("anthropic-version").is_none(),
        "DeepSeek asks for none, unlike Anthropic itself"
    );
}

#[test]
fn only_conversational_operations_move_between_surfaces() {
    let models = prepare(
        &json!({}),
        None,
        Operation::ListModels,
        Dialect::OpenAi,
        request("/v1/models", None),
    )
    .unwrap();
    assert_eq!(
        models.uri(),
        "https://api.deepseek.com/v1/models",
        "a catalogue is not a conversation"
    );

    let count = prepare(
        &json!({}),
        None,
        Operation::CountTokens,
        Dialect::Claude,
        request("/v1/messages/count_tokens", None),
    )
    .unwrap();
    assert_eq!(
        count.uri(),
        "https://api.deepseek.com/anthropic/v1/messages/count_tokens"
    );
}

#[test]
fn a_cache_hit_is_taken_out_of_the_prompt_count() {
    let body = json!({"usage": {
        "prompt_tokens": 1000, "completion_tokens": 20,
        "prompt_cache_hit_tokens": 960, "prompt_cache_miss_tokens": 40,
    }})
    .to_string();
    let headers = HeaderMap::new();
    let usage = DeepSeek
        .extract(UsageContext {
            operation: OperationKey {
                operation: Operation::GenerateContent,
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
        .unwrap();
    assert_eq!(
        usage.tokens.input_tokens,
        Some(40),
        "prompt_tokens counts hits and misses together"
    );
    assert_eq!(usage.tokens.cached_input_tokens, Some(960));
    assert_eq!(usage.tokens.output_tokens, Some(20));
}

#[tokio::test]
async fn the_balance_probe_reports_every_currency_the_account_holds() {
    let (url, snapshot) = quota(
        &DeepSeek,
        "deepseek",
        &json!({"api_key": "sk-deepseek"}),
        None,
        StatusCode::OK,
        json!({"is_available": true, "balance_infos": [
            {"currency": "CNY", "total_balance": "110.00", "granted_balance": "10.00"},
            {"currency": "USD", "total_balance": "15.50"},
        ]})
        .to_string(),
    )
    .await
    .unwrap();
    assert_eq!(url, "https://api.deepseek.com/user/balance");
    assert_eq!(snapshot.entries.len(), 2);
    assert_eq!(snapshot.entries[0].id, BALANCE_DIMENSION);
    assert_eq!(
        snapshot.entries[1].id,
        format!("{BALANCE_DIMENSION}_usd"),
        "further currencies get ids of their own"
    );
    let QuotaValue::Balance(balance) = &snapshot.entries[0].value else {
        panic!("a balance");
    };
    assert_eq!(balance.remaining, Some("110.00".parse().unwrap()));
    assert_eq!(balance.unit.as_deref(), Some("CNY"));
    use gproxy_channel::channel::QuotaModel;
    let config = json!({});
    let secret = json!({"api_key": "sk-deepseek"});
    let declared = DeepSeek.dimensions(
        provider("deepseek", &config, None),
        credential("api_key", &secret, &Value::Null),
    );
    // Only the first balance is the declared one; further currencies are
    // observe-only.
    support::assert_quota_contract(
        Some(&DeepSeek),
        &declared,
        &snapshot.entries,
        &["deepseek_balance_*"],
    );
}

#[test]
fn the_descriptor_declares_three_native_shapes_and_an_api_key() {
    let descriptor = DeepSeek.descriptor();
    assert_eq!(descriptor.id, "deepseek");
    assert_eq!(descriptor.login_modes, [LoginMode::ApiKey]);
    assert!(descriptor.capabilities.quota_query);
    assert!(!descriptor.capabilities.refresh);
    assert_eq!(
        DeepSeek.native_dialects(
            provider("deepseek", &json!({}), None),
            Operation::GenerateContent
        ),
        [Dialect::OpenAiChat, Dialect::OpenAi, Dialect::Claude]
    );
    let secret = json!({});
    assert!(matches!(
        DeepSeek.prepare(PrepareContext {
            provider: provider("deepseek", &json!({}), None),
            credential: credential("api_key", &secret, &Value::Null),
            operation: OperationKey {
                operation: Operation::GenerateContent,
                dialect: Dialect::OpenAiChat,
            },
            request: request("/v1/chat/completions", None),
            endpoint_override: None,
        }),
        Err(ChannelError::InvalidCredential)
    ));
}
