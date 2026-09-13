use gproxy_protocol::WireRequest;
use gproxy_protocol::gemini::embeddings::*;
use gproxy_protocol::openai::embeddings::*;
use gproxy_protocol::openai::rerank::*;
use http::{HeaderMap, Method};
use serde_json::json;

#[test]
fn openai_embedding_supports_token_shapes_and_base64() {
    let req: CreateEmbeddingRequestBody = serde_json::from_value(json!({
        "input":[[1,2],[3,4]], "model":"text-embedding-3-small", "encoding_format":"base64"
    }))
    .unwrap();
    assert!(matches!(req.input, EmbeddingInput::TokenArrays(_)));
    let response: CreateEmbeddingResponseBody = serde_json::from_value(json!({
        "data":[{"embedding":"AQID", "index":0, "object":"embedding"}],
        "model":"text-embedding-3-small", "object":"list", "usage":{"prompt_tokens":4,"total_tokens":4}
    })).unwrap();
    assert!(matches!(
        response.data[0].embedding,
        EmbeddingVector::Base64(_)
    ));
}

#[test]
fn gemini_embed_and_batch_keep_camel_snake_aliases_and_unknowns() {
    let one: EmbedContentResponseBody = serde_json::from_value(json!({
        "embedding":{"values":[0.1,0.2],"shape":[2]},
        "usage_metadata":{"prompt_token_count":2}, "futureField":true
    }))
    .unwrap();
    assert_eq!(one.usage_metadata.unwrap().prompt_token_count, Some(2));
    assert_eq!(one.rest.get("futureField"), Some(&json!(true)));
    let batch: BatchEmbedContentsRequestBody = serde_json::from_value(json!({
        "requests":[{"model":"models/gemini-embedding-001","content":{"parts":[{"text":"x"}]},"taskType":"RETRIEVAL_DOCUMENT","outputDimensionality":3}]
    })).unwrap();
    assert_eq!(batch.requests[0].output_dimensionality, Some(3));
}

#[test]
fn rerank_openai_compatible_structured_documents_and_nullable_echo() {
    let request: RerankRequestBody = serde_json::from_value(json!({
        "model":"cohere/rerank-v3.5", "query":"capital", "documents":["Paris",{"image":"data:image/png;base64,AA=="}], "top_n":1
    })).unwrap();
    assert_eq!(request.top_n, Some(1));
    let response: RerankResponseBody = serde_json::from_value(json!({
        "id":"r1", "model":"cohere/rerank-v3.5", "results":[{"index":0,"relevance_score":0.98,"document":{"text":"Paris"}}], "usage":{"search_units":1,"total_tokens":4}
    })).unwrap();
    assert_eq!(response.results[0].document.text.as_deref(), Some("Paris"));
}

#[test]
fn http_envelope_keeps_embedding_path_and_body_separate() {
    let body =
        CreateEmbeddingRequestBody::builder(EmbeddingInput::Text("hello".into()), "m".into())
            .build();
    let request: CreateEmbeddingRequest = WireRequest {
        method: Method::POST,
        path: "/v1/embeddings".into(),
        query: None,
        headers: HeaderMap::new(),
        body,
    };
    assert_eq!(request.path, "/v1/embeddings");
    assert_eq!(request.method, Method::POST);
    assert!(request.query.is_none());
    assert!(request.headers.is_empty());
    assert_eq!(
        serde_json::to_value(request.body).unwrap(),
        json!({"input":"hello","model":"m"})
    );
}

