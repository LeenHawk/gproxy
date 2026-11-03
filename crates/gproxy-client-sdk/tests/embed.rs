#![cfg(feature = "embeddings")]
use base64::Engine;
use gproxy_client_sdk::{embeddings::*, *};
use protocol::{HttpBody, OperationKey, WireRequest, WireResponse, capability::CapabilityFuture};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    num::{NonZeroU32, NonZeroUsize},
    sync::Mutex,
};
struct Reply {
    status: http::StatusCode,
    headers: http::HeaderMap,
    body: Value,
}
impl Reply {
    fn json(body: Value) -> Self {
        Self {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body,
        }
    }
}
struct Request {
    key: OperationKey,
    model: Option<String>,
    path: String,
    body: Value,
}
struct Script {
    replies: Mutex<VecDeque<Reply>>,
    requests: Mutex<Vec<Request>>,
}
impl Script {
    fn new(replies: Vec<Reply>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
        }
    }
}
impl Backend for Script {
    fn send(
        &self,
        key: OperationKey,
        model: Option<&str>,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'_, Result<WireResponse<HttpBody>>> {
        let model = model.map(str::to_owned);
        Box::pin(async move {
            let bytes = protocol::codec::read_http_body(request.body, DEFAULT_LIMITS)
                .await
                .unwrap();
            self.requests.lock().unwrap().push(Request {
                key,
                model,
                path: request.path,
                body: serde_json::from_slice(&bytes).unwrap(),
            });
            let reply = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected retry or batch");
            Ok(WireResponse {
                status: reply.status,
                headers: reply.headers,
                body: HttpBody::Bytes(reply.body.to_string().into()),
            })
        })
    }
}
fn model(dialect: EmbeddingDialect) -> EmbeddingModel {
    EmbeddingModel {
        id: "provider/embedding model".into(),
        dialect,
    }
}
fn openai_reply(indices: &[usize], offset: usize, tokens: u64, base64: bool) -> Reply {
    Reply::json(json!({
        "object":"list","model":"served",
        "data":indices.iter().map(|index| {
            let values=[(offset+index) as f32,(offset+index+1) as f32,(offset+index+2) as f32];
            let vector=if base64 {json!(base64::engine::general_purpose::STANDARD.encode(values.into_iter().flat_map(f32::to_le_bytes).collect::<Vec<_>>()))}else{json!(values)};
            json!({"object":"embedding","index":index,"embedding":vector})
        }).collect::<Vec<_>>(),
        "usage":{"prompt_tokens":tokens,"total_tokens":tokens}
    }))
}
#[test]
fn openai_batches_reorder_vectors_decode_encodings_and_sum_reported_usage() {
    futures_executor::block_on(async {
        for encoded in [false, true] {
            let client = Client::new(Script::new(vec![
                openai_reply(&[1, 0], 0, 3, encoded),
                openai_reply(&[1, 0], 2, 4, encoded),
                openai_reply(&[0], 4, 5, encoded),
            ]));
            let mut request = EmbedRequest::new(
                model(EmbeddingDialect::Openai),
                (0..5)
                    .map(|n| EmbeddingInput::Text(format!("text {n}")))
                    .collect(),
            );
            request.batch_size = NonZeroUsize::new(2);
            request.dimensions = NonZeroU32::new(3);
            request.purpose = Some(EmbeddingPurpose::Query);
            if encoded {
                request.provider_options.openai.encoding_format =
                    Some(protocol::openai::embeddings::EmbeddingEncodingFormat::Base64);
            }
            let result = client.embed(&request).await.unwrap();
            assert_eq!(
                result.vectors,
                (0..5)
                    .map(|n| vec![n as f64, (n + 1) as f64, (n + 2) as f64])
                    .collect::<Vec<_>>()
            );
            assert_eq!(result.usage.input_tokens, Some(12));
            assert!(
                result
                    .warnings
                    .iter()
                    .any(|w| w.field == "purpose" && w.kind == WarningKind::Unsupported)
            );
            let sent = client.backend.requests.lock().unwrap();
            assert_eq!(sent.len(), 3);
            assert_eq!(sent[0].body["input"], json!(["text 0", "text 1"]));
            assert_eq!(sent[2].body["input"], json!(["text 4"]));
            assert_eq!(sent[0].body["dimensions"], 3);
            assert_eq!(sent[0].path, "/v1/embeddings");
            assert_eq!(sent[0].key.operation, protocol::Operation::CreateEmbedding);
        }
    });
}
#[test]
fn gemini_embeds_images_and_text_with_native_purpose_and_dimensions() {
    futures_executor::block_on(async {
        let client = Client::new(Script::new(vec![
            Reply::json(
                json!({"embedding":{"values":[1,2]},"usageMetadata":{"promptTokenCount":3}}),
            ),
            Reply::json(
                json!({"embedding":{"values":[3,4]},"usageMetadata":{"promptTokenCount":4}}),
            ),
        ]));
        let image = Image {
            source: Asset::Reference {
                reference: "blob".into(),
            },
            media_type: None,
            detail: Detail::High,
        };
        let mut request = EmbedRequest::new(
            model(EmbeddingDialect::Gemini),
            vec![
                EmbeddingInput::Parts(vec![
                    EmbeddingPart::Text("caption".into()),
                    EmbeddingPart::Image(image),
                ]),
                EmbeddingInput::Text("document".into()),
            ],
        );
        request.dimensions = NonZeroU32::new(2);
        request.purpose = Some(EmbeddingPurpose::Document);
        let prepared = request
            .resolve_assets(DeliveryMode::PreferUrl, |id| async move {
                assert_eq!(id.reference, "blob");
                Ok(ResolvedAsset::Bytes {
                    bytes: vec![1, 2, 3],
                    media_type: "image/png".into(),
                })
            })
            .await
            .unwrap();
        let result = client.embed_prepared(prepared).await.unwrap();
        assert_eq!(result.vectors, vec![vec![1.0, 2.0], vec![3.0, 4.0]]);
        assert_eq!(result.usage.input_tokens, Some(7));
        assert!(result.warnings.iter().any(|w| w.field.ends_with(".detail")));
        assert!(result.warnings.iter().any(|w| w.field == "batch_size"));
        let sent = client.backend.requests.lock().unwrap();
        assert_eq!(sent.len(), 2);
        assert_eq!(
            sent[0].path,
            "/v1beta/models/provider%2Fembedding%20model:embedContent"
        );
        assert_eq!(sent[0].model.as_deref(), Some("provider/embedding model"));
        assert_eq!(sent[0].body["taskType"], "RETRIEVAL_DOCUMENT");
        assert_eq!(sent[0].body["outputDimensionality"], 2);
        assert_eq!(sent[0].body["content"]["parts"][0]["text"], "caption");
        assert_eq!(
            sent[0].body["content"]["parts"][1]["inlineData"]["data"],
            "AQID"
        );
        assert_eq!(sent[0].key.dialect, protocol::Dialect::Gemini);
    });
}
#[test]
fn every_supported_gemini_purpose_is_mapped_and_native_config_overrides() {
    futures_executor::block_on(async {
        for (purpose, expected) in [
            (EmbeddingPurpose::Query, "RETRIEVAL_QUERY"),
            (EmbeddingPurpose::Document, "RETRIEVAL_DOCUMENT"),
            (EmbeddingPurpose::SemanticSimilarity, "SEMANTIC_SIMILARITY"),
            (EmbeddingPurpose::Classification, "CLASSIFICATION"),
            (EmbeddingPurpose::Clustering, "CLUSTERING"),
            (EmbeddingPurpose::QuestionAnswering, "QUESTION_ANSWERING"),
            (EmbeddingPurpose::FactVerification, "FACT_VERIFICATION"),
            (EmbeddingPurpose::CodeQuery, "CODE_RETRIEVAL_QUERY"),
        ] {
            let client = Client::new(Script::new(vec![Reply::json(
                json!({"embedding":{"values":[1]}}),
            )]));
            let mut request = EmbedRequest::new(
                model(EmbeddingDialect::Gemini),
                vec![EmbeddingInput::Text("input".into())],
            );
            request.purpose = Some(purpose);
            client.embed(&request).await.unwrap();
            assert_eq!(
                client.backend.requests.lock().unwrap()[0].body["taskType"],
                expected
            );
        }
        let client = Client::new(Script::new(vec![Reply::json(
            json!({"embedding":{"values":[1,2]}}),
        )]));
        let mut request = EmbedRequest::new(
            model(EmbeddingDialect::Gemini),
            vec![EmbeddingInput::Text("input".into())],
        );
        request.purpose = Some(EmbeddingPurpose::Query);
        request.dimensions = NonZeroU32::new(3);
        request.provider_options.gemini.config = Some(
            serde_json::from_value(
                json!({"taskType":"CLASSIFICATION","outputDimensionality":2,"documentOcr":true}),
            )
            .unwrap(),
        );
        let result = client.embed(&request).await.unwrap();
        assert!(result.warnings.iter().any(|w| w.field == "purpose"));
        assert!(result.warnings.iter().any(|w| w.field == "dimensions"));
        let sent = client.backend.requests.lock().unwrap();
        assert!(sent[0].body.get("taskType").is_none());
        assert_eq!(
            sent[0].body["embedContentConfig"]["taskType"],
            "CLASSIFICATION"
        );
        assert_eq!(
            sent[0].body["embedContentConfig"]["outputDimensionality"],
            2
        );
    });
}
#[test]
fn embedding_failures_preserve_http_body_and_never_retry_or_continue_batches() {
    futures_executor::block_on(async {
        let mut rejected = Reply::json(json!({"error":{"message":"slow"}}));
        rejected.status = http::StatusCode::TOO_MANY_REQUESTS;
        rejected.headers.insert(
            http::header::RETRY_AFTER,
            http::HeaderValue::from_static("7"),
        );
        let client = Client::new(Script::new(vec![rejected, openai_reply(&[0], 0, 1, false)]));
        let mut request = EmbedRequest::new(
            model(EmbeddingDialect::Openai),
            vec![
                EmbeddingInput::Text("one".into()),
                EmbeddingInput::Text("two".into()),
            ],
        );
        request.batch_size = NonZeroUsize::new(1);
        let error = client.embed(&request).await.unwrap_err();
        assert_eq!(error.kind, ErrorKind::RateLimit);
        assert_eq!(error.status, Some(429));
        assert!(error.retryable);
        assert_eq!(error.retry_after.as_deref(), Some("7"));
        assert!(error.body.as_deref().unwrap().contains("slow"));
        assert_eq!(client.backend.requests.lock().unwrap().len(), 1);
        assert_eq!(client.backend.replies.lock().unwrap().len(), 1);
    });
}
#[test]
fn malformed_indices_missing_embeddings_and_invalid_vectors_are_reported_with_status() {
    futures_executor::block_on(async {
        let reply = Reply::json(
            json!({"object":"list","model":"served","usage":{"prompt_tokens":1,"total_tokens":1},"data":[{"object":"embedding","index":2,"embedding":[1]}]}),
        );
        let body = reply.body.to_string();
        let client = Client::new(Script::new(vec![reply]));
        let request = EmbedRequest::new(
            model(EmbeddingDialect::Openai),
            vec![EmbeddingInput::Text("one".into())],
        );
        let error = client.embed(&request).await.unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidData);
        assert_eq!(error.status, Some(200));
        assert_eq!(error.body.as_deref(), Some(body.as_str()));
        let client = Client::new(Script::new(vec![Reply::json(
            json!({"usageMetadata":{"promptTokenCount":1}}),
        )]));
        let request = EmbedRequest::new(
            model(EmbeddingDialect::Gemini),
            vec![EmbeddingInput::Text("one".into())],
        );
        let error = client.embed(&request).await.unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidData);
        assert_eq!(error.status, Some(200));
        let encoded = base64::engine::general_purpose::STANDARD.encode([0u8, 1, 2]);
        let client = Client::new(Script::new(vec![Reply::json(
            json!({"object":"list","model":"served","usage":{"prompt_tokens":1,"total_tokens":1},"data":[{"object":"embedding","index":0,"embedding":encoded}]}),
        )]));
        let request = EmbedRequest::new(
            model(EmbeddingDialect::Openai),
            vec![EmbeddingInput::Text("one".into())],
        );
        assert_eq!(
            client.embed(&request).await.unwrap_err().kind,
            ErrorKind::InvalidData
        );
    });
}
#[test]
fn empty_and_unsupported_embedding_inputs_do_not_start_io() {
    futures_executor::block_on(async {
        let client = Client::new(Script::new(vec![]));
        let empty = EmbedRequest::new(model(EmbeddingDialect::Openai), vec![]);
        assert!(client.embed(&empty).await.unwrap().vectors.is_empty());
        let image = Image {
            source: Asset::Inline { bytes: vec![1] },
            media_type: Some("image/png".into()),
            detail: Detail::Auto,
        };
        let unsupported = EmbedRequest::new(
            model(EmbeddingDialect::Openai),
            vec![EmbeddingInput::Image(image)],
        );
        assert_eq!(
            client.embed(&unsupported).await.unwrap_err().kind,
            ErrorKind::InvalidData
        );
        assert!(client.backend.requests.lock().unwrap().is_empty());
        assert!(serde_json::from_value::<EmbedRequest>(json!({"model":{"id":"m","dialect":"openai"},"inputs":[],"dimensions":0,"purpose":null,"batch_size":null})).is_err());
    });
}

