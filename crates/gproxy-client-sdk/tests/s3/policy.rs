use super::*;
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
