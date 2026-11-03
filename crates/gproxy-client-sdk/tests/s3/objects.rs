use super::*;
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
