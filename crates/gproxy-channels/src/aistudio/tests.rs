use bytes::Bytes;
use gproxy_channel_api::{
    Channel, PrepareCtx, ResourceCtx, ResourceMutation, StreamCtx, StreamEnd, UsageCtx,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, StreamFraming, WireFamily};
use http::{HeaderMap, Method};
use rust_decimal::Decimal;
use serde_json::json;

use super::AiStudioChannel;

fn gemini_content(operation: Operation) -> OperationKey {
    OperationKey::content(operation, ContentGenerationKind::GeminiGenerateContent)
}

#[test]
fn prepares_auth_path_query_and_framing() {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        "application/json".parse().unwrap(),
    );
    headers.insert(
        http::header::AUTHORIZATION,
        "Bearer client".parse().unwrap(),
    );
    headers.insert("x-goog-api-key", "client-key".parse().unwrap());
    headers.insert("x-client-only", "drop".parse().unwrap());
    let secret = json!({"api_key":"upstream-key"});
    let settings = json!({"base_url":"https://example.invalid/gemini"});
    let body = Bytes::from_static(br#"{"model":"models/client-alias","contents":[]}"#);
    let request = AiStudioChannel
        .prepare(PrepareCtx {
            key: gemini_content(Operation::StreamGenerateContent),
            stream: true,
            method: &Method::POST,
            path: "/v1beta/models/client-alias:streamGenerateContent",
            query: Some("key=client&alt=sse&pageToken=next&evil=1"),
            headers: &headers,
            body: &body,
            upstream_model: "gemini-3",
            provider_settings: &settings,
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        request.request.uri().to_string(),
        "https://example.invalid/gemini/v1beta/models/gemini-3:streamGenerateContent?alt=sse&pageToken=next"
    );
    assert_eq!(request.request.headers()["x-goog-api-key"], "upstream-key");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(request.request.body()).unwrap()["model"],
        "models/gemini-3"
    );
    assert_eq!(request.request.headers().len(), 2);
    assert_eq!(request.framing, Some(StreamFraming::Sse));

    let default = AiStudioChannel
        .prepare(PrepareCtx {
            key: gemini_content(Operation::StreamGenerateContent),
            stream: true,
            method: &Method::POST,
            path: "/v1beta/models/gemini-3:streamGenerateContent",
            query: Some("key=client"),
            headers: &headers,
            body: &body,
            upstream_model: "gemini-3",
            provider_settings: &json!({}),
            secret: &secret,
        })
        .unwrap();
    assert_eq!(default.framing, Some(StreamFraming::JsonArray));
    assert!(default.request.uri().query().is_none());

    let poll = AiStudioChannel
        .prepare(PrepareCtx {
            key: OperationKey::family(Operation::RetrieveVideo, WireFamily::Gemini),
            stream: false,
            method: &Method::GET,
            path: "/v1beta/models/client-alias/operations/op-1",
            query: None,
            headers: &headers,
            body: &Bytes::new(),
            upstream_model: "veo-3",
            provider_settings: &json!({}),
            secret: &secret,
        })
        .unwrap();
    assert_eq!(
        poll.request.uri().path(),
        "/v1beta/models/veo-3/operations/op-1"
    );
}

#[test]
fn observes_sse_and_json_array_without_reframing() {
    let body = Bytes::new();
    let headers = HeaderMap::new();
    let make = |framing| {
        AiStudioChannel
            .stream_decoder(StreamCtx {
                key: gemini_content(Operation::StreamGenerateContent),
                framing,
                request_body: &body,
                response_headers: &headers,
            })
            .unwrap()
    };
    let first =
        r#"{"responseId":"r1","candidates":[{"index":0,"content":{"parts":[{"text":"hi"}]}}]}"#;
    let last = r#"{"responseId":"r1","candidates":[{"index":0,"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":10,"cachedContentTokenCount":4,"candidatesTokenCount":5,"thoughtsTokenCount":2,"totalTokenCount":17}}"#;

    let mut sse = make(StreamFraming::Sse);
    let raw = Bytes::from(format!("data: {first}\n\ndata: {last}\n\n"));
    let frames = sse.push(raw.clone()).unwrap();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].0.as_ptr(), raw.as_ptr());
    let usage = sse.finish(StreamEnd::Complete).unwrap().usage.unwrap();
    assert_eq!(usage.input_tokens, 10);
    assert_eq!(usage.output_tokens, 7);
    assert_eq!(usage.cached_input_tokens, 4);

    let mut array = make(StreamFraming::JsonArray);
    let head = Bytes::from(format!("[{first},"));
    assert_eq!(
        array.push(head.clone()).unwrap()[0].0.as_ptr(),
        head.as_ptr()
    );
    let tail = Bytes::from(format!("{last}]"));
    assert_eq!(
        array.push(tail.clone()).unwrap()[0].0.as_ptr(),
        tail.as_ptr()
    );
    assert!(array.finish(StreamEnd::Complete).unwrap().usage.is_some());

    let mut truncated = make(StreamFraming::JsonArray);
    truncated.push(Bytes::from(format!("[{first}"))).unwrap();
    assert!(truncated.finish(StreamEnd::Complete).is_err());
    assert!(truncated.finish(StreamEnd::Interrupted).is_ok());
}

