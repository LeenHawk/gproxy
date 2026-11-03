#![cfg(all(feature = "speech", not(target_arch = "wasm32")))]
#[path = "support/media.rs"]
mod support;
use futures_util::StreamExt;
use gproxy_client_sdk::{speech::*, *};
use serde_json::json;
use support::*;
fn request(dialect: SpeechDialect) -> SpeechRequest {
    let mut r = SpeechRequest::new(
        SpeechModel {
            id: "provider/voice".into(),
            dialect,
        },
        "Hello!",
    );
    r.voice = Some("voice-name".into());
    r
}
#[test]
fn openai_binary_synthesis_maps_controls_without_guessing_usage() {
    futures_executor::block_on(async {
        let client = Client::new(Mock::new(vec![Reply::binary("audio/wav", vec![1, 2, 3])]));
        let mut req = request(SpeechDialect::Openai);
        req.format = AudioFormat::Encoded {
            media_type: "audio/wav".into(),
        };
        req.instructions = Some("warm".into());
        req.speed = Some(1.1);
        req.language = Some("en".into());
        let result = client.synthesize(&req).await.unwrap();
        assert_eq!(result.usage.characters, None);
        assert_eq!(result.usage.tokens.input_tokens, None);
        assert_eq!(result.response.id.as_deref(), Some("request-1"));
        assert_eq!(result.response.model, None);
        assert!(result.warnings.iter().any(|w| w.field == "language"));
        assert!(
            matches!(result.audio.unwrap().content.source,Asset::Inline{bytes}if bytes==[1,2,3])
        );
        let sent = client.backend.sent.lock().unwrap();
        assert_eq!(sent[0].key.operation, protocol::Operation::CreateSpeech);
        assert_eq!(sent[0].json()["response_format"], "wav");
        assert_eq!(sent[0].json()["instructions"], "warm");
        assert_eq!(sent[0].json()["speed"], 1.1);
    });
}
#[test]
fn gemini_uses_generate_content_style_format_and_multispeaker_options() {
    let mut req = request(SpeechDialect::Gemini);
    req.instructions = Some("warm".into());
    req.speed = Some(1.2);
    req.language = Some("en-US".into());
    req.format = AudioFormat::Encoded {
        media_type: "audio/wav".into(),
    };
    let rendered = gproxy_client_sdk::speech::gemini::render(&req).unwrap();
    let value = serde_json::to_value(rendered.request).unwrap();
    assert_eq!(
        value["contents"][0]["parts"][0]["speechMetadata"]["style"],
        "warm"
    );
    assert_eq!(
        value["generationConfig"]["responseFormat"]["audio"]["mimeType"],
        "AUDIO_WAV"
    );
    assert_eq!(
        value["generationConfig"]["speechConfig"]["languageCode"],
        "en-US"
    );
    assert!(rendered.warnings.iter().any(|w| w.field == "speed"));
    req.provider_options.gemini.speech_config=Some(serde_json::from_value(json!({"multiSpeakerVoiceConfig":{"speakerVoiceConfigs":[{"speaker":"A","voiceConfig":{"voice":"Puck"}},{"speaker":"B","voiceConfig":{"voice":"Kore"}}]}})).unwrap());
    let rendered = gproxy_client_sdk::speech::gemini::render(&req).unwrap();
    assert!(rendered.warnings.iter().any(|w| w.field == "voice"));
    assert!(
        rendered
            .request
            .generation_config
            .unwrap()
            .speech_config
            .unwrap()
            .multi_speaker_voice_config
            .is_some()
    );
}
#[test]
fn speech_sse_chunks_aggregate_and_only_report_provider_usage() {
    futures_executor::block_on(async {
        let events = vec![
            json!({"type":"speech.audio.delta","audio":"AQI="}),
            json!({"type":"speech.audio.delta","audio":"Aw=="}),
            json!({"type":"speech.audio.done","usage":{"input_tokens":3,"output_tokens":4,"total_tokens":7,"characters":6}}),
        ];
        let client = Client::new(Mock::new(vec![Reply::sse(events)]));
        let result = client
            .stream_speech(&request(SpeechDialect::Openai))
            .await
            .unwrap()
            .aggregate()
            .await
            .unwrap();
        assert_eq!(result.usage.characters, Some(6));
        assert_eq!(result.usage.total_tokens, Some(7));
        assert!(
            matches!(result.audio.unwrap().content.source,Asset::Inline{bytes}if bytes==[1,2,3])
        );
        assert_eq!(
            client.backend.sent.lock().unwrap()[0].json()["stream_format"],
            "sse"
        );
    });
}
#[test]
fn gemini_audio_stream_keeps_pcm_and_late_partial_usage() {
    futures_executor::block_on(async {
        let mut first = gemini(vec![
            json!({"inlineData":{"mimeType":"audio/L16;rate=24000","data":"AQI="}}),
        ]);
        first["candidates"][0]
            .as_object_mut()
            .unwrap()
            .remove("finishReason");
        first["usageMetadata"]["promptTokensDetails"] = json!([{"modality":"TEXT","tokenCount":3}]);
        first["promptFeedback"] = json!({"blockReason":"BLOCK_REASON_UNSPECIFIED"});
        let second = gemini(vec![
            json!({"inlineData":{"mimeType":"audio/L16;rate=24000","data":"Aw=="}}),
        ]);
        let client = Client::new(Mock::new(vec![Reply::sse(vec![
            first,
            second,
            json!({"usageMetadata":{"totalTokenCount":9}}),
        ])]));
        let result = client
            .stream_speech(&request(SpeechDialect::Gemini))
            .await
            .unwrap()
            .aggregate()
            .await
            .unwrap();
        assert_eq!(result.usage.total_tokens, Some(9));
        assert_eq!(result.usage.tokens.input_tokens, Some(3));
        assert_eq!(
            result.provider_metadata[&Vendor::Gemini]["usage_details"]["prompt_tokens_details"],
            json!([{"modality":"TEXT","tokenCount":3}])
        );
        assert_eq!(
            result.provider_metadata[&Vendor::Gemini]["prompt_feedback"]["blockReason"],
            "BLOCK_REASON_UNSPECIFIED"
        );
        let audio = result.audio.unwrap();
        assert!(matches!(
            audio.content.format,
            AudioFormat::Pcm {
                sample_rate: 24000,
                channels: 1,
                ..
            }
        ));
        assert!(matches!(audio.content.source,Asset::Inline{bytes}if bytes==[1,2,3]));
        assert_eq!(
            client.backend.sent.lock().unwrap()[0].key.operation,
            protocol::Operation::StreamGenerateContent
        );
    });
}
#[test]
fn custom_voice_precedence_unknown_settings_and_claude_errors_are_visible() {
    futures_executor::block_on(async {
        let mut req = request(SpeechDialect::Openai);
        req.provider_options.openai.custom_voice =
            Some(protocol::openai::audio::CustomVoice::builder("custom".into()).build());
        req.format = AudioFormat::Unknown(json!({"type":"future"}));
        let native = gproxy_client_sdk::speech::openai::render(&req, false).unwrap();
        assert!(native.warnings.iter().any(|w| w.field == "voice"));
        assert!(native.warnings.iter().any(|w| w.field == "format"));
        assert_eq!(
            serde_json::to_value(native.request).unwrap()["voice"]["id"],
            "custom"
        );
        req.model.dialect = SpeechDialect::Unknown("claude".into());
        let client = Client::new(Mock::new(vec![]));
        assert!(client.synthesize(&req).await.is_err());
        assert!(client.backend.sent.lock().unwrap().is_empty());
        let future = json!({"type":"future_audio_event","value":17});
        let event: SpeechEvent = serde_json::from_value(future.clone()).unwrap();
        assert_eq!(serde_json::to_value(event).unwrap(), future);
    });
}
#[test]
fn cancelled_truncated_and_failed_speech_streams_do_not_fabricate_completion() {
    futures_executor::block_on(async {
        let client = Client::new(Mock::new(vec![Reply::sse(vec![
            json!({"type":"speech.audio.delta","audio":"AQID"}),
        ])]));
        let result = client
            .stream_speech(&request(SpeechDialect::Openai))
            .await
            .unwrap()
            .aggregate()
            .await
            .unwrap();
        assert_eq!(result.finish_reason.unified, FinishReasonKind::Interrupted);
        let error = json!({"error":{"type":"overloaded_error","message":"busy"}});
        let client = Client::new(Mock::new(vec![Reply::sse(vec![error.clone()])]));
        let events = client
            .stream_speech(&request(SpeechDialect::Openai))
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        assert!(events.iter().any(|e|matches!(e,SpeechEvent::Error{error:e}if e.status==Some(200)&&e.kind==ErrorKind::Overloaded)));
        assert!(
            matches!(events.last(),Some(SpeechEvent::Finish{finish_reason,..})if finish_reason.unified==FinishReasonKind::Error)
        );
    });
}

