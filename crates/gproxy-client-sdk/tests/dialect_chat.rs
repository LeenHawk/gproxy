mod support;
use gproxy_client_sdk::*;
use serde_json::{Value, json};
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
use support::content::chat_response;
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
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
        dialects::render(&request, false).unwrap().request["messages"][0],
        message
    );
}
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
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
        dialects::render(&request, false).unwrap().request["messages"][0],
        message
    );
    if let Content::Reasoning(reasoning) = &mut request.messages[0].content[0].content {
        reasoning.text = "edited".into();
    }
    let replay = dialects::render(&request, false).unwrap().request;
    assert_eq!(
        replay["messages"][0]["reasoning_details"][0]["text"],
        "edited"
    );
    assert_eq!(
        replay["messages"][0]["reasoning_details"][0]["signature"],
        "sig"
    );
}
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
#[test]
fn chat_audio_id_continuation_and_expiration_use_transcript() {
    let result = dialects::chat::parse(
        serde_json::from_value(chat_response()).unwrap(),
        Some(AudioFormat::pcm(24000, 1)),
    )
    .unwrap();
    let mut request = support::request(Dialect::OpenaiChat, support::persisted(result));
    let native = serde_json::to_value(
        dialects::chat::render_at(&request, false, 1900000000)
            .unwrap()
            .request,
    )
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
    let native = serde_json::to_value(
        dialects::chat::render_at(&request, false, 2100000000)
            .unwrap()
            .request,
    )
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
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
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
    let native = dialects::render(&request, true).unwrap().request;
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
#[cfg(all(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
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
            json!({
                "audio": {
                    "data": "Aw==",
                    "transcript": "hello"
                },
                "tool_calls": [
                    {
                        "index": 0,
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "weather",
                            "arguments": "{ \"city\": \"Shanghai\" }",
                            "extra": 2
                        },
                        "extra": 3
                    }
                ]
            }),
            json!("tool_calls"),
        ),
    ] {
        events.extend(decoder.push_json(&json!({"id":"chat_1","object":"chat.completion.chunk","created":1,"model":"audio-model","choices":[{"index":0,"delta":delta,"finish_reason":finish}]}).to_string()).unwrap());
    }
    events.extend(
        decoder
            .push_json(
                &json!({
                    "id": "chat_1",
                    "object": "chat.completion.chunk",
                    "created": 1,
                    "model": "audio-model",
                    "choices": [],
                    "usage": {
                        "prompt_tokens": 8,
                        "completion_tokens": 9,
                        "total_tokens": 17
                    }
                })
                .to_string(),
            )
            .unwrap(),
    );
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
