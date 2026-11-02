#![cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
mod support;
use gproxy_client_sdk::*;
use serde_json::{Value, json};

#[test]
fn vendor_options_are_typed_and_keep_native_nulls_and_extensions() {
    let options: ProviderOptions = serde_json::from_value(json!({
        "claude": {
            "max_tokens": 64,
            "thinking": {
                "type": "adaptive"
            },
            "tools": [
                {
                    "name": "native",
                    "input_schema": {
                        "type": "object"
                    }
                }
            ],
            "claude_only": true
        },
        "openai": {
            "responses": {
                "reasoning": {
                    "effort": "high"
                },
                "include": [
                    "reasoning.encrypted_content"
                ],
                "store": null,
                "tools": [
                    {
                        "type": "function",
                        "name": "native",
                        "parameters": {
                            "type": "object"
                        },
                        "strict": null
                    }
                ],
                "responses_only": 2
            },
            "chat": {
                "audio": {
                    "voice": "alloy",
                    "format": "wav",
                    "audio_extra": 7
                },
                "tools": [
                    {
                        "type": "function",
                        "function": {
                            "name": "native",
                            "parameters": {
                                "type": "object"
                            }
                        }
                    }
                ],
                "seed": 3,
                "chat_only": 4
            },
            "openai_only": 5
        },
        "gemini": {
            "generationConfig": {
                "temperature": 0.2,
                "speechConfig": {
                    "languageCode": "en-US",
                    "speech_extra": 8,
                    "voiceConfig": {
                        "voice_extra": 9,
                        "prebuiltVoiceConfig": {
                            "voiceName": "Puck",
                            "prebuilt_extra": 10
                        }
                    }
                }
            },
            "tools": [
                {
                    "googleSearch": {}
                }
            ],
            "cachedContent": "cachedContents/one",
            "gemini_only": 6
        }
    }))
    .unwrap();
    assert_eq!(options.openai.responses.store, Some(None));
    let persisted: ProviderOptions =
        serde_json::from_value(serde_json::to_value(&options).unwrap()).unwrap();
    assert_eq!(options, persisted);
    for dialect in [
        Dialect::Claude,
        Dialect::Openai,
        Dialect::OpenaiChat,
        Dialect::Gemini,
    ] {
        let mut request = GenerateRequest::new(
            Model {
                id: "model".into(),
                dialect,
            },
            vec![Message {
                role: Role::User,
                content: vec![Block::text("hello")],
            }],
        );
        request.provider_options = options.clone();
        request.tools = vec![Tool {
            name: "portable".into(),
            description: String::new(),
            input_schema: json!({"type":"object"}),
        }];
        let native = dialects::render(&request, true).unwrap();
        assert_eq!(native["tools"].as_array().unwrap().len(), 2);
        match dialect {
            Dialect::Claude => {
                assert_eq!(native["thinking"], json!({"type":"adaptive"}));
                assert_eq!(native["max_tokens"], 64);
                assert!(native.get("openai_only").is_none());
            }
            Dialect::Openai => {
                assert_eq!(native["store"], Value::Null);
                assert_eq!(native["reasoning"]["effort"], "high");
                assert_eq!(native["responses_only"], 2);
                assert_eq!(native["openai_only"], 5);
                assert!(native.get("chat_only").is_none());
            }
            Dialect::OpenaiChat => {
                assert_eq!(native["audio"]["format"], "wav");
                assert_eq!(native["seed"], 3);
                assert_eq!(native["stream_options"]["include_usage"], true);
                assert!(native.get("responses_only").is_none());
            }
            Dialect::Gemini => {
                assert_eq!(native["cachedContent"], "cachedContents/one");
                assert_eq!(native["generationConfig"]["temperature"], 0.2);
                assert!(native.get("openai_only").is_none());
            }
        }
        if matches!(dialect, Dialect::OpenaiChat | Dialect::Gemini) {
            request.settings.speech = Some(SpeechSettings {
                voice: if dialect == Dialect::Gemini {
                    "Kore"
                } else {
                    "nova"
                }
                .into(),
                format: AudioFormat::pcm(24000, 1),
            });
            let updated = dialects::render(&request, true).unwrap();
            if dialect == Dialect::OpenaiChat {
                assert_eq!(updated["audio"]["audio_extra"], 7);
                assert_eq!(updated["audio"]["voice"], "nova");
            } else {
                let speech = &updated["generationConfig"]["speechConfig"];
                assert_eq!(speech["languageCode"], "en-US");
                assert_eq!(speech["speech_extra"], 8);
                assert_eq!(speech["voiceConfig"]["voice_extra"], 9);
                assert_eq!(
                    speech["voiceConfig"]["prebuiltVoiceConfig"]["prebuilt_extra"],
                    10
                );
                assert_eq!(
                    speech["voiceConfig"]["prebuiltVoiceConfig"]["voiceName"],
                    "Kore"
                );
            }
        }
    }
}

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
                dialects::render(&request, false).unwrap()["messages"][0]["content"].clone();
            let mut expected = native["content"].clone();
            support::normalize_claude(&mut expected);
            assert_eq!(actual, expected);
            assert!(actual[0]["citations"][0].get("document_title").is_some());
        }
    }
}

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
        dialects::render(&request, false).unwrap()["input"],
        native["output"]
    );
}

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
    let mut request = support::request(
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
    futures_executor::block_on(request.resolve_assets(|reference| {
        calls += 1;
        assert_eq!(reference, "image");
        async {
            Ok(ResolvedAsset::Bytes {
                bytes: vec![1, 2, 3],
                media_type: "image/png".into(),
            })
        }
    }))
    .unwrap();
    assert_eq!(calls, 1);
    let native = dialects::render(&request, false).unwrap();
    assert_eq!(
        native["messages"][0]["content"][0]["content"][0]["source"]["content"][0]["source"]["data"],
        "AQID"
    );
    assert!(request.messages[0].content[0].provider_metadata.is_empty());
}