#[test]
fn raw_pcm_headers_and_buffered_gemini_signatures_retain_the_real_audio_format() {
    futures_executor::block_on(async {
        let client = Client::new(Mock::new(vec![Reply::binary("audio/pcm", vec![1, 2])]));
        let mut req = request(SpeechDialect::Openai);
        req.format = AudioFormat::pcm(24000, 1);
        let result = client.synthesize(&req).await.unwrap();
        assert_eq!(
            result.audio.unwrap().content.format,
            AudioFormat::pcm(24000, 1)
        );
        let native = gemini(vec![
            json!({"inlineData":{"mimeType":"audio/L16;rate=24000","data":"AQI="},"thoughtSignature":"audio-signature"}),
        ]);
        let client = Client::new(Mock::new(vec![Reply::json(native)]));
        let result = client
            .synthesize(&request(SpeechDialect::Gemini))
            .await
            .unwrap();
        let audio = result.audio.unwrap();
        assert_eq!(
            audio.provider_metadata[&Vendor::Gemini]["native"]["thoughtSignature"],
            "audio-signature"
        );
        #[cfg(feature = "gemini")]
        {
            let request = GenerateRequest::new(
                Model {
                    id: "gemini".into(),
                    dialect: Dialect::Gemini,
                },
                vec![Message {
                    role: Role::User,
                    content: vec![Block {
                        content: Content::Audio(audio.content),
                        cache: audio.cache,
                        provider_metadata: audio.provider_metadata,
                    }],
                }],
            );
            let rendered = dialects::gemini::render(&request).unwrap();
            let value = serde_json::to_value(rendered.request).unwrap();
            assert_eq!(
                value["contents"][0]["parts"][0]["inlineData"]["mimeType"],
                "audio/L16;rate=24000"
            );
        }
    });
}
