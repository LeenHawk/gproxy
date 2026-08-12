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

#[test]
fn enriches_grok_46_model_catalogue() {
    let body =
        Bytes::from_static(br#"{"object":"list","data":[{"id":"grok-4.6"},{"id":"other"}]}"#);
    let value: serde_json::Value = serde_json::from_slice(&super::enrich_model_list(body)).unwrap();
    assert_eq!(value["data"][0]["display_name"], "Grok 4.6");
    assert_eq!(value["data"][0]["context_length"], 500_000);
    assert_eq!(value["data"][0]["thinking_supported"], true);
    assert!(value["data"][1].get("context_length").is_none());
}