#[test]
fn missing_usage_stays_unknown_and_tensor_shape_is_reported() {
    futures_executor::block_on(async {
        let client = Client::new(Script::new(vec![
            Reply::json(
                json!({"embedding":{"values":[1,2],"shape":[1,2]},"usageMetadata":{"promptTokenCount":3}}),
            ),
            Reply::json(json!({"embedding":{"values":[3,4]}})),
        ]));
        let request = EmbedRequest::new(
            model(EmbeddingDialect::Gemini),
            vec![
                EmbeddingInput::Text("first".into()),
                EmbeddingInput::Text("second".into()),
            ],
        );
        let result = client.embed(&request).await.unwrap();
        assert_eq!(result.usage.input_tokens, None);
        assert_eq!(result.vectors.len(), 2);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.field == "response.embedding.shape"
                    && w.kind == WarningKind::Compatibility)
        );
    });
}
#[test]
fn nonfinite_base64_vectors_cannot_enter_the_serializable_ir() {
    futures_executor::block_on(async {
        let encoded = base64::engine::general_purpose::STANDARD.encode(f32::NAN.to_le_bytes());
        let body = json!({"object":"list","model":"served","usage":{"prompt_tokens":1,"total_tokens":1},"data":[{"object":"embedding","index":0,"embedding":encoded}]});
        let client = Client::new(Script::new(vec![Reply::json(body)]));
        let request = EmbedRequest::new(
            model(EmbeddingDialect::Openai),
            vec![EmbeddingInput::Text("one".into())],
        );
        let error = client.embed(&request).await.unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidData);
        assert_eq!(error.status, Some(200));
        assert!(error.body.is_some());
    });
}
