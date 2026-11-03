use super::*;

#[cfg(all(feature = "claude", feature = "openai", feature = "gemini"))]
#[test]
fn crossing_vendors_drops_private_metadata_and_options() {
    let results = [
        (
            Dialect::Claude,
            dialects::claude::parse(serde_json::from_value(support::claude()).unwrap()).unwrap(),
        ),
        (
            Dialect::Openai,
            dialects::openai::parse(serde_json::from_value(support::openai()).unwrap()).unwrap(),
        ),
        (
            Dialect::Gemini,
            dialects::gemini::parse(serde_json::from_value(support::gemini()).unwrap()).unwrap(),
        ),
    ];
    for (source, result) in results {
        for target in [Dialect::Claude, Dialect::Openai, Dialect::Gemini] {
            if source == target {
                continue;
            }
            let mut request = support::request(target, vec![result.clone().into_message()]);
            // Generic attachments cannot be represented by Claude's image/document API.
            if target == Dialect::Claude {
                request.messages[0]
                    .content
                    .retain(|b| !matches!(b.content, Content::File(_)));
            }
            support::extensions(&mut request.provider_options, source.vendor())
                .insert("secret_option".into(), json!("private"));
            let rendered = dialects::render(&request, false)
                .unwrap_or_else(|error| panic!("{source:?} -> {target:?}: {error}"))
                .request
                .to_string();
            for private in [
                "signature",
                "encrypted",
                "ciphertext",
                "\"future\":",
                "opaque_extra",
                "envelope_extra",
                "secret_option",
            ] {
                assert!(!rendered.contains(private), "leaked {private}: {rendered}");
            }
        }
    }
}

#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    not(target_arch = "wasm32")
))]
#[test]
fn warnings_cover_detail_ttl_foreign_metadata_ignored_document_fields_and_legacy_controls() {
    let mut block = Block::new(Content::Document(Document {
        source: DocumentSource::Asset {
            source: Asset::Url {
                url: "https://example.com/doc.pdf".into(),
            },
        },
        media_type: Some("application/pdf".into()),
        title: None,
        context: Some("context".into()),
        citations: Some(true),
        detail: Detail::Original,
    }));
    block.cache = Some(CacheBreakpoint {
        ttl: Some(CacheTtl::FiveMinutes),
    });
    block.provider_metadata.insert(
        Vendor::Claude,
        serde_json::from_value(json!({"native":{"signature":"private"}})).unwrap(),
    );
    let request = support::request(
        Dialect::Openai,
        vec![Message {
            role: Role::User,
            content: vec![block],
        }],
    );
    let compiled = dialects::render(&request, false).unwrap();
    for field in [
        ".detail",
        ".cache.ttl",
        ".provider_metadata",
        ".context",
        ".citations",
    ] {
        assert!(
            compiled.warnings.iter().any(|w| w.field.ends_with(field)),
            "missing warning for {field}: {:?}",
            compiled.warnings
        );
    }
    assert!(!compiled.request.to_string().contains("private"));
    let mut claude = support::request(
        Dialect::Claude,
        vec![Message {
            role: Role::User,
            content: vec![Block::new(Content::Image(Image {
                source: Asset::Url {
                    url: "https://example.com/image".into(),
                },
                media_type: None,
                detail: Detail::High,
            }))],
        }],
    );
    claude.settings.temperature = Some(0.3);
    claude.settings.speech = Some(SpeechSettings {
        voice: "voice".into(),
        format: AudioFormat::pcm(24000, 1),
    });
    let warnings = dialects::claude::render(&claude, false).unwrap().warnings;
    assert!(
        warnings
            .iter()
            .any(|w| w.kind == WarningKind::Deprecated && w.field == "settings.temperature")
    );
    assert!(warnings.iter().any(|w| w.field == "settings.speech"));
    assert!(warnings.iter().any(|w| w.field.ends_with(".detail")));
    let mut gemini = support::request(
        Dialect::Gemini,
        vec![Message {
            role: Role::User,
            content: vec![Block::text("cache")],
        }],
    );
    gemini.messages[0].content[0].cache = Some(CacheBreakpoint { ttl: None });
    assert!(
        dialects::gemini::render(&gemini)
            .unwrap()
            .warnings
            .iter()
            .any(|w| w.field.ends_with(".cache"))
    );
}
