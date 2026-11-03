use super::*;
#[cfg(feature = "openai")]
#[tokio::test]
async fn signed_urls_never_replace_the_reference_in_the_request_or_persisted_history() {
    let fixture = storage::fixture(storage::config());
    let request = support::request(
        Dialect::Openai,
        vec![Message {
            role: Role::User,
            content: vec![Block::new(Content::Image(Image {
                source: Asset::Reference {
                    reference: "app:blob".into(),
                },
                media_type: Some("image/png".into()),
                detail: Detail::Auto,
            }))],
        }],
    );
    let before = serde_json::to_value(&request).unwrap();
    let history = Conversation::new(request.messages.clone());
    for _ in 0..2 {
        let prepared = request
            .resolve_assets_s3(&fixture.store, |reference| async move {
                assert_eq!(reference, "app:blob");
                Ok(data("image/png"))
            })
            .await
            .unwrap();
        assert!(prepared.warnings().is_empty());
        let native = prepared.render(false).unwrap().request;
        assert!(
            native["input"][0]["content"][0]["image_url"]
                .as_str()
                .unwrap()
                .contains("X-Amz-Signature=")
        );
        assert_eq!(serde_json::to_value(&request).unwrap(), before);
        let persisted = serde_json::to_string(&history).unwrap();
        assert!(persisted.contains("app:blob"));
        assert!(!persisted.contains("X-Amz-"));
    }
    assert_eq!(fixture.probe.presigns.load(Ordering::SeqCst), 2);
    assert_eq!(
        fixture
            .transport
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.method == http::Method::PUT)
            .count(),
        1
    );
}

#[cfg(feature = "claude")]
#[tokio::test]
async fn mime_based_inline_fallback_preserves_the_round_three_warning() {
    let fixture = storage::fixture(storage::config());
    let request = GenerateRequest::new(
        Model {
            id: "claude".into(),
            dialect: Dialect::Claude,
        },
        vec![Message {
            role: Role::User,
            content: vec![Block::new(Content::Document(Document {
                source: DocumentSource::Asset {
                    source: Asset::Reference {
                        reference: "text".into(),
                    },
                },
                media_type: None,
                title: None,
                context: None,
                citations: None,
                detail: Detail::Auto,
            }))],
        }],
    );
    let prepared = request
        .resolve_assets_s3(&fixture.store, |_| async { Ok(data("text/plain")) })
        .await
        .unwrap();
    assert_eq!(prepared.warnings().len(), 1);
    assert_eq!(prepared.warnings()[0].kind, WarningKind::Compatibility);
    assert!(fixture.transport.requests.lock().unwrap().is_empty());
}

#[cfg(feature = "openai")]
struct GenerationBackend {
    sent: std::sync::Mutex<Vec<serde_json::Value>>,
}

#[cfg(feature = "openai")]
impl Backend for GenerationBackend {
    fn send(
        &self,
        _: protocol::OperationKey,
        _: Option<&str>,
        request: protocol::WireRequest<protocol::HttpBody>,
    ) -> protocol::capability::CapabilityFuture<
        '_,
        Result<protocol::WireResponse<protocol::HttpBody>>,
    > {
        Box::pin(async move {
            let bytes = protocol::codec::read_http_body(request.body, DEFAULT_LIMITS)
                .await
                .unwrap();
            let request: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let stream = request["stream"] == true;
            self.sent.lock().unwrap().push(request);
            let response = support::openai();
            let body = if stream {
                support::openai_events(&response)
                    .into_iter()
                    .map(|event| format!("data: {event}\n\n"))
                    .collect::<String>()
            } else {
                response.to_string()
            };
            Ok(protocol::WireResponse {
                status: http::StatusCode::OK,
                headers: http::HeaderMap::new(),
                body: protocol::HttpBody::Bytes(body.into_bytes().into()),
            })
        })
    }
}

