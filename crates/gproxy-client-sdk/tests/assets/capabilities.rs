use super::*;
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
