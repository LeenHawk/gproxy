use super::*;
use http::Method;
use serde_json::json;

fn prepare(path: &str) -> http::Request<Bytes> {
    let secret = json!({ "api_key": "sk-deepseek" });
    let settings = json!({});
    let headers = HeaderMap::new();
    DeepSeekChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op: crate::protocol::OperationKey::content_generation(
                crate::protocol::Operation::GenerateContent,
                crate::protocol::ContentGenerationKind::OpenAiChatCompletions,
            ),
            stream: false,
            upstream_model_id: "deepseek-chat",
            method: Method::POST,
            path,
            query: None,
            headers: &headers,
            body: Bytes::from_static(b"{}"),
        })
        .unwrap()
        .into_http()
        .unwrap()
}

#[test]
fn claude_messages_path_rehomed_with_x_api_key() {
    let req = prepare("/v1/messages");
    assert_eq!(
        req.uri().to_string(),
        "https://api.deepseek.com/anthropic/v1/messages"
    );
    assert_eq!(req.headers().get("x-api-key").unwrap(), "sk-deepseek");
    assert!(req.headers().get("authorization").is_none());
}

#[test]
fn openai_chat_path_uses_bearer() {
    let req = prepare("/v1/chat/completions");
    assert_eq!(
        req.uri().to_string(),
        "https://api.deepseek.com/v1/chat/completions"
    );
    assert_eq!(
        req.headers().get("authorization").unwrap(),
        "Bearer sk-deepseek"
    );
    assert!(req.headers().get("x-api-key").is_none());
}
