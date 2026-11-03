use super::*;
#[test]
fn unsupported_formats_methods_sizes_and_legacy_file_ids_fail_without_mutating_history() {
    futures_executor::block_on(async {
        let req = request(Dialect::Claude, image(reference(), Some("image/png")));
        let original = serde_json::to_value(&req).unwrap();
        assert_invalid(
            req.resolve_assets(DeliveryMode::PreferUrl, |_| async {
                Ok(bytes("image/tiff"))
            })
            .await,
        );
        assert_eq!(serde_json::to_value(&req).unwrap(), original);
        let req = request(Dialect::Claude, image(reference(), None));
        assert_invalid(
            req.resolve_assets(DeliveryMode::PreferUrl, |_| async {
                Ok(bytes("image/tiff"))
            })
            .await,
        );
        let req = request(
            Dialect::Claude,
            Content::Video(Video {
                source: reference(),
                media_type: Some("video/mp4".into()),
                clip: None,
            }),
        );
        assert_invalid(
            req.resolve_assets(DeliveryMode::PreferUrl, |_| async {
                panic!("unsupported media must fail before resolution")
            })
            .await,
        );
        let req = request(Dialect::Gemini, image(reference(), None));
        assert_invalid(
            req.resolve_assets(DeliveryMode::PreferUrl, |_| async {
                Ok(url(
                    "https://s3.example/huge",
                    Some("image/png"),
                    Some(100_000_001),
                ))
            })
            .await,
        );
        let req = request(
            Dialect::Claude,
            image(
                Asset::ProviderFile {
                    vendor: Vendor::Claude,
                    id: "old-file-id".into(),
                },
                None,
            ),
        );
        assert_invalid(
            req.resolve_assets(DeliveryMode::PreferUrl, |_| async {
                panic!("preparation never uploads or resolves provider IDs")
            })
            .await,
        );
        let req = request(
            Dialect::Claude,
            image(
                Asset::Url {
                    url: "https://caller.example/image".into(),
                },
                Some("image/tiff"),
            ),
        );
        assert_invalid(dialects::render(&req, false));
        let req = request(
            Dialect::Gemini,
            Content::Video(Video {
                source: Asset::Inline { bytes: vec![1] },
                media_type: Some("video/not-supported".into()),
                clip: None,
            }),
        );
        assert_invalid(dialects::render(&req, false));
    });
}

#[test]
fn resolver_errors_are_returned_once_without_retry_or_partial_changes() {
    futures_executor::block_on(async {
        let req = request(Dialect::Openai, image(reference(), None));
        let before = serde_json::to_value(&req).unwrap();
        let error = Error::from_response(
            403,
            &http::HeaderMap::new(),
            "storage resolver denied access".into(),
        );
        let mut calls = 0;
        let result = req
            .resolve_assets(DeliveryMode::PreferUrl, |_| {
                calls += 1;
                let error = error.clone();
                async move { Err(error) }
            })
            .await;
        assert!(matches!(result,Err(e) if e==error));
        assert_eq!(calls, 1);
        assert_eq!(serde_json::to_value(&req).unwrap(), before);
    });
}

#[test]
fn claude_limit_counts_the_complete_encoded_request_not_just_raw_media() {
    let mut req = request(
        Dialect::Claude,
        Content::Text(Text {
            text: "x".repeat(32_000_000),
            refusal: false,
            citations: vec![],
        }),
    );
    assert_invalid(dialects::claude::render(&req, false));
    req.messages[0].content[0] = Block::text("small");
    assert!(dialects::claude::render(&req, false).is_ok());
}
