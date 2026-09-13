use gproxy_protocol::gemini::*;
use gproxy_protocol::{WireRequest, WireResponse};

#[test]
fn count_tokens_contents_body_round_trip_and_http_envelope() {
    let json = r#"{"contents":[{"role":"user","parts":[{"text":"describe these"},{"inlineData":{"mimeType":"image/jpeg","data":"AQI="}},{"inlineData":{"mimeType":"audio/wav","data":"AwQ="}},{"fileData":{"fileUri":"gs://bucket/video.mp4","mimeType":"video/mp4"}},{"functionCall":{"name":"lookup","args":{"q":"x"}}}]}],"futureField":{"preserve":true}}"#;
    let body: CountTokensRequestBody = serde_json::from_str(json).unwrap();
    assert_eq!(
        serde_json::to_value(&body).unwrap(),
        serde_json::from_str::<serde_json::Value>(json).unwrap()
    );
    let mut headers = http::HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    let request: CountTokensRequest = WireRequest {
        method: http::Method::POST,
        path: "/v1beta/models/gemini-2.0-flash:countTokens".into(),
        query: Some("key=test".into()),
        headers,
        body,
    };
    assert_eq!(request.method, http::Method::POST);
    assert_eq!(request.path, "/v1beta/models/gemini-2.0-flash:countTokens");
    assert_eq!(request.query.as_deref(), Some("key=test"));
    assert_eq!(request.body.contents.as_ref().unwrap().len(), 1);
}

#[test]
fn count_tokens_embedded_generate_request_and_response_round_trip() {
    let json = r#"{"generateContentRequest":{"model":"models/gemini-2.0-flash","contents":[{"parts":[{"text":"tool prompt"}]}],"tools":[{"functionDeclarations":[{"name":"lookup","description":"lookup","parameters":{"type":"OBJECT","properties":{"q":{"type":"STRING"}}}}]}]}}"#;
    let body: CountTokensRequestBody = serde_json::from_str(json).unwrap();
    assert_eq!(
        serde_json::to_value(&body).unwrap(),
        serde_json::from_str::<serde_json::Value>(json).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&body).unwrap()["generateContentRequest"]["model"],
        "models/gemini-2.0-flash"
    );
    let response_json = r#"{"totalTokens":12,"promptTokensDetails":[{"modality":"TEXT","tokenCount":10},{"modality":"IMAGE","tokenCount":2}],"cacheTokensDetails":[{"modality":"VIDEO","tokenCount":7}],"unknown":1}"#;
    let response_body: CountTokensResponseBody = serde_json::from_str(response_json).unwrap();
    assert_eq!(
        serde_json::to_value(&response_body).unwrap(),
        serde_json::from_str::<serde_json::Value>(response_json).unwrap()
    );
    let mut headers = http::HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    let response: CountTokensResponse = WireResponse {
        status: http::StatusCode::OK,
        headers,
        body: response_body,
    };
    assert_eq!(response.status, http::StatusCode::OK);
    assert_eq!(response.body.total_tokens, 12);
}

#[test]
fn count_tokens_optional_fields_omit_when_absent() {
    let body: CountTokensResponseBody = serde_json::from_str(r#"{"totalTokens":1}"#).unwrap();
    assert_eq!(
        serde_json::to_string(&body).unwrap(),
        r#"{"totalTokens":1}"#
    );
}
