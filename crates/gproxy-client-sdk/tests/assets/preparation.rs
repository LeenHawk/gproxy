use super::*;

#[cfg(feature = "gemini")]
#[test]
fn caller_blob_references_are_resolved_explicitly() {
    let request = support::request(
        Dialect::Gemini,
        vec![Message {
            role: Role::User,
            content: vec![Block::new(Content::Image(Image {
                source: Asset::Reference {
                    reference: "sha256:blob".into(),
                },
                media_type: None,
                detail: Detail::Auto,
            }))],
        }],
    );
    assert!(dialects::render(&request, false).is_err());
    let prepared = futures_executor::block_on(request.resolve_assets(
        DeliveryMode::PreferUrl,
        |reference| async move {
            assert_eq!(reference.reference, "sha256:blob");
            Ok(ResolvedAsset::Bytes {
                media_type: "image/png".into(),
                bytes: vec![1, 2, 3],
            })
        },
    ))
    .unwrap();
    assert_eq!(
        prepared.render(false).unwrap().request["contents"][0]["parts"][0]["inlineData"]["data"],
        "AQID"
    );
}

#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
#[test]
fn caller_reference_resolution_covers_nested_media_without_uploading() {
    let image = Image {
        source: Asset::Reference {
            reference: "image".into(),
        },
        media_type: None,
        detail: Detail::Low,
    };
    let doc = Document {
        source: DocumentSource::Chunks {
            chunks: vec![ContentBlock::new(ChunkContent::Image(image))],
        },
        media_type: None,
        title: None,
        context: None,
        citations: None,
        detail: Detail::Auto,
    };
    let request = support::request(
        Dialect::Claude,
        vec![Message {
            role: Role::Tool,
            content: vec![Block::new(Content::ToolResult(ToolResult {
                id: "call_1".into(),
                name: "f".into(),
                output: ToolOutput::Blocks(vec![ContentBlock::new(ToolResultContent::Document(
                    doc,
                ))]),
                is_error: false,
                provider_executed: false,
            }))],
        }],
    );
    let mut calls = 0;
    let prepared = futures_executor::block_on(request.resolve_assets(
        DeliveryMode::PreferUrl,
        |reference| {
            calls += 1;
            assert_eq!(reference.reference, "image");
            async {
                Ok(ResolvedAsset::Bytes {
                    bytes: vec![1, 2, 3],
                    media_type: "image/png".into(),
                })
            }
        },
    ))
    .unwrap();
    assert_eq!(calls, 1);
    let native = prepared.render(false).unwrap().request;
    assert_eq!(
        native["messages"][0]["content"][0]["content"][0]["source"]["content"][0]["source"]["data"],
        "AQID"
    );
    assert!(request.messages[0].content[0].provider_metadata.is_empty());
}
