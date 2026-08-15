use super::*;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
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

#[test]
fn prepares_and_reshapes_video_requests() {
    let secret = json!({ "api_key": "xai-test" });
    let settings = json!({});
    let headers = HeaderMap::new();
    let op = crate::protocol::OperationKey::provider(
        crate::protocol::Operation::CreateVideo,
        crate::protocol::Provider::OpenAi,
    );
    let request = XaiChannel
        .prepare(PrepareCtx {
            secret: &secret,
            provider_settings: &settings,
            op,
            stream: false,
            upstream_model_id: "grok-imagine-video",
            method: Method::POST,
            path: "/v1/videos",
            query: None,
            headers: &headers,
            body: Bytes::from_static(br#"{"model":"grok-imagine-video","prompt":"cat"}"#),
        })
        .unwrap()
        .into_http()
        .unwrap();
    assert_eq!(request.uri(), "https://api.x.ai/v1/videos/generations");
    assert_eq!(request.headers()["authorization"], "Bearer xai-test");

    let ctx = ShapeCtx {
        op,
        stream: false,
        status: StatusCode::OK,
        settings: &settings,
    };
    let response =
        XaiChannel.shape_response(Bytes::from_static(br#"{"request_id":"req_1"}"#), &ctx);
    let response: serde_json::Value = serde_json::from_slice(&response).unwrap();
    assert_eq!(response["id"], "req_1");
    assert_eq!(response["status"], "queued");
}

#[test]
fn maps_openai_seconds_to_xai_duration() {
    let settings = json!({});
    let ctx = ShapeCtx {
        op: crate::protocol::OperationKey::provider(
            crate::protocol::Operation::CreateVideo,
            crate::protocol::Provider::OpenAi,
        ),
        stream: false,
        status: StatusCode::OK,
        settings: &settings,
    };
    let mut headers = HeaderMap::new();
    let request = XaiChannel.shape_request(
        Bytes::from_static(br#"{"model":"grok-imagine-video","prompt":"cat","seconds":"8"}"#),
        &mut headers,
        &ctx,
    );
    let request: serde_json::Value = serde_json::from_slice(&request).unwrap();
    assert_eq!(request["duration"], 8);
    assert!(request.get("seconds").is_none());
}

#[test]
fn wraps_uploaded_video_as_xai_url_object() {
    let settings = json!({});
    let ctx = ShapeCtx {
        op: crate::protocol::OperationKey::provider(
            crate::protocol::Operation::EditVideo,
            crate::protocol::Provider::OpenAi,
        ),
        stream: false,
        status: StatusCode::OK,
        settings: &settings,
    };
    let mut headers = HeaderMap::new();
    let request = XaiChannel.shape_request(
        Bytes::from_static(br#"{"prompt":"make it rainy","video":"data:video/mp4;base64,AAAA"}"#),
        &mut headers,
        &ctx,
    );
    let request: serde_json::Value = serde_json::from_slice(&request).unwrap();
    assert_eq!(request["video"]["url"], "data:video/mp4;base64,AAAA");

    let response = XaiChannel.shape_response(
        Bytes::from_static(
            br#"{"request_id":"req_2","status":"done","video":{"url":"https://cdn/video.mp4"}}"#,
        ),
        &ctx,
    );
    let response: serde_json::Value = serde_json::from_slice(&response).unwrap();
    assert_eq!(response["id"], "req_2");
    assert_eq!(response["status"], "completed");
    assert_eq!(response["url"], "https://cdn/video.mp4");
}
