use base64::Engine as _;
use bytes::Bytes;
use gproxy_channel_api::{Channel, PrepareCtx};
use gproxy_protocol::{Operation, OperationKey, WireFamily};
use http::{HeaderMap, Method};
use serde_json::json;

#[test]
fn model_discovery_uses_upstream_cli_identity() {
    let settings = json!({});
    let secret = json!({"access_token":"token"});
    let headers = HeaderMap::new();
    let body = Bytes::new();
    let prepared = super::super::CodexChannel
        .prepare(PrepareCtx {
            key: OperationKey::family(Operation::ListModels, WireFamily::OpenAi),
            stream: false,
            method: &Method::GET,
            path: "/v1/models",
            query: None,
            headers: &headers,
            body: &body,
            upstream_model: "",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();

    assert_eq!(
        prepared.request.uri(),
        "https://chatgpt.com/backend-api/codex/models?client_version=0.153.0"
    );
    assert_eq!(prepared.request.headers()["version"], "0.153.0");
    assert!(
        prepared.request.headers()[http::header::USER_AGENT]
            .to_str()
            .unwrap()
            .starts_with("codex_cli_rs/0.153.0 ")
    );
}

#[test]
fn login_secret_retains_jwt_account_identity() {
    let claims = json!({
        "email": "user@example.com",
        "https://api.openai.com/auth": {
            "chatgpt_account_id": "acct-1",
            "chatgpt_account_is_fedramp": true
        }
    });
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims.to_string());
    let secret = super::super::auth::login_secret(&json!({
        "access_token": "access",
        "refresh_token": "refresh",
        "id_token": format!("header.{payload}.signature")
    }))
    .unwrap();

    assert_eq!(secret["user_email"], "user@example.com");
    assert_eq!(secret["account_id"], "acct-1");
    assert_eq!(secret["chatgpt_account_is_fedramp"], true);
}