#[test]
fn extracts_dimensional_gemini_usage() {
    let body = Bytes::new();
    let headers = HeaderMap::new();
    let response = br#"{
      "candidates":[{"finishReason":"STOP"}],
      "usageMetadata":{
        "promptTokenCount":100,
        "cachedContentTokenCount":40,
        "candidatesTokenCount":50,
        "thoughtsTokenCount":10,
        "totalTokenCount":160,
        "candidatesTokensDetails":[
          {"modality":"TEXT","tokenCount":30},
          {"modality":"IMAGE","tokenCount":15},
          {"modality":"AUDIO","tokenCount":5}
        ]
      }
    }"#;
    let usage = AiStudioChannel
        .extract_usage(UsageCtx {
            key: gemini_content(Operation::GenerateContent),
            request_body: &body,
            response_headers: &headers,
            response_body: response,
        })
        .unwrap();
    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.cached_input_tokens, 40);
    assert_eq!(usage.output_tokens, 40);
    assert_eq!(usage.metrics["reasoning_tokens"], Decimal::from(10));
    assert_eq!(usage.metrics["image_output_tokens"], Decimal::from(15));
    assert_eq!(usage.metrics["audio_output_tokens"], Decimal::from(5));

    let embedding = AiStudioChannel
        .extract_usage(UsageCtx {
            key: OperationKey::family(Operation::CreateEmbedding, WireFamily::Gemini),
            request_body: &body,
            response_headers: &headers,
            response_body:
                br#"{"embedding":{"values":[1.0]},"usageMetadata":{"promptTokenCount":8}}"#,
        })
        .unwrap();
    assert_eq!(embedding.input_tokens, 8);
}

#[test]
fn observes_file_and_veo_resources_and_successful_completion() {
    let headers = HeaderMap::new();
    let request = Bytes::new();
    let poll = br#"{
      "name":"models/veo-3/operations/op-1","done":true,
      "response":{"generateVideoResponse":{"generatedSamples":[
        {"video":{"uri":"https://example.invalid/v1beta/files/file-1:download"}},
        {"video":{"uri":"bad"}}
      ],"generatedVideos":[
        {"video":{"uri":"files/file-2"}}
      ]}}
    }"#;
    let key = OperationKey::family(Operation::RetrieveVideo, WireFamily::Gemini);
    assert!(
        AiStudioChannel
            .settlement_ready(UsageCtx {
                key,
                request_body: &request,
                response_headers: &headers,
                response_body: poll,
            })
            .unwrap()
    );
    let mutations = AiStudioChannel
        .resource_mutations(ResourceCtx {
            key,
            request_resource: Some(("video", "op-1")),
            request_body: &request,
            response_headers: &headers,
            response_body: poll,
        })
        .unwrap();
    let saved = mutations
        .iter()
        .filter_map(|mutation| match mutation {
            ResourceMutation::Save { kind, id, .. } => Some((*kind, id.as_str())),
            ResourceMutation::Delete { .. } => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        saved,
        vec![("video", "op-1"), ("file", "file-1"), ("file", "file-2")]
    );

    let failed = br#"{"name":"models/veo-3/operations/op-2","done":true,"error":{"code":3}}"#;
    assert!(
        !AiStudioChannel
            .settlement_ready(UsageCtx {
                key,
                request_body: &request,
                response_headers: &headers,
                response_body: failed,
            })
            .unwrap()
    );
    assert_eq!(
        AiStudioChannel
            .resource_mutations(ResourceCtx {
                key,
                request_resource: Some(("video", "op-2")),
                request_body: &request,
                response_headers: &headers,
                response_body: failed,
            })
            .unwrap()
            .len(),
        1
    );

    let files = br#"{"files":[{"name":"files/a"},{"name":"files/b"}]}"#;
    let files = AiStudioChannel
        .resource_mutations(ResourceCtx {
            key: OperationKey::family(Operation::ListFiles, WireFamily::Gemini),
            request_resource: None,
            request_body: &request,
            response_headers: &headers,
            response_body: files,
        })
        .unwrap();
    assert_eq!(files.len(), 2);
}
