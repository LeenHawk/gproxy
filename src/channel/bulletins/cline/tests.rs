//! Covers Cline's credential families, full model catalogue, and account usage
//! envelope.

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use serde_json::json;

use super::*;
use crate::protocol::ContentGenerationKind;

fn prepared(
    secret: &serde_json::Value,
    op: OperationKey,
    method: Method,
    path: &str,
) -> http::Request<Bytes> {
    let settings = json!({});
    let headers = HeaderMap::new();
    ClineChannel
        .prepare(PrepareCtx {
            secret,
            provider_settings: &settings,
            op,
            stream: false,
            upstream_model_id: "anthropic/claude-sonnet-4.6",
            method,
            path,
            query: None,
            headers: &headers,
            body: Bytes::from_static(b"{}"),
        })
        .unwrap()
        .into_http()
        .unwrap()
}

fn chat_op() -> OperationKey {
    OperationKey::content_generation(
        Operation::GenerateContent,
        ContentGenerationKind::OpenAiChatCompletions,
    )
}

#[test]
fn credentials_use_api_key_family_even_with_device_login() {
    let metadata = crate::channel::metadata::builtin(&ClineChannel);
    assert_eq!(
        metadata.credential_family,
        crate::channel::CredentialFamily::ApiKey
    );
    assert_eq!(
        metadata.login_modes,
        vec![crate::channel::LoginMode::Device]
    );
    assert_eq!(metadata.secret_template, json!({ "api_key": "" }));
}

#[test]
fn account_tokens_are_prefixed_and_api_keys_are_not() {
    let token = prepared(
        &json!({ "access_token": "jwt-abc", "refresh_token": "rt" }),
        chat_op(),
        Method::POST,
        "/v1/chat/completions",
    );
    assert_eq!(
        token.headers().get("authorization").unwrap(),
        "Bearer workos:jwt-abc"
    );
    assert_eq!(
        token.uri().to_string(),
        "https://api.cline.bot/api/v1/chat/completions"
    );
    assert_eq!(token.headers().get("x-title").unwrap(), "Cline");

    // A pasted workspace key is a different token family — sent verbatim.
    let key = prepared(
        &json!({ "api_key": "cline-key" }),
        chat_op(),
        Method::POST,
        "/v1/chat/completions",
    );
    assert_eq!(
        key.headers().get("authorization").unwrap(),
        "Bearer cline-key"
    );

    // An already-prefixed token must not be double-prefixed.
    let stored = prepared(
        &json!({ "access_token": "workos:jwt-abc" }),
        chat_op(),
        Method::POST,
        "/v1/chat/completions",
    );
    assert_eq!(
        stored.headers().get("authorization").unwrap(),
        "Bearer workos:jwt-abc"
    );
}

#[test]
fn usage_unwraps_the_envelope_and_needs_a_user_id() {
    let settings = json!({});
    // A pasted API key never learned the account id, so there is nothing to ask.
    assert!(
        usage::request(&json!({ "api_key": "k" }), &settings)
            .unwrap()
            .is_none()
    );

    let req = usage::request(&json!({ "api_key": "k", "user_id": "usr_1" }), &settings)
        .unwrap()
        .expect("user_id present");
    assert_eq!(
        req.uri().to_string(),
        "https://api.cline.bot/api/v1/users/usr_1/balance"
    );

    let snapshot = usage::parse(
        StatusCode::OK,
        &Bytes::from_static(br#"{"success":true,"data":{"balance":12.5,"userId":"usr_1"}}"#),
    )
    .expect("snapshot");
    let credits = snapshot.credits.expect("credits");
    assert_eq!(credits.balance.as_deref(), Some("12.5"));
    assert_eq!(credits.has_credits, Some(true));
}

#[test]
fn only_account_credentials_refresh() {
    // No refresh token: a pasted key must never be treated as refreshable.
    assert!(!ClineChannel.needs_refresh(&json!({ "api_key": "k" })));
    // Expired JWT (`exp` in 2021) with a refresh token.
    let expired = format!(
        "h.{}.s",
        base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            br#"{"exp":1609459200}"#,
        )
    );
    assert!(ClineChannel.needs_refresh(&json!({
        "access_token": expired,
        "refresh_token": "rt",
    })));
}

#[test]
fn channel_unwraps_buffered_chat_envelope() {
    let body = Bytes::from_static(
        br#"{"success":true,"data":{"id":"gen_1","object":"chat.completion","choices":[]}}"#,
    );
    let shaped = ClineChannel.shape_response(
        body,
        &crate::channel::ShapeCtx {
            op: chat_op(),
            stream: false,
            status: StatusCode::OK,
            settings: &json!({}),
        },
    );
    let value: serde_json::Value = serde_json::from_slice(&shaped).unwrap();
    assert_eq!(value["id"], "gen_1");
    assert!(value.get("data").is_none());
}
