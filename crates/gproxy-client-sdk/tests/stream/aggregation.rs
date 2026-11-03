use super::*;
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
#[test]
fn server_outputs_are_ir_tool_results_and_roundtrip_including_streams() {
    let mut native = support::openai();
    native["output"] = json!([
        {
            "type":"file_search_call","id":"fs_1","queries":["query"],"status":"completed","agent":null,
            "results":[{"file_id":"file_1","text":"result payload","extra":1}]
        },
        {
            "type":"code_interpreter_call","id":"ci_1","code":"print(1)","container_id":"container_1","status":"completed",
            "outputs":[{"type":"logs","logs":"log payload"},{"type":"image","url":"data:image/png;base64,AQIDBAU="}]
        },
        {
            "type":"computer_call","id":"computer_1","call_id":"call_1","action":{"type":"screenshot"},
            "actions":[{"type":"screenshot"}],"pending_safety_checks":[],"status":"completed"
        }
    ]);
    let parsed = dialects::openai::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    assert_eq!(parsed.content.len(), 5);
    assert!(matches!(
        &parsed.content[1].content,
        Content::ToolResult(ToolResult {
            provider_executed: true,
            output: ToolOutput::Json(_),
            ..
        })
    ));
    assert!(matches!(
        &parsed.content[3].content,
        Content::ToolResult(ToolResult {
            provider_executed: true,
            output: ToolOutput::Json(_),
            ..
        })
    ));
    for block in &parsed.content {
        let metadata = serde_json::to_string(&block.provider_metadata).unwrap();
        for payload in ["result payload", "log payload", "AQIDBAU=", "print(1)"] {
            assert!(
                !metadata.contains(payload),
                "duplicated payload in {metadata}"
            );
        }
    }
    let events = support::decode(Dialect::Openai, support::openai_events(&native));
    assert!(events.iter().any(|e| matches!(
        e,
        Event::ToolCallStart {
            provider_executed: true,
            ..
        }
    )));
    assert_eq!(support::aggregate(events), parsed);
    let request = support::request(Dialect::Openai, support::persisted(parsed));
    assert_eq!(
        dialects::render(&request, false).unwrap().request["input"],
        native["output"]
    );
}

#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
#[test]
fn citations_are_portable_and_stream_aggregation_keeps_late_grounding() {
    let mut native = support::claude();
    native["content"] = json!([
        {
            "type": "text",
            "text": "evidence",
            "citations": [
                {
                    "type": "page_location",
                    "cited_text": "quote",
                    "document_index": 0,
                    "document_title": "Report",
                    "start_page_number": 1,
                    "end_page_number": 2,
                    "file_id": null,
                    "extra": 3
                }
            ]
        }
    ]);
    let parsed = dialects::claude::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    let Content::Text(text) = &parsed.content[0].content else {
        panic!()
    };
    assert_eq!(text.citations[0].cited_text.as_deref(), Some("quote"));
    assert!(matches!(
        text.citations[0].source,
        CitationSource::Document {
            range: Some(DocumentRange::Pages { start: 1, end: 2 }),
            ..
        }
    ));
    let request = support::request(Dialect::Claude, support::persisted(parsed));
    assert_eq!(
        dialects::render(&request, false).unwrap().request["messages"][0]["content"],
        native["content"]
    );
    let mut native = support::openai();
    native["output"] = json!([
        {
            "type": "message",
            "id": "msg_1",
            "role": "assistant",
            "status": "completed",
            "content": [
                {
                    "type": "output_text",
                    "text": "evidence",
                    "logprobs": [],
                    "annotations": [
                        {
                            "type": "url_citation",
                            "start_index": 0,
                            "end_index": 8,
                            "url": "https://example.com/source",
                            "title": "Source",
                            "extra": 4
                        }
                    ]
                }
            ]
        }
    ]);
    let parsed = dialects::openai::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    let request = support::request(Dialect::Openai, support::persisted(parsed));
    assert_eq!(
        dialects::render(&request, false).unwrap().request["input"],
        native["output"]
    );
    let first = json!({"candidates":[{"content":{"parts":[{"text":"grounded"}]}}]});
    let last = json!({
        "candidates": [
            {
                "finishReason": "STOP",
                "groundingMetadata": {
                    "groundingChunks": [
                        {
                            "web": {
                                "uri": "https://example.com/source",
                                "title": "Source"
                            }
                        }
                    ],
                    "groundingSupports": [
                        {
                            "segment": {
                                "partIndex": 0,
                                "startIndex": 0,
                                "endIndex": 8,
                                "text": "grounded"
                            },
                            "groundingChunkIndices": [
                                0
                            ]
                        }
                    ]
                }
            }
        ]
    });
    let mut combined = first.clone();
    combined["candidates"][0]["groundingMetadata"] =
        last["candidates"][0]["groundingMetadata"].clone();
    combined["candidates"][0]["finishReason"] = json!("STOP");
    let parsed = dialects::gemini::parse(serde_json::from_value(combined).unwrap()).unwrap();
    let mut decoder = stream::StreamDecoder::new(Dialect::Gemini);
    let mut events = decoder.push_json(&first.to_string()).unwrap();
    events.extend(decoder.push_json(&last.to_string()).unwrap());
    events.extend(decoder.finish().unwrap());
    assert!(events.iter().any(|e| matches!(e, Event::Citation { .. })));
    assert_eq!(
        futures_executor::block_on(aggregate(futures_util::stream::iter(events))).unwrap(),
        parsed
    );
}