fn exact<T: serde::de::DeserializeOwned + serde::Serialize + std::fmt::Debug>(
    value: serde_json::Value,
) {
    let parsed: T = serde_json::from_value(value.clone()).unwrap();
    assert!(
        format!("{parsed:?}")
            .split("rest: ")
            .skip(1)
            .all(|rest| rest.starts_with("{}")),
        "{parsed:?}"
    );
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
}
#[test]
fn embedding_variants_defaults_and_precise_gemini_usage() {
    for input in [
        json!("text"),
        json!(["a", "b"]),
        json!([1, 2]),
        json!([[1], [2]]),
    ] {
        exact::<CreateEmbeddingRequestBody>(json!({"input":input,"model":"m"}));
    }
    for vector in [json!([0.1, 0.2]), json!("AQID")] {
        exact::<CreateEmbeddingResponseBody>(
            json!({"data":[{"embedding":vector,"index":0,"object":"embedding"}],"model":"m","object":"list","usage":{"prompt_tokens":1,"total_tokens":1}}),
        );
    }
    exact::<EmbedContentRequestBody>(
        json!({"content":{},"taskType":"RETRIEVAL_QUERY","title":"t","outputDimensionality":3,"embedContentConfig":{"taskType":"RETRIEVAL_DOCUMENT","title":"d","outputDimensionality":2,"autoTruncate":false,"documentOcr":true,"audioTrackExtraction":true}}),
    );
    exact::<EmbedContentResponseBody>(
        json!({"embedding":{"values":[0.1],"shape":[1]},"usageMetadata":{"promptTokenCount":1,"promptTokenDetails":[{"modality":"TEXT","tokenCount":1}]}}),
    );
    exact::<BatchEmbedContentsResponseBody>(
        json!({"embeddings":[{}],"usageMetadata":{"promptTokenDetails":[{}]}}),
    );
    exact::<EmbedContentResponseBody>(json!({}));
    exact::<BatchEmbedContentsResponseBody>(json!({}));
    let parsed:EmbedContentRequestBody=serde_json::from_value(json!({"content":{},"embed_content_config":{"document_ocr":true,"audio_track_extraction":false,"task_type":"CLUSTERING"}})).unwrap();
    assert_eq!(
        serde_json::to_value(parsed).unwrap(),
        json!({"content":{},"embedContentConfig":{"documentOcr":true,"audioTrackExtraction":false,"taskType":"CLUSTERING"}})
    );
    let parsed: EmbeddingUsageMetadata =
        serde_json::from_value(json!({"prompt_token_count":0,"prompt_token_details":null}))
            .unwrap();
    assert_eq!(
        serde_json::to_value(parsed).unwrap(),
        json!({"promptTokenCount":0})
    );
    assert!(serde_json::from_value::<EmbedContentRequestBody>(json!({})).is_err());
    assert!(serde_json::from_value::<BatchEmbedContentRequest>(json!({"content":{}})).is_err());
}
#[test]
fn openrouter_provider_shapes_preserve_all_known_fields_and_nulls() {
    let value = json!({"model":"m","query":"q","documents":["a",{"text":"b","image":"u"}],"top_n":1,"provider":{"allow_fallbacks":true,"data_collection":"deny","enforce_distillable_text":true,"ignore":["custom-provider"],"only":["x"],"order":["x"],"max_price":{"audio":"1","completion":"2","image":"3","prompt":"4","request":"5"},"preferred_max_latency":{"p50":1,"p75":2,"p90":3,"p99":4},"preferred_min_throughput":100,"quantizations":["fp16"],"require_parameters":true,"sort":{"by":"exacto","partition":"none"},"zdr":true}});
    exact::<RerankRequestBody>(value.clone());
    for path in [
        "/provider",
        "/provider/allow_fallbacks",
        "/provider/data_collection",
        "/provider/enforce_distillable_text",
        "/provider/ignore",
        "/provider/only",
        "/provider/order",
        "/provider/preferred_max_latency",
        "/provider/preferred_min_throughput",
        "/provider/quantizations",
        "/provider/require_parameters",
        "/provider/sort",
        "/provider/zdr",
        "/provider/sort/by",
        "/provider/sort/partition",
        "/provider/preferred_max_latency/p50",
    ] {
        let mut null = value.clone();
        *null.pointer_mut(path).unwrap() = serde_json::Value::Null;
        exact::<RerankRequestBody>(null);
        let mut missing = value.clone();
        let (parent, key) = path.rsplit_once('/').unwrap();
        missing
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key);
        exact::<RerankRequestBody>(missing);
    }
    exact::<RerankRequestBody>(
        json!({"model":"m","query":"q","documents":["a"],"provider":{"sort":"price","preferred_min_throughput":{"p50":100,"p99":null}}}),
    );
    exact::<RerankResponseBody>(
        json!({"id":"i","model":"m","provider":"p","results":[{"index":0,"relevance_score":0.9,"document":{"image":"u"}}],"usage":{"search_units":1,"total_tokens":2,"cost":0.1}}),
    );
    assert!(
        serde_json::from_value::<RerankResponseBody>(
            json!({"model":"m","results":[{"index":0,"relevance_score":0.1}]})
        )
        .is_err()
    );
    assert!(
        serde_json::from_value::<RerankResponseBody>(
            json!({"model":"m","results":[{"index":0,"relevance_score":0.1,"document":null}]})
        )
        .is_err()
    );
    exact::<DashscopeRerankRequestBody>(
        json!({"model":"qwen3-rerank","query":"q","documents":["a"],"top_n":1,"instruct":"rank"}),
    );
    exact::<DashscopeRerankResponseBody>(
        json!({"id":"i","object":"list","model":"qwen3-rerank","results":[{"index":0,"relevance_score":0.9}],"usage":{"total_tokens":1}}),
    );
}
#[test]
fn closed_enums_and_http_response_are_typed() {
    for value in [
        "TASK_TYPE_UNSPECIFIED",
        "RETRIEVAL_QUERY",
        "RETRIEVAL_DOCUMENT",
        "SEMANTIC_SIMILARITY",
        "CLASSIFICATION",
        "CLUSTERING",
        "QUESTION_ANSWERING",
        "FACT_VERIFICATION",
        "CODE_RETRIEVAL_QUERY",
    ] {
        exact::<GeminiTaskType>(json!(value));
    }
    for value in [
        "int4", "int8", "fp4", "mxfp4", "nvfp4", "fp6", "fp8", "mxfp8", "fp16", "bf16", "fp32",
        "unknown",
    ] {
        exact::<Quantization>(json!(value));
    }
    assert!(serde_json::from_value::<Quantization>(json!("fp7")).is_err());
    assert!(serde_json::from_value::<EmbeddingEncodingFormat>(json!("hex")).is_err());
    let body = CreateEmbeddingResponseBody::builder(
        vec![],
        "m".into(),
        EmbeddingListObject::List,
        EmbeddingUsage::builder(0, 0).build(),
    )
    .build();
    let mut headers = HeaderMap::new();
    headers.insert("content-type", "application/json".parse().unwrap());
    let response: CreateEmbeddingResponse = gproxy_protocol::WireResponse {
        status: http::StatusCode::OK,
        headers: headers.clone(),
        body,
    };
    assert_eq!(response.status, http::StatusCode::OK);
    assert_eq!(response.headers, headers);
    exact::<CreateEmbeddingResponseBody>(serde_json::to_value(response.body).unwrap());
    let unknown: EmbedContentResponseBody = serde_json::from_value(json!({"future":null})).unwrap();
    assert_eq!(unknown.rest["future"], json!(null));
    assert_eq!(
        serde_json::to_value(unknown).unwrap(),
        json!({"future":null})
    );
}

