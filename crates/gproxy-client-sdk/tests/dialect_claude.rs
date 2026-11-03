mod support;
use gproxy_client_sdk::*;
use serde_json::{Value, json};

#[cfg(feature = "claude")]
#[test]
fn claude_same_vendor_roundtrip_including_signed_and_redacted_thinking() {
    let native = support::claude();
    let parsed = dialects::claude::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    assert_eq!(parsed.usage.reasoning_tokens, Some(2));
    let request = support::request(Dialect::Claude, support::persisted(parsed));
    let rendered = dialects::render(&request, false).unwrap().request;
    assert_eq!(rendered["messages"][0]["content"], native["content"]);
}

#[cfg(feature = "claude")]
#[test]
fn claude_stream_aggregates_to_the_buffered_result_and_can_be_replayed() {
    let native = support::claude();
    let events = support::decode(Dialect::Claude, support::claude_events(&native));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ReasoningDelta { .. }))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::ToolCallArgsDelta { .. }))
    );
    let result = support::aggregate(events);
    assert_eq!(
        result,
        dialects::claude::parse(serde_json::from_value(native.clone()).unwrap()).unwrap()
    );
    let rendered = dialects::render(
        &support::request(Dialect::Claude, support::persisted(result)),
        false,
    )
    .unwrap()
    .request;
    assert_eq!(rendered["messages"][0]["content"], native["content"]);
}

#[cfg(feature = "claude")]
#[test]
fn claude_tool_result_optional_fields_are_replayed_losslessly() {
    for value in [
        json!({"type":"tool_result","tool_use_id":"call"}),
        json!({"type":"tool_result","tool_use_id":"call","content":"answer","is_error":false,"extra":true}),
        json!({"type":"tool_result","tool_use_id":"call","content":[{"type":"text","text":"answer","extra":true}],"is_error":true}),
    ] {
        let parsed =
            dialects::claude::parse_tool_result(serde_json::from_value(value.clone()).unwrap())
                .unwrap();
        let request = support::request(
            Dialect::Claude,
            vec![Message {
                role: Role::Tool,
                content: vec![parsed],
            }],
        );
        assert_eq!(
            dialects::render(&request, false).unwrap().request["messages"][0]["content"][0],
            value
        );
    }
}

#[cfg(feature = "claude")]
#[test]
fn typed_claude_mapping_preserves_nullable_history_and_citation_fields() {
    let mut native = support::claude();
    native["content"][2]["citations"] = json!([{
        "type":"char_location",
        "cited_text":"quote",
        "document_index":0,
        "document_title":null,
        "file_id":"source_file",
        "start_char_index":0,
        "end_char_index":5,
        "futureCitation":{"keep":true}
    }]);
    native["content"][3]["toolset_name"] = Value::Null;
    let parsed = dialects::claude::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    let rendered = dialects::render(
        &support::request(Dialect::Claude, support::persisted(parsed)),
        false,
    )
    .unwrap()
    .request;
    let mut expected = native["content"].clone();
    support::normalize_claude(&mut expected);
    assert_eq!(rendered["messages"][0]["content"], expected);
}
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
#[test]
fn claude_optional_nulls_are_omitted_and_required_nullable_titles_stay_present() {
    for kind in ["char_location", "page_location", "content_block_location"] {
        for title_present in [false, true] {
            let mut citation = json!({
                "type":kind,
                "cited_text":"quoted",
                "document_index":0,
                "file_id":null,
                "future":true
            });
            match kind {
                "char_location" => {
                    citation["start_char_index"] = json!(0);
                    citation["end_char_index"] = json!(6);
                }
                "page_location" => {
                    citation["start_page_number"] = json!(1);
                    citation["end_page_number"] = json!(2);
                }
                _ => {
                    citation["start_block_index"] = json!(0);
                    citation["end_block_index"] = json!(1);
                }
            }
            if title_present {
                citation["document_title"] = Value::Null;
            }
            let mut native = support::claude();
            native["content"] = json!([
                {"type":"text","text":"answer","citations":[citation]},
                {"type":"text","text":"plain","citations":null},
                {"type":"tool_use","id":"call_1","name":"f","input":{},"toolset_name":null},
                {"type":"web_search_tool_result","tool_use_id":"server_1","content":[{"type":"web_search_result","encrypted_content":"cipher","title":"Result","url":"https://example.com","page_age":null}]}
            ]);
            let parsed =
                dialects::claude::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
            let request = support::request(Dialect::Claude, support::persisted(parsed));
            let actual =
                dialects::render(&request, false).unwrap().request["messages"][0]["content"]
                    .clone();
            let mut expected = native["content"].clone();
            support::normalize_claude(&mut expected);
            assert_eq!(actual, expected);
            assert!(actual[0]["citations"][0].get("document_title").is_some());
        }
    }
}
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
#[test]
fn claude_document_image_cache_and_tool_media_are_typed_and_lossless() {
    let native = json!({
        "type": "tool_result",
        "tool_use_id": "call_1",
        "is_error": false,
        "cache_control": {
            "type": "ephemeral",
            "ttl": "1h",
            "future": 1
        },
        "extra": true,
        "content": [
            {
                "type": "text",
                "text": "see report",
                "cache_control": {
                    "type": "ephemeral"
                },
                "future": 2
            },
            {
                "type": "image",
                "source": {
                    "type": "base64",
                    "media_type": "image/png",
                    "data": "AQID",
                    "source_extra": 3
                },
                "image_extra": 4
            },
            {
                "type": "document",
                "source": {
                    "type": "content",
                    "content": [
                        {
                            "type": "text",
                            "text": "part",
                            "extra": 5
                        },
                        {
                            "type": "image",
                            "source": {
                                "type": "url",
                                "url": "https://example.com/a.png",
                                "source_extra": 6
                            }
                        }
                    ],
                    "source_extra": 7
                },
                "title": "Report",
                "context": "Notes",
                "citations": {
                    "enabled": true,
                    "extra": 8
                },
                "cache_control": {
                    "type": "ephemeral",
                    "ttl": "5m"
                },
                "extra": 9
            }
        ]
    });
    let block =
        dialects::claude::parse_tool_result(serde_json::from_value(native.clone()).unwrap())
            .unwrap();
    assert_eq!(block.cache.as_ref().unwrap().ttl, Some(CacheTtl::OneHour));
    let Content::ToolResult(result) = &block.content else {
        panic!()
    };
    let ToolOutput::Blocks(blocks) = &result.output else {
        panic!()
    };
    assert!(matches!(blocks[1].content, ToolResultContent::Image(_)));
    assert!(matches!(blocks[2].content, ToolResultContent::Document(_)));
    let req = support::request(
        Dialect::Claude,
        vec![Message {
            role: Role::Tool,
            content: vec![block],
        }],
    );
    assert_eq!(
        dialects::render(&req, false).unwrap().request["messages"][0]["content"][0],
        native
    );
}
