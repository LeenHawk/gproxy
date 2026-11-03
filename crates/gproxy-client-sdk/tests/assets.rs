#![cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
mod support;
use futures_util::StreamExt;
use gproxy_client_sdk::*;
use serde_json::Value;
use std::{cell::Cell, rc::Rc, sync::Mutex};

fn reference() -> Asset {
    Asset::Reference {
        reference: "blob:sha256".into(),
    }
}
fn image(source: Asset, mime: Option<&str>) -> Content {
    Content::Image(Image {
        source,
        media_type: mime.map(str::to_owned),
        detail: Detail::Auto,
    })
}
fn request(dialect: Dialect, content: Content) -> GenerateRequest {
    support::request(
        dialect,
        vec![Message {
            role: Role::User,
            content: vec![Block::new(content)],
        }],
    )
}
fn url(url: &str, mime: Option<&str>, size: Option<u64>) -> ResolvedAsset {
    ResolvedAsset::Url {
        url: url.into(),
        media_type: mime.map(str::to_owned),
        byte_len: size,
    }
}
fn bytes(mime: &str) -> ResolvedAsset {
    ResolvedAsset::Bytes {
        bytes: vec![1, 2, 3],
        media_type: mime.into(),
    }
}
fn assert_invalid<T>(result: Result<T>) {
    assert!(matches!(
        result,
        Err(Error {
            kind: ErrorKind::InvalidData,
            ..
        })
    ));
}
#[test]
fn delivery_table_matches_the_four_native_shapes_and_embedding_modes() {
    use AssetDialect as D;
    use MediaKind as K;
    let kinds = [K::Image, K::Document, K::Audio, K::Video, K::File];
    let cases = [
        (
            D::Claude,
            [
                (true, true),
                (true, true),
                (false, false),
                (false, false),
                (false, false),
            ],
        ),
        (
            D::Openai,
            [
                (true, true),
                (true, true),
                (false, false),
                (false, false),
                (true, true),
            ],
        ),
        (
            D::OpenaiChat,
            [
                (true, true),
                (true, false),
                (true, false),
                (false, false),
                (true, false),
            ],
        ),
        (D::Gemini, [(true, true); 5]),
        (D::OpenaiEmbedding, [(false, false); 5]),
        (
            D::GeminiEmbedding,
            [
                (true, true),
                (false, false),
                (false, false),
                (false, false),
                (false, false),
            ],
        ),
    ];
    for (target, expected) in cases {
        for (kind, (inline, url)) in kinds.into_iter().zip(expected) {
            let cap = media_capability(target, kind);
            assert_eq!(
                (cap.inline.is_some(), cap.url.is_some()),
                (inline, url),
                "{target:?}/{kind:?}"
            );
            assert_eq!(
                cap.preferred(DeliveryMode::PreferUrl),
                if url {
                    Some(DeliveryMethod::Url)
                } else if inline {
                    Some(DeliveryMethod::Inline)
                } else {
                    None
                }
            );
        }
    }
    let claude = media_capability(D::Claude, K::Image);
    assert_eq!(
        claude.inline.unwrap().media_types,
        ["image/jpeg", "image/png", "image/gif", "image/webp"]
    );
    assert_eq!(claude.max_request_bytes, Some(32_000_000));
    assert_eq!(claude.inline.unwrap().max_bytes, None); // No per-image cap in local references.
    let openai = media_capability(D::Openai, K::File);
    assert_eq!(openai.inline.unwrap().max_bytes, None);
    assert!(!openai.inline.unwrap().exhaustive); // Do not invent a MIME whitelist.
    let video = media_capability(D::Gemini, K::Video);
    assert_eq!(video.inline.unwrap().max_bytes, Some(100_000_000));
    assert!(video.inline.unwrap().exclusive);
    assert!(!video.url.unwrap().exclusive);
}
#[test]
fn fresh_url_snapshots_leave_request_and_persisted_history_unchanged() {
    futures_executor::block_on(async {
        let request = request(Dialect::Openai, image(reference(), Some("image/png")));
        let original = serde_json::to_value(&request).unwrap();
        let history = Conversation::new(request.messages.clone());
        let history_value = serde_json::to_value(&history).unwrap();
        let generations = Rc::new(Cell::new(0));
        for generation in 1..=2 {
            // This future intentionally holds Rc across await: no Send bound on wasm resolvers.
            let count = generations.clone();
            let prepared = request
                .resolve_assets(DeliveryMode::PreferUrl, move |input| {
                    let count = count.clone();
                    async move {
                        std::future::ready(()).await;
                        count.set(count.get() + 1);
                        assert_eq!(input.reference, "blob:sha256");
                        assert_eq!(input.target, AssetDialect::Openai);
                        assert_eq!(input.kind, MediaKind::Image);
                        assert_eq!(input.location, AssetLocation::Message);
                        assert_eq!(input.preferred, DeliveryMethod::Url);
                        assert!(
                            input.capability.inline.is_some() && input.capability.url.is_some()
                        );
                        Ok(url(
                            &format!("https://s3.example/blob?signature={}", count.get()),
                            Some("image/png"),
                            Some(3),
                        ))
                    }
                })
                .await
                .unwrap();
            assert!(prepared.warnings().is_empty());
            let native = prepared.render(false).unwrap().request;
            assert_eq!(
                native["input"][0]["content"][0]["image_url"],
                format!("https://s3.example/blob?signature={generation}")
            );
            assert_eq!(serde_json::to_value(&request).unwrap(), original);
            assert_eq!(serde_json::to_value(&history).unwrap(), history_value);
            assert!(
                !serde_json::to_string(&history)
                    .unwrap()
                    .contains("signature=")
            );
        }
        assert_eq!(generations.get(), 2);
    });
}
#[test]
fn inline_fallback_warns_but_plain_inline_and_urls_are_not_rewritten() {
    futures_executor::block_on(async {
        let req = request(Dialect::Claude, image(reference(), None));
        let prepared = req
            .resolve_assets(DeliveryMode::PreferUrl, |input| async move {
                assert_eq!(input.preferred, DeliveryMethod::Url);
                Ok(bytes("image/png"))
            })
            .await
            .unwrap();
        assert_eq!(prepared.warnings().len(), 1);
        assert_eq!(prepared.warnings()[0].kind, WarningKind::Compatibility);
        let native = prepared.render(false).unwrap();
        assert_eq!(
            native.request["messages"][0]["content"][0]["source"]["data"],
            "AQID"
        );
        assert_eq!(native.warnings.len(), 1);
        for source in [
            Asset::Inline {
                bytes: vec![1, 2, 3],
            },
            Asset::Url {
                url: "https://caller.example/plain".into(),
            },
        ] {
            let req = request(Dialect::Claude, image(source, Some("image/png")));
            let original = dialects::render(&req, false).unwrap().request;
            let prepared = req
                .resolve_assets(DeliveryMode::PreferUrl, |_| async {
                    panic!("direct values must not call the resolver")
                })
                .await
                .unwrap();
            assert!(prepared.warnings().is_empty());
            assert_eq!(prepared.render(false).unwrap().request, original);
        }
    });
}
#[test]
fn chat_audio_and_files_choose_inline_and_cannot_accept_a_url() {
    futures_executor::block_on(async {
        let audio = Content::Audio(Audio {
            source: reference(),
            format: AudioFormat::Encoded {
                media_type: "audio/wav".into(),
            },
            transcript: None,
        });
        let req = request(Dialect::OpenaiChat, audio);
        let prepared = req
            .resolve_assets(DeliveryMode::PreferUrl, |input| async move {
                assert_eq!(input.kind, MediaKind::Audio);
                assert_eq!(input.preferred, DeliveryMethod::Inline);
                assert!(input.capability.url.is_none());
                Ok(bytes("audio/wav"))
            })
            .await
            .unwrap();
        assert!(prepared.warnings().is_empty());
        assert_eq!(
            prepared.render(false).unwrap().request["messages"][0]["content"][0]["input_audio"]["format"],
            "wav"
        );
        assert_invalid(
            req.resolve_assets(DeliveryMode::PreferUrl, |_| async {
                Ok(url("https://s3.example/audio", Some("audio/wav"), None))
            })
            .await,
        );
        let req = request(
            Dialect::OpenaiChat,
            Content::File(File {
                source: reference(),
                media_type: Some("text/plain".into()),
                filename: Some("note.txt".into()),
            }),
        );
        let prepared = req
            .resolve_assets(DeliveryMode::PreferUrl, |input| async move {
                assert_eq!(input.kind, MediaKind::File);
                assert!(input.capability.url.is_none());
                Ok(bytes("text/plain"))
            })
            .await
            .unwrap();
        assert_eq!(
            prepared.render(false).unwrap().request["messages"][0]["content"][0]["file"]["file_data"],
            "data:text/plain;base64,AQID"
        );
    });
}
#[test]
fn document_mime_narrows_the_method_and_all_five_kinds_reach_the_resolver() {
    futures_executor::block_on(async {
        let req = request(
            Dialect::Claude,
            Content::Document(Document {
                source: DocumentSource::Asset {
                    source: reference(),
                },
                media_type: Some("text/plain".into()),
                title: None,
                context: None,
                citations: None,
                detail: Detail::Auto,
            }),
        );
        let prepared = req
            .resolve_assets(DeliveryMode::PreferUrl, |input| async move {
                assert_eq!(input.kind, MediaKind::Document);
                assert!(input.capability.url.is_none()); // Claude URL source is a PDF source.
                assert_eq!(input.preferred, DeliveryMethod::Inline);
                Ok(ResolvedAsset::Bytes {
                    bytes: b"hello".to_vec(),
                    media_type: "text/plain".into(),
                })
            })
            .await
            .unwrap();
        assert_eq!(
            prepared.render(false).unwrap().request["messages"][0]["content"][0]["source"]["data"],
            "hello"
        );
        let kinds = [
            MediaKind::Image,
            MediaKind::Document,
            MediaKind::Audio,
            MediaKind::Video,
            MediaKind::File,
        ];
        let contents = [
            image(reference(), Some("image/png")),
            Content::Document(Document {
                source: DocumentSource::Asset {
                    source: reference(),
                },
                media_type: Some("application/pdf".into()),
                title: None,
                context: None,
                citations: None,
                detail: Detail::Auto,
            }),
            Content::Audio(Audio {
                source: reference(),
                format: AudioFormat::Encoded {
                    media_type: "audio/wav".into(),
                },
                transcript: None,
            }),
            Content::Video(Video {
                source: reference(),
                media_type: Some("video/mp4".into()),
                clip: None,
            }),
            Content::File(File {
                source: reference(),
                media_type: Some("text/csv".into()),
                filename: None,
            }),
        ];
        for (kind, content) in kinds.into_iter().zip(contents) {
            let req = request(Dialect::Gemini, content);
            let prepared = req
                .resolve_assets(DeliveryMode::PreferUrl, |input| async move {
                    assert_eq!(input.kind, kind);
                    assert_eq!(input.target, AssetDialect::Gemini);
                    assert_eq!(input.preferred, DeliveryMethod::Url);
                    Ok(url(
                        "https://s3.example/media",
                        input.media_type.as_deref(),
                        Some(12),
                    ))
                })
                .await
                .unwrap();
            assert_eq!(
                prepared.render(false).unwrap().request["contents"][0]["parts"][0]["fileData"]["fileUri"],
                "https://s3.example/media"
            );
        }
    });
}
#[test]
fn tool_result_envelopes_narrow_gemini_to_inline_and_reject_chat_media() {
    futures_executor::block_on(async {
        let result = Content::ToolResult(ToolResult {
            id: "call".into(),
            name: "lookup".into(),
            output: ToolOutput::Blocks(vec![ContentBlock::new(ToolResultContent::Image(Image {
                source: reference(),
                media_type: Some("image/png".into()),
                detail: Detail::Auto,
            }))]),
            is_error: false,
            provider_executed: false,
        });
        let req = request(Dialect::Gemini, result.clone());
        let prepared = req
            .resolve_assets(DeliveryMode::PreferUrl, |input| async move {
                assert_eq!(input.location, AssetLocation::ToolResult);
                assert_eq!(input.preferred, DeliveryMethod::Inline);
                assert!(input.capability.url.is_none());
                Ok(bytes("image/png"))
            })
            .await
            .unwrap();
        let native = prepared.render(false).unwrap().request;
        assert_eq!(
            native["contents"][0]["parts"][0]["functionResponse"]["parts"][0]["inlineData"]["data"],
            "AQID"
        );
        assert_invalid(
            req.resolve_assets(DeliveryMode::PreferUrl, |_| async {
                Ok(url("https://s3.example/image", None, None))
            })
            .await,
        );
        let req = request(Dialect::OpenaiChat, result);
        assert_invalid(
            req.resolve_assets(DeliveryMode::PreferUrl, |_| async {
                panic!("Chat tool messages have no media delivery")
            })
            .await,
        );
    });
}
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

