#![cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
mod support;
use gproxy_client_sdk::*;
use serde_json::{Value, json};
fn document(source: DocumentSource) -> Document {
    Document {
        source,
        media_type: Some("application/pdf".into()),
        title: Some("Report".into()),
        context: Some("Notes".into()),
        citations: Some(true),
        detail: Detail::High,
    }
}
fn gemini(parts: Value) -> GenerateResult {
    dialects::gemini::parse(serde_json::from_value(json!({"candidates":[{"content":{"role":"model","parts":parts},"finishReason":"STOP"}]})).unwrap()).unwrap()
}
fn replay_gemini(parts: Value) {
    let result = gemini(parts.clone());
    let request = support::request(Dialect::Gemini, support::persisted(result));
    assert_eq!(
        dialects::render(&request, false).unwrap()["contents"][0]["parts"],
        parts
    );
}
#[test]
fn claude_document_image_cache_and_tool_media_are_typed_and_lossless() {
    let native = json!({"type":"tool_result","tool_use_id":"call_1","is_error":false,"cache_control":{"type":"ephemeral","ttl":"1h","future":1},"extra":true,"content":[
        {"type":"text","text":"see report","cache_control":{"type":"ephemeral"},"future":2},
        {"type":"image","source":{"type":"base64","media_type":"image/png","data":"AQID","source_extra":3},"image_extra":4},
        {"type":"document","source":{"type":"content","content":[{"type":"text","text":"part","extra":5},{"type":"image","source":{"type":"url","url":"https://example.com/a.png","source_extra":6}}],"source_extra":7},"title":"Report","context":"Notes","citations":{"enabled":true,"extra":8},"cache_control":{"type":"ephemeral","ttl":"5m"},"extra":9}
    ]});
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
        dialects::render(&req, false).unwrap()["messages"][0]["content"][0],
        native
    );
}
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
    let native = dialects::render(&request, false).unwrap();
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
    let native = dialects::render(&request, false).unwrap();
    assert_eq!(
        native["input"][0]["content"][0]["prompt_cache_breakpoint"]["ttl"],
        "30m"
    );
    assert_eq!(native["input"][0]["content"][1]["detail"], "low");
    assert_eq!(native["input"][0]["content"][2]["type"], "input_file");
    assert_eq!(native["input"][0]["content"][2]["detail"], "high");
}
#[test]
fn gemini_images_pcm_video_and_server_tools_roundtrip() {
    replay_gemini(json!([
        {"inlineData":{"mimeType":"image/png","data":"AQID","displayName":"generated","extra":1},"mediaResolution":{"level":"MEDIA_RESOLUTION_HIGH","extra":2},"thoughtSignature":"signed"},
        {"inlineData":{"mimeType":"audio/pcm;rate=24000","data":"BAU=","extra":3},"audioTranscription":{"text":"hello","extra":4}},
        {"fileData":{"fileUri":"https://example.com/movie.mp4","mimeType":"video/mp4","extra":5},"videoMetadata":{"startOffset":"1.500s","endOffset":"3s","fps":2.0,"extra":6}},
        {"toolCall":{"toolType":"GOOGLE_SEARCH_WEB","id":"s1","toolName":"search","args":{"q":"weather"},"extra":7}},
        {"toolResponse":{"toolType":"GOOGLE_SEARCH_WEB","id":"s1","response":{"answer":"sunny"},"extra":8}}
    ]));
}
#[test]
fn gemini_tool_result_media_preserves_json_and_nested_extensions() {
    replay_gemini(
        json!([{"functionResponse":{"name":"inspect","id":"call_1","response":{"answer":7},"parts":[{"inlineData":{"mimeType":"image/png","data":"AQID","extra":1},"extra":2},{"inlineData":{"mimeType":"application/pdf","data":"BAU=","displayName":"Doc","extra":3}}],"extra":4},"thoughtSignature":"signature"}]),
    );
}
#[test]
fn gemini_modalities_voice_and_pcm_constraints() {
    let mut request = support::request(Dialect::Gemini, vec![]);
    request.settings.output_modalities = vec![Modality::Audio];
    request.settings.speech = Some(SpeechSettings {
        voice: "Kore".into(),
        format: AudioFormat::pcm(24000, 1),
    });
    let native = dialects::render(&request, false).unwrap();
    assert_eq!(
        native["generationConfig"]["responseModalities"],
        json!(["AUDIO"])
    );
    assert_eq!(
        native["generationConfig"]["speechConfig"]["voiceConfig"]["prebuiltVoiceConfig"]["voiceName"],
        "Kore"
    );
    request.settings.speech.as_mut().unwrap().format = AudioFormat::Encoded {
        media_type: "audio/mp3".into(),
    };
    assert!(matches!(
        dialects::render(&request, false),
        Err(Error::InvalidData { .. })
    ));
}
fn chat_response() -> Value {
    json!({"id":"chat_1","object":"chat.completion","created":1,"model":"audio-model","choices":[{"index":0,"finish_reason":"tool_calls","message":{"role":"assistant","content":"hello","refusal":null,"audio":{"id":"audio_1","expires_at":2000000000,"data":"AQID","transcript":"spoken hello","extra":1},"tool_calls":[{"id":"call_1","type":"function","function":{"name":"weather","arguments":"{ \"city\": \"Shanghai\" }","extra":2},"extra":3}],"extra":4}}],"usage":{"prompt_tokens":8,"completion_tokens":9,"total_tokens":17}})
}
#[test]
fn chat_audio_id_continuation_and_expiration_use_transcript() {
    let result = dialects::chat::parse(
        serde_json::from_value(chat_response()).unwrap(),
        Some(AudioFormat::pcm(24000, 1)),
    )
    .unwrap();
    let mut request = support::request(Dialect::OpenaiChat, support::persisted(result));
    let native =
        serde_json::to_value(dialects::chat::render_at(&request, false, 1900000000).unwrap())
            .unwrap();
    assert_eq!(
        native["messages"][0]["audio"],
        json!({"id":"audio_1","extra":1})
    );
    assert_eq!(native["messages"][0]["content"], "hello");
    assert_eq!(
        native["messages"][0]["tool_calls"],
        chat_response()["choices"][0]["message"]["tool_calls"]
    );
    request.messages[0].content.reverse();
    let native =
        serde_json::to_value(dialects::chat::render_at(&request, false, 2100000000).unwrap())
            .unwrap();
    assert!(native["messages"][0].get("audio").is_none());
    assert!(
        native["messages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("spoken hello")
    );
    assert_eq!(
        native["messages"][0]["tool_calls"][0]["function"]["name"],
        "weather"
    );
}
#[test]
fn chat_input_audio_and_output_settings_are_typed() {
    let audio = Audio {
        source: Asset::Inline {
            bytes: vec![1, 2, 3],
        },
        format: AudioFormat::Encoded {
            media_type: "audio/wav".into(),
        },
        transcript: None,
    };
    let mut request = support::request(
        Dialect::OpenaiChat,
        vec![Message {
            role: Role::User,
            content: vec![Block::new(Content::Audio(audio))],
        }],
    );
    request.settings.output_modalities = vec![Modality::Text, Modality::Audio];
    request.settings.speech = Some(SpeechSettings {
        voice: "alloy".into(),
        format: AudioFormat::pcm(24000, 1),
    });
    let native = dialects::render(&request, true).unwrap();
    assert_eq!(
        native["messages"][0]["content"][0]["input_audio"],
        json!({"data":"AQID","format":"wav"})
    );
    assert_eq!(native["audio"], json!({"voice":"alloy","format":"pcm16"}));
    assert_eq!(native["modalities"], json!(["text", "audio"]));
    if let Content::Audio(audio) = &mut request.messages[0].content[0].content {
        audio.format = AudioFormat::pcm(16000, 1);
    }
    assert!(dialects::render(&request, false).is_err());
}
#[test]
fn chat_stream_audio_transcript_tool_calls_and_citations_match_buffered_result() {
    let mut request = support::request(Dialect::OpenaiChat, vec![]);
    request.settings.speech = Some(SpeechSettings {
        voice: "alloy".into(),
        format: AudioFormat::pcm(24000, 1),
    });
    let mut decoder = stream::StreamDecoder::with_audio_format(
        Dialect::OpenaiChat,
        Some(AudioFormat::pcm(24000, 1)),
    );
    let mut events = Vec::new();
    for (delta, finish) in [
        (
            json!({"role":"assistant","content":"hello","audio":{"id":"audio_1","data":"AQI=","transcript":"spoken ","expires_at":2000000000,"extra":1},"extra":4}),
            Value::Null,
        ),
        (
            json!({"audio":{"data":"Aw==","transcript":"hello"},"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"weather","arguments":"{ \"city\": \"Shanghai\" }","extra":2},"extra":3}]}),
            json!("tool_calls"),
        ),
    ] {
        events.extend(decoder.push_json(&json!({"id":"chat_1","object":"chat.completion.chunk","created":1,"model":"audio-model","choices":[{"index":0,"delta":delta,"finish_reason":finish}]}).to_string()).unwrap());
    }
    events.extend(decoder.push_json(&json!({"id":"chat_1","object":"chat.completion.chunk","created":1,"model":"audio-model","choices":[],"usage":{"prompt_tokens":8,"completion_tokens":9,"total_tokens":17}}).to_string()).unwrap());
    events.extend(decoder.finish().unwrap());
    assert!(events.iter().any(|e| matches!(e, Event::AudioDelta { .. })));
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::TranscriptDelta { .. }))
    );
    let result = futures_executor::block_on(aggregate(futures_util::stream::iter(events))).unwrap();
    let buffered = dialects::chat::parse(
        serde_json::from_value(chat_response()).unwrap(),
        Some(AudioFormat::pcm(24000, 1)),
    )
    .unwrap();
    assert_eq!(result, buffered);
}
#[test]
fn version_one_persists_the_rich_content_graph() {
    let doc = ContentBlock::new(document(DocumentSource::Text {
        text: "evidence".into(),
    }));
    let mut text = Block::text("claim");
    if let Content::Text(value) = &mut text.content {
        value.citations.push(Citation {
            source: CitationSource::Document {
                document: DocumentReference::Block {
                    document: Box::new(doc),
                },
                range: Some(DocumentRange::Chars { start: 0, end: 8 }),
            },
            title: Some("source".into()),
            cited_text: Some("evidence".into()),
            text_range: Some(TextRange { start: 0, end: 5 }),
            provider_metadata: ProviderMetadata::new(),
        });
    }
    text.cache = Some(CacheBreakpoint { ttl: None });
    let conversation = Conversation::new(vec![Message {
        role: Role::Assistant,
        content: vec![text],
    }]);
    let value = serde_json::to_value(&conversation).unwrap();
    assert_eq!(value["schema_version"], "1");
    assert_eq!(
        serde_json::from_value::<Conversation>(value).unwrap(),
        conversation
    );
}

