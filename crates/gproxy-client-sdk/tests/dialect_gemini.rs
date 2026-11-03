mod support;
use gproxy_client_sdk::*;
use serde_json::json;
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
use support::content::{gemini, replay_gemini};

#[cfg(feature = "gemini")]
#[test]
fn gemini_in_band_errors_keep_the_native_status_without_inventing_http_status() {
    let mut decoder = stream::StreamDecoder::new(Dialect::Gemini);
    let events = decoder
        .push_json(
            &json!({"error":{"code":429,"status":"RESOURCE_EXHAUSTED","message":"quota"}})
                .to_string(),
        )
        .unwrap();
    let error = events
        .iter()
        .find_map(|event| match event {
            Event::Error { error } => Some(error),
            _ => None,
        })
        .unwrap();
    assert_eq!(error.kind, ErrorKind::RateLimit);
    assert_eq!(error.status, None);
    assert!(error.retryable);
    assert!(events.iter().any(|event| matches!(
        event,
        Event::Finish {
            reason: FinishReason {
                unified: FinishReasonKind::Error,
                ..
            },
            ..
        }
    )));
}

#[cfg(feature = "gemini")]
#[test]
fn gemini_same_vendor_preserves_signatures_and_unknown_nested_media_fields() {
    let native = support::gemini();
    let result = dialects::gemini::parse(serde_json::from_value(native.clone()).unwrap()).unwrap();
    let request = support::request(Dialect::Gemini, support::persisted(result));
    let rendered = dialects::render(&request, false).unwrap().request;
    assert_eq!(
        rendered["contents"][0]["parts"],
        native["candidates"][0]["content"]["parts"]
    );
}