#[test]
fn chat_refusal_and_legacy_tool_arguments_keep_distinct_stream_blocks() {
    let message = json!({
        "role":"assistant","content":"visible","refusal":"refused",
        "function_call":{"name":"f","arguments":"{\"x\":1}","extra":true},"extra_message":7
    });
    let native = json!({
        "id":"chat_1","object":"chat.completion","created":1,"model":"m",
        "choices":[{"index":0,"message":message,"finish_reason":"function_call"}]
    });
    let parsed = dialects::chat::parse(serde_json::from_value(native).unwrap(), None).unwrap();
    let mut decoder = stream::StreamDecoder::new(Dialect::OpenaiChat);
    let first = json!({"content":"visible","refusal":"refused","function_call":{"name":"f","arguments":"{\"x\":","extra":true},"extra_message":7});
    let second = json!({"function_call":{"arguments":"1}"}});
    let mut events = Vec::new();
    for (delta, reason) in [(first, Value::Null), (second, json!("function_call"))] {
        events.extend(decoder.push_json(&json!({"id":"chat_1","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":delta,"finish_reason":reason}]}).to_string()).unwrap());
    }
    events.extend(decoder.finish().unwrap());
    assert_eq!(support::aggregate(events), parsed);
    let request = support::request(Dialect::OpenaiChat, support::persisted(parsed));
    assert_eq!(
        dialects::render(&request, false).unwrap()["messages"][0],
        message
    );
}

#[test]
fn chat_reasoning_details_keep_signatures_and_layout_without_copying_visible_text() {
    let details = json!([
        {"type":"reasoning.text","index":0,"id":"r0","format":"native","text":"portable reasoning","signature":"sig","extra":1},
        {"type":"reasoning.summary","index":1,"summary":" summary","signature":null},
        {"type":"reasoning.encrypted","index":2,"data":"cipher"}
    ]);
    let message =
        json!({"role":"assistant","content":null,"refusal":null,"reasoning_details":details});
    let native = json!({"id":"chat_1","object":"chat.completion","created":1,"model":"m","choices":[{"index":0,"message":message,"finish_reason":"stop"}]});
    let parsed = dialects::chat::parse(serde_json::from_value(native).unwrap(), None).unwrap();
    assert!(
        matches!(&parsed.content[0].content,Content::Reasoning(Reasoning {text}) if text=="portable reasoning summary")
    );
    let metadata = serde_json::to_string(&parsed.content[0].provider_metadata).unwrap();
    assert!(!metadata.contains("portable reasoning"));
    assert!(!metadata.contains(" summary"));
    assert!(metadata.contains("sig"));
    assert!(metadata.contains("cipher"));
    let mut decoder = stream::StreamDecoder::new(Dialect::OpenaiChat);
    let mut events = Vec::new();
    for (part, reason) in [
        (
            json!([{"type":"reasoning.text","index":0,"id":"r0","format":"native","text":"portable ","extra":1}]),
            Value::Null,
        ),
        (
            json!([{"type":"reasoning.text","index":0,"text":"reasoning","signature":"sig"}]),
            Value::Null,
        ),
        (
            json!([{"type":"reasoning.summary","index":1,"summary":" summary","signature":null},{"type":"reasoning.encrypted","index":2,"data":"cipher"}]),
            json!("stop"),
        ),
    ] {
        events.extend(decoder.push_json(&json!({"id":"chat_1","object":"chat.completion.chunk","created":1,"model":"m","choices":[{"index":0,"delta":{"reasoning_details":part},"finish_reason":reason}]}).to_string()).unwrap());
    }
    events.extend(decoder.finish().unwrap());
    assert!(
        events
            .iter()
            .any(|event| matches!(event,Event::ReasoningDelta {delta,..} if delta=="portable "))
    );
    assert_eq!(support::aggregate(events), parsed);
    let mut request = support::request(Dialect::OpenaiChat, support::persisted(parsed));
    assert_eq!(
        dialects::render(&request, false).unwrap()["messages"][0],
        message
    );
    if let Content::Reasoning(reasoning) = &mut request.messages[0].content[0].content {
        reasoning.text = "edited".into();
    }
    let replay = dialects::render(&request, false).unwrap();
    assert_eq!(
        replay["messages"][0]["reasoning_details"][0]["text"],
        "edited"
    );
    assert_eq!(
        replay["messages"][0]["reasoning_details"][0]["signature"],
        "sig"
    );
}
