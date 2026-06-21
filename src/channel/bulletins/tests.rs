//! Representative tests across the channel auth styles.

use bytes::Bytes;
use http::{HeaderMap, Method};
use serde_json::Value;
use serde_json::json;

use super::{aistudio, claudeapi, codex, custom, openai};
use crate::channel::{Channel, ChannelError, PrepareCtx};

fn prep<'a>(
    settings: &'a Value,
    secret: &'a Value,
    headers: &'a HeaderMap,
    method: Method,
    path: &'a str,
) -> PrepareCtx<'a> {
    PrepareCtx {
        secret,
        provider_settings: settings,
        upstream_model_id: "m",
        method,
        path,
        query: None,
        headers,
        body: Bytes::from_static(b"{}"),
    }
}

#[test]
fn openai_bearer_and_default_base_url() {
    let settings = json!({}); // no base_url → baked default
    let secret = json!({ "api_key": "sk-x" });
    let h = HeaderMap::new();
    let req = openai::OpenAiChannel
        .prepare(prep(
            &settings,
            &secret,
            &h,
            Method::POST,
            "/v1/chat/completions",
        ))
        .unwrap()
        .request;
    assert_eq!(
        req.uri().to_string(),
        "https://api.openai.com/v1/chat/completions"
    );
    assert_eq!(req.headers().get("authorization").unwrap(), "Bearer sk-x");
}

#[test]
fn settings_base_url_overrides_default() {
    let settings = json!({ "base_url": "http://127.0.0.1:9009" });
    let secret = json!({ "api_key": "sk-x" });
    let h = HeaderMap::new();
    let req = openai::OpenAiChannel
        .prepare(prep(
            &settings,
            &secret,
            &h,
            Method::POST,
            "/v1/chat/completions",
        ))
        .unwrap()
        .request;
    assert_eq!(req.uri().host(), Some("127.0.0.1"));
}

#[test]
fn claudeapi_dual_header_no_bearer() {
    let settings = json!({});
    let secret = json!({ "api_key": "ak" });
    let h = HeaderMap::new();
    let req = claudeapi::ClaudeApiChannel
        .prepare(prep(&settings, &secret, &h, Method::POST, "/v1/messages"))
        .unwrap()
        .request;
    assert_eq!(req.headers().get("x-api-key").unwrap(), "ak");
    assert_eq!(
        req.headers().get("anthropic-version").unwrap(),
        "2023-06-01"
    );
    assert!(req.headers().get("authorization").is_none());
}

#[test]
fn aistudio_key_in_query() {
    let settings = json!({});
    let secret = json!({ "api_key": "gk" });
    let h = HeaderMap::new();
    let req = aistudio::AiStudioChannel
        .prepare(prep(
            &settings,
            &secret,
            &h,
            Method::POST,
            "/v1beta/models/gemini:generateContent",
        ))
        .unwrap()
        .request;
    assert_eq!(req.uri().query(), Some("key=gk"));
    assert!(req.headers().get("authorization").is_none());
}

#[test]
fn custom_protocol_driven_auth() {
    let settings = json!({ "base_url": "https://up.example" });
    let secret = json!({ "api_key": "k" });
    let h = HeaderMap::new();

    let claude = custom::CustomChannel
        .prepare(prep(&settings, &secret, &h, Method::POST, "/v1/messages"))
        .unwrap()
        .request;
    assert_eq!(claude.headers().get("x-api-key").unwrap(), "k");

    let oai = custom::CustomChannel
        .prepare(prep(
            &settings,
            &secret,
            &h,
            Method::POST,
            "/v1/chat/completions",
        ))
        .unwrap()
        .request;
    assert_eq!(oai.headers().get("authorization").unwrap(), "Bearer k");

    let gemini = custom::CustomChannel
        .prepare(prep(
            &settings,
            &secret,
            &h,
            Method::POST,
            "/v1beta/models/g:generateContent",
        ))
        .unwrap()
        .request;
    assert_eq!(gemini.headers().get("x-goog-api-key").unwrap(), "k");
}

#[test]
fn custom_requires_base_url() {
    let settings = json!({});
    let secret = json!({ "api_key": "k" });
    let h = HeaderMap::new();
    let err = custom::CustomChannel
        .prepare(prep(
            &settings,
            &secret,
            &h,
            Method::POST,
            "/v1/chat/completions",
        ))
        .unwrap_err();
    assert!(matches!(err, ChannelError::MissingSetting("base_url")));
}

#[test]
fn codex_rejects_credential_without_token() {
    // Codex is OAuth-backed: an empty secret has no access_token, so `prepare`
    // fails the credential rather than building a request.
    let settings = json!({});
    let secret = json!({});
    let h = HeaderMap::new();
    let err = codex::CodexChannel
        .prepare(prep(&settings, &secret, &h, Method::POST, "/v1/responses"))
        .unwrap_err();
    assert!(matches!(err, ChannelError::InvalidCredential(_)));
}
