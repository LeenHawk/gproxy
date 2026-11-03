use super::*;
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
#[test]
fn separate_media_fields_and_request_settings_reach_native_dialects() {
    let mut text = Block::text("read");
    text.cache = Some(CacheBreakpoint {
        ttl: Some(CacheTtl::FiveMinutes),
    });
    let image = Block::new(Content::Image(Image {
        source: Asset::Url {
            url: "https://example.com/x.png".into(),
        },
        media_type: Some("image/png".into()),
        detail: Detail::Low,
    }));
    let doc = Block::new(Content::Document(document(DocumentSource::Asset {
        source: Asset::Inline {
            bytes: b"%PDF".to_vec(),
        },
    })));
    let request = support::request(
        Dialect::Claude,
        vec![Message {
            role: Role::User,
            content: vec![text, image, doc],
        }],
    );
    let native = dialects::render(&request, false).unwrap().request;
    assert_eq!(native["messages"][0]["content"][2]["type"], "document");
    assert_eq!(native["messages"][0]["content"][2]["title"], "Report");
    assert_eq!(
        native["messages"][0]["content"][2]["citations"]["enabled"],
        true
    );
    let mut request = request;
    request.model.dialect = Dialect::Openai;
    request.messages[0].content[0].cache = Some(CacheBreakpoint {
        ttl: Some(CacheTtl::ThirtyMinutes),
    });
    let native = dialects::render(&request, false).unwrap().request;
    assert_eq!(
        native["input"][0]["content"][0]["prompt_cache_breakpoint"]["ttl"],
        "30m"
    );
    assert_eq!(native["input"][0]["content"][1]["detail"], "low");
    assert_eq!(native["input"][0]["content"][2]["type"], "input_file");
    assert_eq!(native["input"][0]["content"][2]["detail"], "high");
}