#[cfg(feature = "openai")]
#[tokio::test]
async fn configured_client_prepares_each_generate_and_stream_with_only_the_byte_callback() {
    use futures_util::StreamExt;
    let fixture = storage::fixture(storage::config());
    let client = Client::new(GenerationBackend {
        sent: std::sync::Mutex::new(Vec::new()),
    })
    .with_s3(fixture.store);
    let request = support::request(
        Dialect::Openai,
        vec![Message {
            role: Role::User,
            content: vec![Block::new(Content::Image(Image {
                source: Asset::Reference {
                    reference: "app-key".into(),
                },
                media_type: Some("image/png".into()),
                detail: Detail::Auto,
            }))],
        }],
    );
    let original = serde_json::to_value(&request).unwrap();
    let mut loads = 0;
    for _ in 0..2 {
        let result = client
            .generate(&request, |reference| {
                loads += 1;
                assert_eq!(reference, "app-key");
                std::future::ready(Ok(data("image/png")))
            })
            .await
            .unwrap();
        assert!(result.warnings.is_empty());
    }
    let stream = client
        .stream(&request, |_| {
            loads += 1;
            std::future::ready(Ok(data("image/png")))
        })
        .await
        .unwrap();
    let events = stream.collect::<Vec<_>>().await;
    assert!(matches!(&events[0],Event::Start{warnings} if warnings.is_empty()));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Finish { .. }))
    );
    assert_eq!(loads, 3);
    assert_eq!(fixture.probe.presigns.load(Ordering::SeqCst), 3);
    assert_eq!(
        fixture
            .transport
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.method == http::Method::PUT)
            .count(),
        1
    );
    let sent = client.client.backend.sent.lock().unwrap();
    assert_eq!(sent.len(), 3);
    for request in sent.iter() {
        assert!(
            request["input"][0]["content"][0]["image_url"]
                .as_str()
                .unwrap()
                .contains("X-Amz-Signature=")
        );
    }
    assert_eq!(serde_json::to_value(&request).unwrap(), original);
    assert!(
        !serde_json::to_string(&Conversation::new(request.messages))
            .unwrap()
            .contains("X-Amz-")
    );
}

#[cfg(feature = "embeddings")]
struct EmbeddingBackend {
    sent: std::sync::Mutex<Vec<serde_json::Value>>,
}

#[cfg(feature = "embeddings")]
impl Backend for EmbeddingBackend {
    fn send(
        &self,
        key: protocol::OperationKey,
        _: Option<&str>,
        request: protocol::WireRequest<protocol::HttpBody>,
    ) -> protocol::capability::CapabilityFuture<
        '_,
        Result<protocol::WireResponse<protocol::HttpBody>>,
    > {
        Box::pin(async move {
            let bytes = protocol::codec::read_http_body(request.body, DEFAULT_LIMITS)
                .await
                .unwrap();
            self.sent
                .lock()
                .unwrap()
                .push(serde_json::from_slice(&bytes).unwrap());
            let response = if key.dialect == protocol::Dialect::Gemini {
                serde_json::json!({"embedding":{"values":[0.5]}})
            } else {
                serde_json::json!({"object":"list","model":"embed","data":[{"object":"embedding","index":0,"embedding":[0.5]}],"usage":{"prompt_tokens":1,"total_tokens":1}})
            };
            Ok(protocol::WireResponse {
                status: http::StatusCode::OK,
                headers: http::HeaderMap::new(),
                body: protocol::HttpBody::Bytes(response.to_string().into_bytes().into()),
            })
        })
    }
}

#[cfg(feature = "embeddings")]
#[tokio::test]
async fn configured_embedding_client_uploads_gemini_images_and_leaves_openai_text_alone() {
    use embeddings::*;
    let fixture = storage::fixture(storage::config());
    let client = Client::new(EmbeddingBackend {
        sent: std::sync::Mutex::new(Vec::new()),
    })
    .with_s3(fixture.store);
    let request = EmbedRequest::new(
        EmbeddingModel {
            id: "embed".into(),
            dialect: EmbeddingDialect::Gemini,
        },
        vec![EmbeddingInput::Image(Image {
            source: Asset::Reference {
                reference: "embed-blob".into(),
            },
            media_type: Some("image/png".into()),
            detail: Detail::Auto,
        })],
    );
    let before = serde_json::to_value(&request).unwrap();
    let result = client
        .embed(&request, |reference| async move {
            assert_eq!(reference, "embed-blob");
            Ok(data("image/png"))
        })
        .await
        .unwrap();
    assert_eq!(result.vectors, vec![vec![0.5]]);
    assert!(result.warnings.is_empty());
    {
        let sent = client.client.backend.sent.lock().unwrap();
        assert!(
            sent[0]["content"]["parts"][0]["fileData"]["fileUri"]
                .as_str()
                .unwrap()
                .contains("X-Amz-Signature=")
        );
    }
    assert_eq!(serde_json::to_value(&request).unwrap(), before);
    let request = EmbedRequest::new(
        EmbeddingModel {
            id: "embed".into(),
            dialect: EmbeddingDialect::Openai,
        },
        vec![EmbeddingInput::Text("hello".into())],
    );
    let result = client
        .embed(&request, |_| async {
            panic!("text embeddings do not load assets")
        })
        .await
        .unwrap();
    assert_eq!(result.vectors, vec![vec![0.5]]);
    assert_eq!(fixture.probe.presigns.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.transport.requests.lock().unwrap().len(), 2);
}
