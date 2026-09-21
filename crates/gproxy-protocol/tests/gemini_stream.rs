use gproxy_protocol::gemini::{
    GenerateContentRequestBody, StreamChunk, StreamRequest, StreamResponse,
};
use gproxy_protocol::{WireRequest, connection::StreamFraming};
use http::{HeaderMap, Method};
use serde_json::json;

#[test]
fn stream_records_are_the_documented_generate_response_shape() {
    // The official stream examples contain partial candidate records followed
    // by a final record carrying usageMetadata.
    let frames = [
        json!({"candidates":[{"content":{"parts":[{"text":"Hel"}],"role":"model"}}]}),
        json!({"candidates":[{"content":{"parts":[{"text":"lo"}],"role":"model"}}]}),
        json!({"candidates":[],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":2,"totalTokenCount":5}}),
    ];
    let chunks: Vec<StreamChunk> = frames
        .iter()
        .cloned()
        .map(serde_json::from_value)
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        chunks[0].candidates.as_ref().unwrap()[0]
            .content
            .as_ref()
            .unwrap()
            .parts
            .as_ref()
            .unwrap()[0]
            .text
            .as_deref(),
        Some("Hel")
    );
    assert_eq!(
        chunks[2].usage_metadata.as_ref().unwrap().total_token_count,
        Some(5)
    );
    assert!(chunks[0].rest.is_empty());
    for (chunk, frame) in chunks.iter().zip(&frames) {
        assert_eq!(serde_json::to_value(chunk).unwrap(), *frame);
    }
}

#[test]
fn stream_accepts_camel_and_snake_aliases_and_preserves_unknowns() {
    let value = json!({"usage_metadata":{"prompt_token_count":1},"futureField":{"x":true}});
    let chunk: StreamChunk = serde_json::from_value(value).unwrap();
    assert_eq!(
        chunk.usage_metadata.as_ref().unwrap().prompt_token_count,
        Some(1)
    );
    assert_eq!(chunk.rest.get("futureField").unwrap(), &json!({"x":true}));
    assert_eq!(
        serde_json::from_value::<StreamChunk>(json!({}))
            .unwrap()
            .rest
            .len(),
        0
    );
}

#[test]
fn stream_request_keeps_five_http_elements_and_framing_metadata() {
    let body = GenerateContentRequestBody::builder(vec![]).build();
    let request: StreamRequest = WireRequest {
        method: Method::POST,
        path: "/v1beta/models/gemini-2.0-flash:streamGenerateContent".into(),
        query: Some("alt=sse&key=test".into()),
        headers: HeaderMap::new(),
        body,
    };
    assert_eq!(
        request.path,
        "/v1beta/models/gemini-2.0-flash:streamGenerateContent"
    );
    assert_eq!(request.query.as_deref(), Some("alt=sse&key=test"));
    assert_eq!(request.method, Method::POST);
    assert!(request.headers.is_empty());
    assert_eq!(
        serde_json::to_value(request.body).unwrap(),
        json!({"contents":[]})
    );
    let _sse = StreamFraming::Sse;
    let _json_array = StreamFraming::JsonArray;
    fn accepts_stream_response(_: StreamResponse) {}
    let _ = accepts_stream_response;
}

#[test]
fn raw_response_envelope_and_typed_empty_record_are_distinct() {
    struct Empty;
    impl futures_core::Stream for Empty {
        type Item = Result<bytes::Bytes, gproxy_protocol::connection::TransportError>;
        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            std::task::Poll::Ready(None)
        }
    }
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "text/event-stream".parse().unwrap());
    let response: StreamResponse = gproxy_protocol::WireResponse {
        status: http::StatusCode::OK,
        headers: headers.clone(),
        body: Box::pin(Empty),
    };
    assert_eq!(response.status, http::StatusCode::OK);
    assert_eq!(response.headers, headers);
    assert_eq!(
        serde_json::to_value(StreamChunk::builder().build()).unwrap(),
        json!({})
    );
    let chunk: StreamChunk =
        serde_json::from_value(json!({"usage_metadata":null,"future":1})).unwrap();
    assert_eq!(serde_json::to_value(chunk).unwrap(), json!({"future":1}));
}