#[test]
fn gemini_and_rerank_http_aliases_keep_resource_paths_outside_json() {
    let body =
        EmbedContentRequestBody::builder(gproxy_protocol::gemini::Content::builder().build())
            .build();
    let request: EmbedContentRequest = WireRequest {
        method: Method::POST,
        path: "/v1beta/models/m:embedContent".into(),
        query: Some("key=test".into()),
        headers: HeaderMap::new(),
        body,
    };
    assert_eq!(request.query.as_deref(), Some("key=test"));
    assert_eq!(
        serde_json::to_value(request.body).unwrap(),
        json!({"content":{}})
    );
    let batch: BatchEmbedContentsRequest = WireRequest {
        method: Method::POST,
        path: "/v1beta/models/m:batchEmbedContents".into(),
        query: None,
        headers: HeaderMap::new(),
        body: BatchEmbedContentsRequestBody::builder(vec![
            BatchEmbedContentRequest::builder(
                "models/m".into(),
                gproxy_protocol::gemini::Content::builder().build(),
            )
            .build(),
        ])
        .build(),
    };
    assert_eq!(
        serde_json::to_value(batch.body).unwrap(),
        json!({"requests":[{"model":"models/m","content":{}}]})
    );
    let response: BatchEmbedContentsResponse = gproxy_protocol::WireResponse {
        status: http::StatusCode::OK,
        headers: HeaderMap::new(),
        body: BatchEmbedContentsResponseBody::builder().build(),
    };
    assert_eq!(serde_json::to_value(response.body).unwrap(), json!({}));
    let request: RerankRequest = WireRequest {
        method: Method::POST,
        path: "/api/v1/rerank".into(),
        query: None,
        headers: HeaderMap::new(),
        body: RerankRequestBody::builder(
            "m".into(),
            "q".into(),
            vec![RerankDocumentInput::Text("d".into())],
        )
        .build(),
    };
    assert_eq!(
        serde_json::to_value(request.body).unwrap(),
        json!({"model":"m","query":"q","documents":["d"]})
    );
}
