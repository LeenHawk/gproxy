//! Representative tests across the channel auth styles.

use bytes::Bytes;
use http::{HeaderMap, Method};
use serde_json::Value;
use serde_json::json;

use super::{aistudio, claudeapi, codex, custom, openai};
use crate::channel::{Channel, ChannelError, PrepareCtx};
use crate::protocol::{ContentGenerationKind, Operation, OperationKey};

fn prep<'a>(
    settings: &'a Value,
    secret: &'a Value,
    headers: &'a HeaderMap,
    method: Method,
    path: &'a str,
) -> PrepareCtx<'a> {
    let kind = if path.contains("chat/completions") {
        ContentGenerationKind::OpenAiChatCompletions
    } else if path.contains("responses") {
        ContentGenerationKind::OpenAiResponses
    } else if path.contains("messages") {
        ContentGenerationKind::ClaudeMessages
    } else {
        ContentGenerationKind::GeminiGenerateContent
    };
    PrepareCtx {
        secret,
        provider_settings: settings,
        op: OperationKey::content_generation(Operation::GenerateContent, kind),
        stream: false,
        upstream_model_id: "m",
        method,
        path,
        query: None,
        headers,
        body: Bytes::from_static(b"{}"),
    }
}

#[test]
fn openai_bearer_and_default_endpoint() {
    let settings = json!({});
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
        .into_http();
    assert_eq!(
        req.uri().to_string(),
        "https://api.openai.com/v1/chat/completions"
    );
    assert_eq!(req.headers().get("authorization").unwrap(), "Bearer sk-x");
}

#[test]
fn exact_endpoint_overrides_default_without_appending_path() {
    let settings = json!({
        "base_url": "https://fallback.example",
        "endpoints": {
            "openai_chat_completions": "https://api-gateway.merge.dev/v1/openai"
        }
    });
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
        .into_http();
    assert_eq!(
        req.uri().to_string(),
        "https://api-gateway.merge.dev/v1/openai"
    );
}

#[test]
fn settings_base_url_is_used_without_endpoint_override() {
    let settings = json!({ "base_url": "http://127.0.0.1:9009" });
    let secret = json!({ "api_key": "sk-x" });
    let headers = HeaderMap::new();
    let req = openai::OpenAiChannel
        .prepare(prep(
            &settings,
            &secret,
            &headers,
            Method::POST,
            "/v1/chat/completions",
        ))
        .unwrap()
        .into_http();
    assert_eq!(
        req.uri().to_string(),
        "http://127.0.0.1:9009/v1/chat/completions"
    );
}

#[test]
fn claudeapi_dual_header_no_bearer() {
    let settings = json!({});
    let secret = json!({ "api_key": "ak" });
    let h = HeaderMap::new();
    let req = claudeapi::ClaudeApiChannel
        .prepare(prep(&settings, &secret, &h, Method::POST, "/v1/messages"))
        .unwrap()
        .into_http();
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
        .into_http();
    assert_eq!(req.uri().query(), Some("key=gk"));
    assert!(req.headers().get("authorization").is_none());
}

#[test]
fn custom_protocol_driven_auth() {
    let settings = json!({
        "endpoints": {
            "claude_messages": "https://up.example/claude",
            "openai_chat_completions": "https://up.example/chat",
            "gemini_generate_content": "https://up.example/gemini"
        }
    });
    let secret = json!({ "api_key": "k" });
    let h = HeaderMap::new();

    let claude = custom::CustomChannel
        .prepare(prep(&settings, &secret, &h, Method::POST, "/v1/messages"))
        .unwrap()
        .into_http();
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
        .into_http();
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
        .into_http();
    assert_eq!(gemini.headers().get("x-goog-api-key").unwrap(), "k");
}

#[test]
fn custom_requires_base_or_matching_endpoint() {
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