struct Script {
    sent: Mutex<Vec<Value>>,
    stream: bool,
}
impl Backend for Script {
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
            let raw = protocol::codec::read_http_body(request.body, DEFAULT_LIMITS)
                .await
                .unwrap();
            self.sent
                .lock()
                .unwrap()
                .push(serde_json::from_slice(&raw).unwrap());
            let native = support::openai();
            let body = if self.stream {
                support::openai_events(&native)
                    .into_iter()
                    .map(|event| format!("data: {event}\n\n"))
                    .collect::<String>()
            } else {
                native.to_string()
            };
            Ok(protocol::WireResponse {
                status: http::StatusCode::OK,
                headers: http::HeaderMap::new(),
                body: protocol::HttpBody::Bytes(body.into_bytes().into()),
            })
        })
    }
}
#[test]
fn prepared_generation_sends_only_native_media_and_attaches_fallback_warnings_to_results_and_streams()
 {
    futures_executor::block_on(async {
        let req = request(Dialect::Openai, image(reference(), Some("image/png")));
        for streaming in [false, true] {
            let client = Client::new(Script {
                sent: Mutex::new(vec![]),
                stream: streaming,
            });
            let prepared = req
                .resolve_assets(DeliveryMode::PreferUrl, |_| async {
                    Ok(bytes("image/png"))
                })
                .await
                .unwrap();
            let warnings = prepared.warnings().to_vec();
            assert_eq!(warnings.len(), 1);
            if streaming {
                let mut stream = client.stream_prepared(prepared).await.unwrap();
                assert!(
                    matches!(stream.next().await,Some(Event::Start{warnings:w}) if w==warnings)
                );
                let events = stream.collect::<Vec<_>>().await;
                assert!(events.iter().any(|e| matches!(e, Event::Finish { .. })));
            } else {
                let result = client.generate_prepared(prepared).await.unwrap();
                assert_eq!(result.warnings, warnings);
            }
            let sent = client.backend.sent.lock().unwrap();
            assert_eq!(sent.len(), 1);
            assert_eq!(
                sent[0]["input"][0]["content"][0]["image_url"],
                "data:image/png;base64,AQID"
            );
            assert!(!sent[0].to_string().contains("blob:sha256"));
        }
    });
}
#[test]
fn inline_mode_is_the_default_and_asks_for_bytes_without_fallback_warnings() {
    assert_eq!(DeliveryMode::default(), DeliveryMode::Inline);
    futures_executor::block_on(async {
        // Claude images accept both inline and URL; Inline mode must not prefer the URL.
        let req = request(Dialect::Claude, image(reference(), None));
        let prepared = req
            .resolve_assets(DeliveryMode::Inline, |input| async move {
                assert_eq!(input.preferred, DeliveryMethod::Inline);
                Ok(bytes("image/png"))
            })
            .await
            .unwrap();
        assert!(prepared.warnings().is_empty());
        let native = prepared.render(false).unwrap();
        assert_eq!(
            native.request["messages"][0]["content"][0]["source"]["data"],
            "AQID"
        );

        // A resolver that still returns a URL is honored, but the deviation is reported.
        let prepared = req
            .resolve_assets(DeliveryMode::Inline, |_| async {
                Ok(url("https://s3.example/image", Some("image/png"), None))
            })
            .await
            .unwrap();
        assert_eq!(prepared.warnings().len(), 1);
        assert_eq!(prepared.warnings()[0].kind, WarningKind::Compatibility);
    });
}
#[test]
fn preferred_method_follows_the_mode_and_falls_back_to_what_the_target_accepts() {
    let both = media_capability(AssetDialect::Claude, MediaKind::Image);
    assert_eq!(
        both.preferred(DeliveryMode::Inline),
        Some(DeliveryMethod::Inline)
    );
    assert_eq!(
        both.preferred(DeliveryMode::PreferUrl),
        Some(DeliveryMethod::Url)
    );

    let mut url_only = both;
    url_only.inline = None;
    assert_eq!(
        url_only.preferred(DeliveryMode::Inline),
        Some(DeliveryMethod::Url)
    );
    let mut inline_only = both;
    inline_only.url = None;
    assert_eq!(
        inline_only.preferred(DeliveryMode::PreferUrl),
        Some(DeliveryMethod::Inline)
    );
}
