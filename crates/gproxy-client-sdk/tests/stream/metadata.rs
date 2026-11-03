use super::*;
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    not(target_arch = "wasm32")
))]
#[test]
fn result_and_stream_expose_reported_identity_time_and_early_warnings() {
    futures_executor::block_on(async {
        let native = answer();
        let client = Client::new(Script::new(vec![
            Reply::json(native.clone()),
            Reply::sse(early_events(&native)),
        ]));
        let mut request = support::request(
            Dialect::Openai,
            vec![Message {
                role: Role::User,
                content: vec![Block::new(Content::Image(Image {
                    source: Asset::Url {
                        url: "https://example.com/image".into(),
                    },
                    media_type: None,
                    detail: Detail::Medium,
                }))],
            }],
        );
        let result = client.generate(&request).await.unwrap();
        assert_eq!(result.response.id.as_deref(), Some("response_served"));
        assert_eq!(result.response.model.as_deref(), Some("served_model"));
        assert_eq!(result.response.timestamp, Some(1770000000));
        assert_eq!(result.finish_reason.unified, FinishReasonKind::Stop);
        assert_eq!(result.finish_reason.raw.as_deref(), Some("completed"));
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.field.ends_with(".detail") && w.kind == WarningKind::Compatibility)
        );
        let mut stream = client.stream(&request).await.unwrap();
        let first = stream.next().await.unwrap();
        assert!(matches!(&first,Event::Start {warnings} if warnings==&result.warnings));
        let second = stream.next().await.unwrap();
        assert!(matches!(&second,Event::ResponseMetadata {metadata} if metadata==&result.response));
        let mut events = vec![first, second];
        events.extend(stream.collect::<Vec<_>>().await);
        let aggregate = gproxy_client_sdk::aggregate(futures_util::stream::iter(events))
            .await
            .unwrap();
        assert_eq!(aggregate, result);
        request
            .provider_options
            .openai
            .responses
            .previous_response_id = result.response.id.map(Some);
        assert_eq!(
            dialects::openai::render(&request, false)
                .unwrap()
                .request
                .previous_response_id,
            Some(Some("response_served".into()))
        );
        let sent = client.backend.requests.lock().unwrap();
        assert_eq!(sent[0]["input"][0]["content"][0]["detail"], "auto");
        assert!(sent[0].get("warnings").is_none());
    });
}

#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    not(target_arch = "wasm32")
))]
#[test]
fn claude_and_gemini_metadata_do_not_invent_timestamps_or_requested_models() {
    let mut claude = support::claude();
    claude["model"] = json!("claude_served");
    let parsed = dialects::claude::parse(serde_json::from_value(claude.clone()).unwrap()).unwrap();
    assert_eq!(parsed.response.model.as_deref(), Some("claude_served"));
    assert_eq!(parsed.response.timestamp, None);
    assert_eq!(
        support::aggregate(support::decode(
            Dialect::Claude,
            support::claude_events(&claude)
        )),
        parsed
    );
    let first = json!({"modelVersion":"gemini_served","responseId":"gemini_response","candidates":[{"content":{"parts":[{"text":"hello"}]}}]});
    let last = json!({"candidates":[{"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":1}});
    let mut decoder = stream::StreamDecoder::new(Dialect::Gemini);
    let mut events = decoder.push_json(&first.to_string()).unwrap();
    events.extend(decoder.push_json(&last.to_string()).unwrap());
    events.extend(decoder.finish().unwrap());
    let parsed = support::aggregate(events);
    assert_eq!(parsed.response.id.as_deref(), Some("gemini_response"));
    assert_eq!(parsed.response.model.as_deref(), Some("gemini_served"));
    assert_eq!(parsed.response.timestamp, None);
    assert_eq!(parsed.finish_reason.raw.as_deref(), Some("STOP"));
}

#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    not(target_arch = "wasm32")
))]
#[test]
fn retrieved_sources_do_not_require_inline_anchors() {
    let native = json!({"candidates":[{"content":{"parts":[{"text":"answer"}]},"finishReason":"STOP","groundingMetadata":{"groundingChunks":[{"web":{"uri":"https://example.com","title":"Web"}},{"retrievedContext":{"title":"Unlocated","text":"snippet"}},{"retrievedContext":{"uri":"https://example.com/doc","title":"Doc","text":"document text","pageNumber":1}}]}}]});
    let parsed = dialects::gemini::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    assert_eq!(parsed.sources.len(), 3);
    assert!(parsed.sources[1].source.is_none());
    assert_eq!(parsed.sources[1].text.as_deref(), Some("snippet"));
    assert!(
        matches!(&parsed.content[0].content,Content::Text(Text {citations,..}) if citations.is_empty())
    );
    let events = support::decode(Dialect::Gemini, vec![native]);
    assert_eq!(support::aggregate(events), parsed);
    let mut claude = support::claude();
    claude["content"] = json!([{"type":"web_search_tool_result","tool_use_id":"search","content":[{"type":"web_search_result","encrypted_content":"cipher","title":"Retrieved","url":"https://example.com"}]}]);
    assert_eq!(
        dialects::claude::parse(serde_json::from_value(claude).unwrap())
            .unwrap()
            .sources
            .len(),
        1
    );
    let mut openai = answer();
    openai["output"] = json!([{"type":"web_search_call","id":"s1","status":"completed","action":{"type":"search","query":"q","sources":[{"type":"url","url":"https://example.com"}]}},{"type":"file_search_call","id":"f1","status":"completed","queries":["q"],"results":[{"text":"unlocated file snippet"}]}]);
    let parsed = dialects::openai::parse(serde_json::from_value(openai.clone()).unwrap()).unwrap();
    assert_eq!(parsed.sources.len(), 2);
    assert_eq!(
        parsed.sources[1].text.as_deref(),
        Some("unlocated file snippet")
    );
    assert_eq!(
        support::aggregate(support::decode(
            Dialect::Openai,
            support::openai_events(&openai)
        )),
        parsed
    );
}
