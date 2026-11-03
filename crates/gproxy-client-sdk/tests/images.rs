#![cfg(all(feature = "images", not(target_arch = "wasm32")))]
#[path = "support/media.rs"]
mod support;
use futures_util::StreamExt;
use gproxy_client_sdk::{images::*, *};
use serde_json::json;
use std::num::NonZeroU32;
use support::*;
fn request(dialect: ImageDialect) -> ImageRequest {
    ImageRequest::new(
        ImageModel {
            id: "provider/model".into(),
            dialect,
        },
        "paint a fox",
    )
}
fn image(source: Asset) -> ContentBlock<Image> {
    ContentBlock::new(Image {
        source,
        media_type: Some("image/png".into()),
        detail: Detail::Auto,
    })
}
#[test]
fn generation_and_edit_choose_native_openai_operations_and_keep_metadata() {
    futures_executor::block_on(async {
        let client = Client::new(Mock::new(vec![
            Reply::json(image_response()),
            Reply::json(image_response()),
        ]));
        let mut req = request(ImageDialect::Openai);
        req.seed = Some(42);
        req.n = NonZeroU32::new(2).unwrap();
        req.size = Some(ImageSize {
            width: NonZeroU32::new(1536).unwrap(),
            height: NonZeroU32::new(864).unwrap(),
        });
        let result = client.generate_images(&req).await.unwrap();
        assert_eq!(result.images.len(), 1);
        assert_eq!(result.response.timestamp, Some(123));
        assert_eq!(result.response.model, None);
        assert_eq!(result.usage.tokens.input_tokens, Some(3));
        assert_eq!(result.usage.total_tokens, Some(7));
        assert_eq!(
            result.images[0].provider_metadata[&Vendor::Openai]["revised_prompt"],
            "expanded prompt"
        );
        assert!(matches!(&result.images[0].content.source,Asset::Inline{bytes}if bytes==&[1,2,3]));
        assert!(result.warnings.iter().any(|w| w.field == "seed"));
        req.images.push(image(Asset::Inline {
            bytes: vec![1, 2, 3],
        }));
        req.mask = Some(image(Asset::Url {
            url: "https://store.example/mask.png".into(),
        }));
        client.generate_images(&req).await.unwrap();
        let sent = client.backend.sent.lock().unwrap();
        assert_eq!(sent[0].key.operation, protocol::Operation::CreateImage);
        assert_eq!(sent[1].key.operation, protocol::Operation::EditImage);
        assert_eq!(sent[1].path, "/v1/images/edits");
        assert_eq!(sent[0].json()["size"], "1536x864");
        assert_eq!(sent[0].json()["n"], 2);
        assert!(sent[0].json().get("seed").is_none());
        assert_eq!(
            sent[1].json()["images"][0]["image_url"],
            "data:image/png;base64,AQID"
        );
        assert_eq!(
            sent[1].json()["mask"]["image_url"],
            "https://store.example/mask.png"
        );
    });
}
#[test]
fn gemini_direct_generation_splits_n_preserves_signatures_and_side_text() {
    futures_executor::block_on(async {
        let native = gemini(vec![
            json!({"inlineData":{"mimeType":"image/png","data":"AQID","extra_blob":true},"thoughtSignature":"image-signature","extra_part":42}),
            json!({"text":"A fox."}),
        ]);
        let client = Client::new(Mock::new(vec![
            Reply::json(native.clone()),
            Reply::json(native),
        ]));
        let mut req = request(ImageDialect::Gemini);
        req.n = NonZeroU32::new(2).unwrap();
        req.aspect_ratio = Some(AspectRatio {
            width: NonZeroU32::new(16).unwrap(),
            height: NonZeroU32::new(9).unwrap(),
        });
        req.resolution = Some(ResolutionTier::K2);
        req.output_format = Some(ImageOutputFormat::Png);
        let result = client.generate_images(&req).await.unwrap();
        assert_eq!(result.images.len(), 2);
        assert_eq!(result.text.len(), 2);
        assert_eq!(result.usage.tokens.input_tokens, Some(6));
        assert_eq!(result.usage.total_tokens, Some(14));
        assert_eq!(result.response.model.as_deref(), Some("served-gemini"));
        assert!(result.warnings.iter().any(|w| w.field == "output_format"));
        let sent = client.backend.sent.lock().unwrap();
        assert_eq!(sent.len(), 2);
        for item in sent.iter() {
            assert_eq!(item.key.operation, protocol::Operation::GenerateContent);
            assert_eq!(item.key.dialect, protocol::Dialect::Gemini);
            assert!(item.path.ends_with(":generateContent"));
            assert_eq!(
                item.json()["generationConfig"]["imageConfig"]["aspectRatio"],
                "16:9"
            );
            assert_eq!(
                item.json()["generationConfig"]["imageConfig"]["imageSize"],
                "2K"
            );
        }
        drop(sent);
        req.images = vec![result.images.into_iter().next().unwrap()];
        req.n = NonZeroU32::MIN;
        let rendered = gproxy_client_sdk::images::gemini::render(&req).unwrap();
        let value = serde_json::to_value(rendered.request).unwrap();
        assert_eq!(
            value["contents"][0]["parts"][1]["thoughtSignature"],
            "image-signature"
        );
        assert_eq!(
            value["contents"][0]["parts"][1]["inlineData"]["extra_blob"],
            true
        );
    });
}
#[test]
fn masks_unsupported_inputs_unknown_dialects_and_future_enums_are_explicit() {
    futures_executor::block_on(async {
        let client = Client::new(Mock::new(vec![]));
        let mut req = request(ImageDialect::Gemini);
        req.mask = Some(image(Asset::Inline { bytes: vec![1] }));
        req.images.push(image(Asset::Inline { bytes: vec![1] }));
        assert!(client.generate_images(&req).await.is_err());
        let mut req = request(ImageDialect::Openai);
        req.images.push(image(Asset::ProviderFile {
            vendor: Vendor::Openai,
            id: "old-file".into(),
        }));
        assert!(client.generate_images(&req).await.is_err());
        req.model.dialect = serde_json::from_value(json!("claude")).unwrap();
        assert!(client.generate_images(&req).await.is_err());
        assert!(client.backend.sent.lock().unwrap().is_empty());
        for value in [json!("future_format"), json!("future_tier")] {
            if value == "future_format" {
                let parsed: ImageOutputFormat = serde_json::from_value(value.clone()).unwrap();
                assert_eq!(serde_json::to_value(parsed).unwrap(), value);
            } else {
                let parsed: ResolutionTier = serde_json::from_value(value.clone()).unwrap();
                assert_eq!(serde_json::to_value(parsed).unwrap(), value);
            }
        }
    });
}
#[test]
fn preparation_uses_operation_capabilities_and_never_persists_signed_urls() {
    futures_executor::block_on(async {
        let mut req = request(ImageDialect::Openai);
        req.images.push(image(Asset::Reference {
            reference: "blob".into(),
        }));
        let before = serde_json::to_value(&req).unwrap();
        for generation in 1..=2 {
            let prepared = req
                .resolve_assets(DeliveryMode::PreferUrl, |input| async move {
                    assert_eq!(input.target, AssetDialect::OpenaiImages);
                    assert_eq!(input.location, AssetLocation::ImageInput);
                    Ok(ResolvedAsset::Url {
                        url: format!("https://store.example/image?signature={generation}"),
                        media_type: Some("image/png".into()),
                        byte_len: Some(3),
                    })
                })
                .await
                .unwrap();
            let client = Client::new(Mock::new(vec![Reply::json(image_response())]));
            client.generate_images_prepared(prepared).await.unwrap();
            assert_eq!(
                client.backend.sent.lock().unwrap()[0].json()["images"][0]["image_url"],
                format!("https://store.example/image?signature={generation}")
            );
            assert_eq!(serde_json::to_value(&req).unwrap(), before);
        }
    });
}
#[test]
fn dimensions_native_precedence_safety_and_partial_streaming_are_reported() {
    futures_executor::block_on(async {
        let mut req = request(ImageDialect::Gemini);
        req.aspect_ratio = Some(AspectRatio {
            width: NonZeroU32::new(5).unwrap(),
            height: NonZeroU32::new(7).unwrap(),
        });
        let rendered = gproxy_client_sdk::images::gemini::render(&req).unwrap();
        assert!(rendered.warnings.iter().any(|w| w.field == "aspect_ratio"));
        let blocked = json!({"responseId":"blocked","promptFeedback":{"blockReason":"SAFETY"},"usageMetadata":{"promptTokenCount":3}});
        let client = Client::new(Mock::new(vec![Reply::json(blocked)]));
        let result = client.generate_images(&req).await.unwrap();
        assert!(result.images.is_empty());
        assert_eq!(
            result.finish_reason.unified,
            FinishReasonKind::ContentBlocked
        );
        let client = Client::new(Mock::new(vec![Reply::sse(vec![
            image_event(false, true),
            image_event(false, false),
        ])]));
        let stream = client
            .stream_images(&request(ImageDialect::Openai))
            .await
            .unwrap();
        let events = stream.collect::<Vec<_>>().await;
        assert!(matches!(&events[0], ImageEvent::Start { .. }));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, ImageEvent::PartialImage { index: 0, .. }))
        );
        assert!(
            matches!(events.last(),Some(ImageEvent::Finish{result})if result.images.len()==1&&result.usage.total_tokens==Some(7))
        );
    });
}
#[test]
fn image_stream_errors_keep_status_body_and_gemini_warns_about_single_completion() {
    futures_executor::block_on(async {
        let error = json!({"type":"error","code":"rate_limit_exceeded","message":"later"});
        let client = Client::new(Mock::new(vec![Reply::sse(vec![error.clone()])]));
        let events = client
            .stream_images(&request(ImageDialect::Openai))
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        assert!(events.iter().any(|e|matches!(e,ImageEvent::Error{error:e}if e.kind==ErrorKind::RateLimit&&e.status==Some(200)&&e.body.as_deref()==Some(error.to_string().as_str()))));
        assert!(
            matches!(events.last(),Some(ImageEvent::Finish{result})if result.finish_reason.unified==FinishReasonKind::Error)
        );
        let native = gemini(vec![
            json!({"inlineData":{"mimeType":"image/png","data":"AQID"}}),
        ]);
        let client = Client::new(Mock::new(vec![Reply::json(native)]));
        let events = client
            .stream_images(&request(ImageDialect::Gemini))
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        assert_eq!(events.len(), 2);
        assert!(
            matches!(&events[0],ImageEvent::Start{warnings}if warnings.iter().any(|w|w.field=="stream"))
        );
    });
}

