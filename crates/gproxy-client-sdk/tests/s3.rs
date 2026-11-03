#![cfg(all(feature = "s3", not(target_arch = "wasm32")))]
#[path = "support/s3.rs"]
mod storage;
#[cfg(any(feature = "openai", feature = "gemini"))]
mod support;
use gproxy_client_sdk::{s3::*, *};
use std::{sync::atomic::Ordering, time::Duration};

fn data(mime: &str) -> AssetData {
    AssetData {
        bytes: b"abc".to_vec(),
        media_type: mime.into(),
    }
}
fn input(target: AssetDialect, kind: MediaKind, mime: Option<&str>) -> ResolveAsset {
    let capability = media_capability(target, kind);
    ResolveAsset {
        reference: "hash-of-app-blob".into(),
        target,
        kind,
        location: AssetLocation::Message,
        media_type: mime.map(str::to_owned),
        preferred: capability.preferred(DeliveryMode::PreferUrl).unwrap(),
        capability,
    }
}
#[test]
fn content_addressed_keys_use_sha256_canonical_extensions_and_an_optional_prefix() {
    let fixture = storage::fixture(storage::config());
    assert_eq!(
        fixture.store.object_key(&data("image/png")),
        "images/ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad.png"
    );
    assert_eq!(
        fixture.store.object_key(&data("image/jpeg")),
        fixture.store.object_key(&data("image/jpg"))
    );
    assert!(
        fixture
            .store
            .object_key(&data("audio/l16;rate=24000"))
            .ends_with(".pcm")
    );
    assert!(
        fixture
            .store
            .object_key(&data("application/x-custom"))
            .ends_with(".bin")
    );
    let mut changed = data("image/png");
    changed.bytes.push(0);
    assert_ne!(
        fixture.store.object_key(&changed),
        fixture.store.object_key(&data("image/png"))
    );
    let mut config = storage::config();
    config.key_prefix.clear();
    let fixture = storage::fixture(config);
    assert!(!fixture.store.object_key(&data("image/png")).contains('/'));
    assert!(fixture.transport.requests.lock().unwrap().is_empty());
}
#[tokio::test]
async fn stat_skips_repeat_uploads_and_every_request_reaches_opendal_presign() {
    let fixture = storage::fixture(storage::config());
    for _ in 0..2 {
        let resolved = fixture
            .store
            .resolve(
                input(AssetDialect::Openai, MediaKind::Image, None),
                data("image/png"),
            )
            .await
            .unwrap();
        let ResolvedAsset::Url {
            url,
            media_type,
            byte_len,
        } = resolved
        else {
            panic!("URL should be preferred")
        };
        assert_eq!(media_type.as_deref(), Some("image/png"));
        assert_eq!(byte_len, Some(3));
        let uri: http::Uri = url.parse().unwrap();
        assert_eq!(uri.host(), Some("account.r2.cloudflarestorage.com"));
        assert_eq!(
            uri.path(),
            format!("/assets/{}", fixture.store.object_key(&data("image/png")))
        );
        let query = uri.query().unwrap();
        assert!(query.contains("X-Amz-Algorithm=AWS4-HMAC-SHA256"));
        assert!(query.contains("X-Amz-Expires=90"));
        assert!(query.contains("X-Amz-Signature="));
    }
    assert_eq!(fixture.probe.presigns.load(Ordering::SeqCst), 2);
    let requests = fixture.transport.requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.method == http::Method::HEAD)
            .count(),
        2
    );
    let writes: Vec<_> = requests
        .iter()
        .filter(|r| r.method == http::Method::PUT)
        .collect();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].mime.as_deref(), Some("image/png"));
    assert_eq!(
        fixture
            .transport
            .objects
            .lock()
            .unwrap()
            .values()
            .next()
            .unwrap()
            .bytes,
        b"abc"
    );
}
#[tokio::test]
async fn an_object_that_already_exists_is_not_overwritten() {
    let fixture = storage::fixture(storage::config());
    let path = format!("/assets/{}", fixture.store.object_key(&data("image/png")));
    fixture.transport.objects.lock().unwrap().insert(
        path,
        storage::Stored {
            bytes: b"abc".to_vec(),
            mime: "image/png".into(),
        },
    );
    fixture
        .store
        .resolve(
            input(AssetDialect::Openai, MediaKind::Image, None),
            data("image/png"),
        )
        .await
        .unwrap();
    assert_eq!(fixture.transport.requests.lock().unwrap().len(), 1);
    assert_eq!(
        fixture.transport.requests.lock().unwrap()[0].method,
        http::Method::HEAD
    );
    assert_eq!(fixture.probe.presigns.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn inline_only_media_and_mime_based_fallback_do_not_touch_storage() {
    let fixture = storage::fixture(storage::config());
    for (target, kind, mime) in [
        (AssetDialect::OpenaiChat, MediaKind::Audio, "audio/wav"),
        (AssetDialect::OpenaiChat, MediaKind::File, "text/plain"),
        (AssetDialect::Claude, MediaKind::Document, "text/plain"),
    ] {
        let resolved = fixture
            .store
            .resolve(input(target, kind, None), data(mime))
            .await
            .unwrap();
        assert!(matches!(resolved,ResolvedAsset::Bytes{bytes,..} if bytes==b"abc"));
    }
    assert!(fixture.transport.requests.lock().unwrap().is_empty());
    assert_eq!(fixture.probe.presigns.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn invalid_mime_and_delivery_constraints_fail_before_any_storage_operation() {
    let fixture = storage::fixture(storage::config());
    let mut limited = input(AssetDialect::Gemini, MediaKind::Image, None);
    limited.capability.url.as_mut().unwrap().max_bytes = Some(2);
    for (input, data) in [
        (
            input(AssetDialect::Claude, MediaKind::Image, None),
            data("image/tiff"),
        ),
        (
            input(AssetDialect::Openai, MediaKind::Image, Some("image/png")),
            data("image/jpeg"),
        ),
        (limited, data("image/png")),
    ] {
        let error = fixture.store.resolve(input, data).await.unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidData);
    }
    assert!(fixture.transport.requests.lock().unwrap().is_empty());
    assert_eq!(fixture.probe.presigns.load(Ordering::SeqCst), 0);
}
#[tokio::test]
async fn storage_errors_do_not_trigger_retry_or_inline_fallback() {
    for method in [http::Method::HEAD, http::Method::PUT] {
        let fixture = storage::fixture(storage::config());
        *fixture.transport.fail.lock().unwrap() = Some((method.clone(), 403));
        let error = fixture
            .store
            .resolve(
                input(AssetDialect::Openai, MediaKind::Image, None),
                data("image/png"),
            )
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Auth);
        assert!(!error.retryable);
        assert_eq!(
            fixture
                .transport
                .requests
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.method == method)
                .count(),
            1
        );
        assert_eq!(fixture.probe.presigns.load(Ordering::SeqCst), 0);
    }
    let fixture = storage::fixture(storage::config());
    *fixture.probe.failure.lock().unwrap() = Some(gproxy_file::ErrorKind::Unsupported);
    let error = fixture
        .store
        .resolve(
            input(AssetDialect::Openai, MediaKind::Image, None),
            data("image/png"),
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidData);
    assert_eq!(fixture.probe.presigns.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.transport.requests.lock().unwrap().len(), 2); // HEAD, PUT; no retry.
}
#[tokio::test]
async fn endpoint_addressing_session_credentials_and_expiry_use_the_opendal_builder() {
    let mut config = storage::config();
    config.endpoint = "https://minio.example".into();
    config.region = "us-east-1".into();
    config.virtual_host_style = true;
    config.session_token = Some("test-session-token".into());
    config.presign_expiry = Duration::from_secs(123);
    let fixture = storage::fixture(config);
    let ResolvedAsset::Url { url, .. } = fixture
        .store
        .resolve(
            input(AssetDialect::Openai, MediaKind::Image, None),
            data("image/png"),
        )
        .await
        .unwrap()
    else {
        panic!("expected signed URL")
    };
    let uri: http::Uri = url.parse().unwrap();
    assert_eq!(uri.host(), Some("assets.minio.example"));
    assert!(uri.query().unwrap().contains("X-Amz-Expires=123"));
    assert!(
        uri.query()
            .unwrap()
            .contains("X-Amz-Security-Token=test-session-token")
    );
    assert!(
        fixture
            .transport
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|r| r.uri.host() == Some("assets.minio.example"))
    );
}
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
#[tokio::test]
async fn minio_path_style_preserves_the_custom_http_endpoint_and_port() {
    let mut config = storage::config();
    config.endpoint = "http://127.0.0.1:9000".into();
    config.region = "us-east-1".into();
    let fixture = storage::fixture(config);
    let ResolvedAsset::Url { url, .. } = fixture
        .store
        .resolve(
            input(AssetDialect::Openai, MediaKind::Image, None),
            data("image/png"),
        )
        .await
        .unwrap()
    else {
        panic!("expected URL")
    };
    let uri: http::Uri = url.parse().unwrap();
    assert_eq!(uri.scheme_str(), Some("http"));
    assert_eq!(uri.host(), Some("127.0.0.1"));
    assert_eq!(uri.port_u16(), Some(9000));
    assert!(uri.path().starts_with("/assets/images/"));
    assert!(
        fixture
            .transport
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|r| r.uri.port_u16() == Some(9000))
    );
}

#[tokio::test]
async fn presigns_requiring_custom_get_headers_cannot_be_returned_as_plain_vendor_urls() {
    let fixture = storage::fixture(storage::config());
    fixture.probe.require_header.store(true, Ordering::SeqCst);
    let error = fixture
        .store
        .resolve(
            input(AssetDialect::Openai, MediaKind::Image, None),
            data("image/png"),
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidData);
    assert!(error.message.contains("custom headers"));
    assert_eq!(fixture.probe.presigns.load(Ordering::SeqCst), 1);
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

#[tokio::test]
async fn inline_preference_returns_bytes_without_touching_the_store() {
    let fixture = storage::fixture(storage::config());
    let mut request = input(AssetDialect::Openai, MediaKind::Image, None);
    request.preferred = DeliveryMethod::Inline;
    let resolved = fixture
        .store
        .resolve(request, data("image/png"))
        .await
        .unwrap();
    assert!(matches!(resolved, ResolvedAsset::Bytes { .. }));
    assert!(fixture.transport.requests.lock().unwrap().is_empty());
}