#[cfg(feature = "gemini")]
#[test]
fn gemini_stream_keeps_native_part_boundaries_and_late_usage() {
    let native = support::gemini();
    let mut chunks = native["candidates"][0]["content"]["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|part| json!({"candidates":[{"content":{"role":"model","parts":[part]}}]}))
        .collect::<Vec<_>>();
    chunks.push(json!({"candidates":[{"finishReason":"STOP"}]}));
    chunks.push(json!({"usageMetadata":native["usageMetadata"]}));
    let result = support::aggregate(support::decode(Dialect::Gemini, chunks));
    assert_eq!(
        result,
        dialects::gemini::parse(serde_json::from_value(native.clone()).unwrap()).unwrap()
    );
    assert_eq!(
        dialects::render(
            &support::request(Dialect::Gemini, support::persisted(result)),
            false
        )
        .unwrap()
        .request["contents"][0]["parts"],
        native["candidates"][0]["content"]["parts"]
    );
}

#[cfg(all(feature = "claude", feature = "gemini"))]
#[test]
fn claude_tool_result_name_is_resolved_from_an_earlier_call_when_continuing_on_gemini() {
    let reply =
        dialects::claude::parse(serde_json::from_value(support::claude()).unwrap()).unwrap();
    let native: protocol::claude::content::ToolResultBlock = serde_json::from_value(json!({
        "type":"tool_result",
        "tool_use_id":"tool_1",
        "content":"sunny"
    }))
    .unwrap();
    let result = dialects::claude::parse_tool_result(native).unwrap();
    assert!(matches!(&result.content,Content::ToolResult(ToolResult {name,..}) if name.is_empty()));
    let request = support::request(
        Dialect::Gemini,
        vec![
            reply.into_message(),
            Message {
                role: Role::Tool,
                content: vec![result],
            },
        ],
    );
    let native = dialects::gemini::render(&request).unwrap().request;
    let response = native.contents[1].parts.as_ref().unwrap()[0]
        .function_response
        .as_ref()
        .unwrap();
    assert_eq!(response.name, "weather");
    assert_eq!(response.id.as_deref(), Some("tool_1"));
}

#[cfg(all(feature = "openai", feature = "gemini"))]
#[test]
fn responses_tool_result_name_is_resolved_from_earlier_call_id() {
    let mut native = support::openai();
    native["output"].as_array_mut().unwrap().push(json!({
        "type":"function_call_output",
        "id":"result_1",
        "call_id":"call_1",
        "output":"sunny",
        "status":"completed"
    }));
    let result = dialects::openai::parse(serde_json::from_value(native).unwrap()).unwrap();
    assert!(
        matches!(&result.content.last().unwrap().content,Content::ToolResult(ToolResult {name,..}) if name.is_empty())
    );
    let rendered = dialects::gemini::render(&support::request(
        Dialect::Gemini,
        vec![result.into_message()],
    ))
    .unwrap()
    .request;
    let parts = rendered.contents[0].parts.as_ref().unwrap();
    let response = parts
        .iter()
        .find_map(|part| part.function_response.as_ref())
        .unwrap();
    assert_eq!(response.name, "weather");
}
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
#[test]
fn gemini_images_pcm_video_and_server_tools_roundtrip() {
    replay_gemini(json!([
        {
            "inlineData": {
                "mimeType": "image/png",
                "data": "AQID",
                "displayName": "generated",
                "extra": 1
            },
            "mediaResolution": {
                "level": "MEDIA_RESOLUTION_HIGH",
                "extra": 2
            },
            "thoughtSignature": "signed"
        },
        {
            "inlineData": {
                "mimeType": "audio/pcm;rate=24000",
                "data": "BAU=",
                "extra": 3
            },
            "audioTranscription": {
                "text": "hello",
                "extra": 4
            }
        },
        {
            "fileData": {
                "fileUri": "https://example.com/movie.mp4",
                "mimeType": "video/mp4",
                "extra": 5
            },
            "videoMetadata": {
                "startOffset": "1.500s",
                "endOffset": "3s",
                "fps": 2.0,
                "extra": 6
            }
        },
        {
            "toolCall": {
                "toolType": "GOOGLE_SEARCH_WEB",
                "id": "s1",
                "toolName": "search",
                "args": {
                    "q": "weather"
                },
                "extra": 7
            }
        },
        {
            "toolResponse": {
                "toolType": "GOOGLE_SEARCH_WEB",
                "id": "s1",
                "response": {
                    "answer": "sunny"
                },
                "extra": 8
            }
        }
    ]));
}
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
#[test]
fn gemini_tool_result_media_preserves_json_and_nested_extensions() {
    replay_gemini(json!([
        {
            "functionResponse": {
                "name": "inspect",
                "id": "call_1",
                "response": {
                    "answer": 7
                },
                "parts": [
                    {
                        "inlineData": {
                            "mimeType": "image/png",
                            "data": "AQID",
                            "extra": 1
                        },
                        "extra": 2
                    },
                    {
                        "inlineData": {
                            "mimeType": "application/pdf",
                            "data": "BAU=",
                            "displayName": "Doc",
                            "extra": 3
                        }
                    }
                ],
                "extra": 4
            },
            "thoughtSignature": "signature"
        }
    ]));
}
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
#[test]
fn gemini_modalities_voice_and_pcm_constraints() {
    let mut request = support::request(Dialect::Gemini, vec![]);
    request.settings.output_modalities = vec![Modality::Audio];
    request.settings.speech = Some(SpeechSettings {
        voice: "Kore".into(),
        format: AudioFormat::pcm(24000, 1),
    });
    let native = dialects::render(&request, false).unwrap().request;
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
        Err(Error {
            kind: ErrorKind::InvalidData,
            ..
        })
    ));
}
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
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
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
#[test]
fn gemini_file_audio_transcript_and_video_without_mime_keep_their_media_kinds() {
    let parts = json!([
        {
            "fileData": {
                "fileUri": "https://example.com/audio",
                "mimeType": "audio/pcm;rate=16000",
                "displayName": "clip"
            },
            "audioTranscription": {
                "text": "transcript",
                "speakerLabel": "one"
            }
        },
        {
            "fileData": {
                "fileUri": "https://example.com/video",
                "displayName": "video"
            },
            "videoMetadata": {
                "startOffset": "1s",
                "endOffset": "2s",
                "fps": 1.0
            }
        }
    ]);
    let parsed = gemini(parts.clone());
    assert!(
        matches!(&parsed.content[0].content,Content::Audio(Audio {transcript:Some(text),..}) if text=="transcript")
    );
    assert!(matches!(
        &parsed.content[1].content,
        Content::Video(Video { clip: Some(_), .. })
    ));
    replay_gemini(parts);
}
