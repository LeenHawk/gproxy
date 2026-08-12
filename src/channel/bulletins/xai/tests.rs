use super::*;
use bytes::Bytes;
use http::{HeaderMap, Method};
use serde_json::json;

#[test]
fn prepares_official_api_request() {
    let secret = json!({ "api_key": "xai-test" });
    let settings = json!({});
    let mut headers = HeaderMap::new();
    headers.insert("x-grok-conv-id", "conversation-1".parse().unwrap());
    let request = XaiChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op: crate::protocol::OperationKey::content_generation(
                crate::protocol::Operation::GenerateContent,
                crate::protocol::ContentGenerationKind::OpenAiResponses,
            ),
            stream: false,
            upstream_model_id: "grok-4.3",
            method: Method::POST,
            path: "/v1/responses",
            query: None,
            headers: &headers,
            body: Bytes::from_static(b"{}"),
        })
        .unwrap()
        .into_http()
        .unwrap();

    assert_eq!(request.uri(), "https://api.x.ai/v1/responses");
    assert_eq!(request.headers()["authorization"], "Bearer xai-test");
    assert_eq!(request.headers()["x-grok-conv-id"], "conversation-1");
}