#[test]
fn gemini_separate_transcript_parts_merge_into_audio_and_replay_the_native_layout() {
    let parts = json!([
        {"inlineData":{"mimeType":"audio/pcm;rate=24000","data":"AQID"},"thoughtSignature":"audio-sig"},
        {"audioTranscription":{"text":"你好 ","extra":1},"partMetadata":{"x":2}},
        {"audioTranscription":{"text":"world","extra":3}},
        {"text":"next"}
    ]);
    let result = gemini(parts.clone());
    assert_eq!(result.content.len(), 2);
    let Content::Audio(audio) = &result.content[0].content else {
        panic!()
    };
    assert_eq!(audio.transcript.as_deref(), Some("你好 world"));
    replay_gemini(parts.clone());
    let mut decoder = stream::StreamDecoder::new(Dialect::Gemini);
    let mut events = Vec::new();
    for part in parts.as_array().unwrap() {
        events.extend(
            decoder
                .push_json(&json!({"candidates":[{"content":{"parts":[part]}}]}).to_string())
                .unwrap(),
        );
    }
    events.extend(
        decoder
            .push_json(&json!({"candidates":[{"finishReason":"STOP"}]}).to_string())
            .unwrap(),
    );
    events.extend(decoder.finish().unwrap());
    assert!(
        events
            .iter()
            .any(|e| matches!(e,Event::TranscriptDelta {delta,..} if delta=="你好 "))
    );
    assert_eq!(
        futures_executor::block_on(aggregate(futures_util::stream::iter(events))).unwrap(),
        result
    );
}
#[test]
fn citations_are_portable_and_stream_aggregation_keeps_late_grounding() {
    let mut native = support::claude();
    native["content"] = json!([{"type":"text","text":"evidence","citations":[{"type":"page_location","cited_text":"quote","document_index":0,"document_title":"Report","start_page_number":1,"end_page_number":2,"file_id":null,"extra":3}]}]);
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
        dialects::render(&request, false).unwrap()["messages"][0]["content"],
        native["content"]
    );
    let mut native = support::openai();
    native["output"] = json!([{"type":"message","id":"msg_1","role":"assistant","status":"completed","content":[{"type":"output_text","text":"evidence","logprobs":[],"annotations":[{"type":"url_citation","start_index":0,"end_index":8,"url":"https://example.com/source","title":"Source","extra":4}]}]}]);
    let parsed = dialects::openai::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    let request = support::request(Dialect::Openai, support::persisted(parsed));
    assert_eq!(
        dialects::render(&request, false).unwrap()["input"],
        native["output"]
    );
    let first = json!({"candidates":[{"content":{"parts":[{"text":"grounded"}]}}]});
    let last = json!({"candidates":[{"finishReason":"STOP","groundingMetadata":{"groundingChunks":[{"web":{"uri":"https://example.com/source","title":"Source"}}],"groundingSupports":[{"segment":{"partIndex":0,"startIndex":0,"endIndex":8,"text":"grounded"},"groundingChunkIndices":[0]}]}}]});
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
#[test]
fn responses_generated_images_and_builtin_tools_roundtrip_as_portable_blocks() {
    let mut native = support::openai();
    native["output"] = json!([
        {"type":"image_generation_call","id":"ig_1","status":"completed","result":"AQID","output_format":"png","revised_prompt":"a landscape","extra":1},
        {"type":"web_search_call","id":"web_1","status":"completed","action":{"type":"search","query":"weather","extra":2},"extra":3}
    ]);
    let parsed = dialects::openai::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    assert!(matches!(parsed.content[0].content, Content::Image(_)));
    assert!(matches!(
        parsed.content[1].content,
        Content::ToolCall(ToolCall {
            provider_executed: true,
            ..
        })
    ));
    let request = support::request(Dialect::Openai, support::persisted(parsed));
    assert_eq!(
        dialects::render(&request, false).unwrap()["input"],
        native["output"]
    );
}