#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini",
    not(target_arch = "wasm32")
))]
#[test]
fn refusal_is_portable_and_claude_stream_block_end_is_authoritative() {
    let mut native = answer();
    native["output"][0]["content"] = json!([{"type":"refusal","refusal":"Cannot help"}]);
    let parsed = dialects::openai::parse(serde_json::from_value(native).unwrap()).unwrap();
    assert!(matches!(
        &parsed.content[0].content,
        Content::Text(Text { refusal: true, .. })
    ));
    let request = support::request(Dialect::OpenaiChat, support::persisted(parsed));
    let compiled = dialects::render(&request, false).unwrap();
    assert_eq!(compiled.request["messages"][0]["refusal"], "Cannot help");
    assert!(
        compiled
            .warnings
            .iter()
            .any(|w| w.field.ends_with(".provider_metadata"))
    );
    let mut claude = support::claude();
    claude["content"] = json!([{"type":"text","text":"Cannot help"}]);
    claude["stop_reason"] = json!("refusal");
    let buffered =
        dialects::claude::parse(serde_json::from_value(claude.clone()).unwrap()).unwrap();
    let events = support::decode(Dialect::Claude, support::claude_events(&claude));
    assert!(events.iter().any(|event| matches!(
        event,
        Event::BlockEnd {
            block: Block {
                content: Content::Text(Text { refusal: true, .. }),
                ..
            },
            ..
        }
    )));
    assert_eq!(support::aggregate(events), buffered);
    let switched = support::request(Dialect::Gemini, support::persisted(buffered));
    let compiled = dialects::render(&switched, false).unwrap();
    assert!(
        compiled
            .warnings
            .iter()
            .any(|w| w.field.ends_with(".refusal"))
    );
    assert!(matches!(
        switched.messages[0].content[0].content,
        Content::Text(Text { refusal: true, .. })
    ));
}

#[cfg(feature = "gemini")]
#[test]
fn gemini_text_split_across_chunks_aggregates_to_the_non_streaming_shape() {
    // Live: "1 2 3 4 5" streamed as three text parts became three blocks.
    let chunks = [
        json!({"candidates":[{"content":{"role":"model","parts":[{"text":"1 2 "}]}}]}),
        json!({"candidates":[{"content":{"role":"model","parts":[{"text":"3 4 "}]}}]}),
        json!({"candidates":[{"content":{"role":"model","parts":[{"text":"5","thoughtSignature":"c2ln"}]},"finishReason":"STOP"}]}),
    ];
    let single = json!({"candidates":[{"content":{"role":"model","parts":[{"text":"1 2 3 4 5","thoughtSignature":"c2ln"}]},"finishReason":"STOP"}]});
    let parsed = dialects::gemini::parse(serde_json::from_value(single).unwrap()).unwrap();

    let mut decoder = stream::StreamDecoder::new(Dialect::Gemini);
    let mut events = Vec::new();
    for chunk in &chunks {
        events.extend(decoder.push_json(&chunk.to_string()).unwrap());
    }
    events.extend(decoder.finish().unwrap());
    let deltas: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::TextDelta { id, delta } => Some((*id, delta.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(deltas.len(), 3);
    assert!(deltas.iter().all(|(id, _)| *id == deltas[0].0));

    let aggregated =
        futures_executor::block_on(aggregate(futures_util::stream::iter(events))).unwrap();
    assert_eq!(aggregated.content, parsed.content);
}