#[cfg(feature = "s3")]
#[path = "support/s3.rs"]
mod store;
#[cfg(feature = "s3")]
#[tokio::test]
async fn configured_s3_image_client_presigns_without_persisting_the_delivery_url() {
    let fixture = store::fixture(store::config());
    let client = Client::new(Mock::new(vec![Reply::json(image_response())])).with_s3(fixture.store);
    let mut req = request(ImageDialect::Openai);
    req.images.push(image(Asset::Reference {
        reference: "input-blob".into(),
    }));
    let before = serde_json::to_value(&req).unwrap();
    client
        .generate_images(&req, |id| async move {
            assert_eq!(id, "input-blob");
            Ok(s3::AssetData {
                bytes: vec![1, 2, 3],
                media_type: "image/png".into(),
            })
        })
        .await
        .unwrap();
    assert!(
        client.client.backend.sent.lock().unwrap()[0].json()["images"][0]["image_url"]
            .as_str()
            .unwrap()
            .contains("X-Amz-Signature=")
    );
    assert_eq!(serde_json::to_value(&req).unwrap(), before);
    assert_eq!(
        fixture
            .probe
            .presigns
            .load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    let recorded = fixture.transport.requests.lock().unwrap();
    assert!(
        recorded
            .iter()
            .any(|r| r.method == http::Method::PUT && r.mime.as_deref() == Some("image/png"))
    );
    assert!(
        recorded
            .iter()
            .all(|r| r.uri.host() == Some("account.r2.cloudflarestorage.com"))
    );
}
