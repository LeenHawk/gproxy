#![cfg(all(feature = "transcription", not(target_arch = "wasm32")))]
#[path = "support/media.rs"]
mod support;
use futures_util::StreamExt;
use gproxy_client_sdk::{transcription::*, *};
use serde_json::json;
use support::*;
fn request(dialect: TranscriptionDialect) -> TranscriptionRequest {
    TranscriptionRequest::new(
        TranscriptionModel {
            id: "provider/transcribe".into(),
            dialect,
        },
        audio(),
    )
}
fn verbose() -> serde_json::Value {
    json!({"task":"transcribe","duration":1.75,"language":"english","text":"hello","usage":{"type":"duration","seconds":2},"segments":[{"id":0,"avg_logprob":-0.1,"compression_ratio":1,"no_speech_prob":0,"seek":0,"start":0.1,"end":1.5,"temperature":0,"text":"hello","tokens":[1,2]}],"words":[{"word":"hello","start":0.1,"end":1.5}]})
}
#[test]
fn native_upload_selects_verbose_json_and_keeps_duration_billing_separate() {
    futures_executor::block_on(async {
        let client = Client::new(Mock::new(vec![Reply::json(verbose())]));
        let mut req = request(TranscriptionDialect::Openai);
        req.language = Some("en".into());
        req.prompt = Some("proper names".into());
        req.timestamps = vec![TimestampGranularity::Word, TimestampGranularity::Segment];
        let result = client.transcribe_audio(&req).await.unwrap();
        assert_eq!(result.text, "hello");
        assert_eq!(result.language.as_deref(), Some("english"));
        assert_eq!(result.duration, Some(1.75));
        assert_eq!(result.usage.audio_seconds, Some(2.0));
        assert_eq!(result.words[0].start, Some(0.1));
        assert_eq!(result.segments[0].end, Some(1.5));
        assert_eq!(
            result.segments[0].provider_metadata[&Vendor::Openai]["avg_logprob"],
            -0.1
        );
        let sent = client.backend.sent.lock().unwrap();
        assert_eq!(
            sent[0].key.operation,
            protocol::Operation::CreateTranscription
        );
        assert_eq!(sent[0].path, "/v1/audio/transcriptions");
        let bytes = String::from_utf8_lossy(&sent[0].bytes);
        assert!(bytes.contains("name=\"response_format\"\r\n\r\nverbose_json"));
        assert!(bytes.contains("name=\"timestamp_granularities[]\"\r\n\r\nword"));
        assert!(bytes.contains("filename=\"audio.wav\""));
        assert!(bytes.contains("proper names"));
    });
}
#[test]
fn diarization_uses_diarized_json_and_reports_unsupported_word_controls() {
    futures_executor::block_on(async {
        let native = json!({"duration":2,"task":"transcribe","text":"hello","segments":[{"id":"a","start":0,"end":2,"speaker":"Alice","text":"hello","type":"transcript.text.segment"}],"usage":{"type":"duration","seconds":3}});
        let client = Client::new(Mock::new(vec![Reply::json(native)]));
        let mut req = request(TranscriptionDialect::Openai);
        req.diarization = true;
        req.timestamps = vec![TimestampGranularity::Word];
        req.prompt = Some("hint".into());
        let result = client.transcribe_audio(&req).await.unwrap();
        assert_eq!(result.segments[0].speaker.as_deref(), Some("Alice"));
        assert_eq!(result.usage.audio_seconds, Some(3.0));
        assert!(result.warnings.iter().any(|w| w.field == "timestamps.word"));
        assert!(result.warnings.iter().any(|w| w.field == "prompt"));
        assert!(
            String::from_utf8_lossy(&client.backend.sent.lock().unwrap()[0].bytes)
                .contains("diarized_json")
        );
    });
}
#[test]
fn gemini_direct_transcription_parses_reported_offsets_and_speaker_labels() {
    futures_executor::block_on(async {
        let native = gemini(vec![
            json!({"audioTranscription":{"text":"hello","speakerLabel":"spk_1","words":[{"word":"hello","startOffset":"0.1s","endOffset":"1.5s","confidence":0.9}]}}),
        ]);
        let client = Client::new(Mock::new(vec![Reply::json(native)]));
        let mut req = request(TranscriptionDialect::Gemini);
        req.language = Some("en-US".into());
        req.prompt = Some("accurate spelling".into());
        req.timestamps = vec![TimestampGranularity::Segment, TimestampGranularity::Word];
        req.diarization = true;
        let result = client.transcribe_audio(&req).await.unwrap();
        assert_eq!(result.text, "hello");
        assert_eq!(result.words[0].start, Some(0.1));
        assert_eq!(result.words[0].end, Some(1.5));
        assert_eq!(result.words[0].speaker.as_deref(), Some("spk_1"));
        assert_eq!(result.segments[0].start, Some(0.1));
        assert_eq!(result.language, None);
        assert_eq!(result.duration, None);
        assert_eq!(result.usage.audio_seconds, None);
        assert_eq!(result.response.model.as_deref(), Some("served-gemini"));
        let sent = client.backend.sent.lock().unwrap();
        assert_eq!(sent[0].key.operation, protocol::Operation::GenerateContent);
        assert_eq!(sent[0].key.dialect, protocol::Dialect::Gemini);
        let body = sent[0].json();
        assert_eq!(
            body["generationConfig"]["audioTranscriptionConfig"]["wordTimestamp"],
            true
        );
        assert_eq!(
            body["generationConfig"]["audioTranscriptionConfig"]["diarization"],
            true
        );
        assert_eq!(body["contents"][0]["parts"][0]["text"], "accurate spelling");
        assert_eq!(
            body["contents"][0]["parts"][1]["inlineData"]["data"],
            "AQID"
        );
    });
}
#[test]
fn conflicting_gemini_controls_and_unmappable_inputs_fail_before_io() {
    futures_executor::block_on(async {
        let client = Client::new(Mock::new(vec![]));
        let mut req = request(TranscriptionDialect::Gemini);
        req.diarization = true;
        req.provider_options.gemini.config =
            Some(serde_json::from_value(json!({"mode":"SMART"})).unwrap());
        assert!(client.transcribe_audio(&req).await.is_err());
        req.provider_options.gemini.config =
            Some(serde_json::from_value(json!({"customVocabulary":["name"]})).unwrap());
        assert!(client.transcribe_audio(&req).await.is_err());
        req.diarization = false;
        req.provider_options.gemini.config =
            Some(serde_json::from_value(json!({"languageAuto":{},"languageHints":{}})).unwrap());
        assert!(client.transcribe_audio(&req).await.is_err());
        let mut req = request(TranscriptionDialect::Openai);
        req.audio.content.source = Asset::Url {
            url: "https://store.example/file.wav".into(),
        };
        assert!(client.transcribe_audio(&req).await.is_err());
        req.model.dialect = TranscriptionDialect::Unknown("claude".into());
        assert!(client.transcribe_audio(&req).await.is_err());
        assert!(client.backend.sent.lock().unwrap().is_empty());
    });
}
#[test]
fn preparation_obeys_inline_only_openai_and_preserves_persisted_references() {
    futures_executor::block_on(async {
        let mut req = request(TranscriptionDialect::Openai);
        req.audio.content.source = Asset::Reference {
            reference: "audio-blob".into(),
        };
        let before = serde_json::to_value(&req).unwrap();
        let prepared = req
            .resolve_assets(DeliveryMode::PreferUrl, |input| async move {
                assert_eq!(input.target, AssetDialect::OpenaiTranscription);
                assert_eq!(input.preferred, DeliveryMethod::Inline);
                assert!(input.capability.url.is_none());
                Ok(ResolvedAsset::Bytes {
                    bytes: vec![1, 2, 3],
                    media_type: "audio/wav".into(),
                })
            })
            .await
            .unwrap();
        let client = Client::new(Mock::new(vec![
            Reply::json(json!({"text":"heard"})),
            Reply::json(gemini(vec![json!({"audioTranscription":{"text":"heard"}})])),
        ]));
        assert_eq!(
            client
                .transcribe_audio_prepared(prepared)
                .await
                .unwrap()
                .text,
            "heard"
        );
        assert_eq!(serde_json::to_value(&req).unwrap(), before);
        let future = json!({"type":"future_transcription","data":1});
        let parsed: TranscriptionEvent = serde_json::from_value(future.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), future);
    });
}
#[test]
fn streamed_deltas_segments_done_and_interruption_remain_distinct() {
    futures_executor::block_on(async {
        let events = vec![
            json!({"type":"transcript.text.delta","delta":"hel"}),
            json!({"type":"transcript.text.segment","id":"s","start":0.1,"end":1.0,"speaker":"A","text":"hello"}),
            json!({"type":"transcript.text.done","text":"hello","usage":{"type":"tokens","input_tokens":2,"output_tokens":3,"total_tokens":5}}),
        ];
        let client = Client::new(Mock::new(vec![Reply::sse(events)]));
        let result = client
            .stream_transcription(&request(TranscriptionDialect::Openai))
            .await
            .unwrap()
            .aggregate()
            .await
            .unwrap();
        assert_eq!(result.text, "hello");
        assert_eq!(result.segments[0].speaker.as_deref(), Some("A"));
        assert_eq!(result.usage.total_tokens, Some(5));
        assert_eq!(result.finish_reason.unified, FinishReasonKind::Stop);
        let client = Client::new(Mock::new(vec![Reply::sse(vec![
            json!({"type":"transcript.text.delta","delta":"partial"}),
        ])]));
        let result = client
            .stream_transcription(&request(TranscriptionDialect::Openai))
            .await
            .unwrap()
            .aggregate()
            .await
            .unwrap();
        assert_eq!(result.text, "partial");
        assert_eq!(result.finish_reason.unified, FinishReasonKind::Interrupted);
    });
}
#[test]
fn gemini_streams_annotations_and_verbose_openai_streams_warn_about_buffering() {
    futures_executor::block_on(async {
        let native = gemini(vec![
            json!({"audioTranscription":{"text":"hello","words":[{"word":"hello","startOffset":"0.1s","endOffset":"1.0s"}]}}),
        ]);
        let client = Client::new(Mock::new(vec![Reply::sse(vec![
            native,
            json!({"usageMetadata":{"totalTokenCount":8}}),
        ])]));
        let result = client
            .stream_transcription(&request(TranscriptionDialect::Gemini))
            .await
            .unwrap()
            .aggregate()
            .await
            .unwrap();
        assert_eq!(result.text, "hello");
        assert_eq!(result.usage.total_tokens, Some(8));
        assert_eq!(result.usage.tokens.input_tokens, Some(3));
        let client = Client::new(Mock::new(vec![Reply::json(verbose())]));
        let mut req = request(TranscriptionDialect::Openai);
        req.timestamps.push(TimestampGranularity::Word);
        let events = client
            .stream_transcription(&req)
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        assert!(
            matches!(&events[0],TranscriptionEvent::Start{warnings}if warnings.iter().any(|w|w.field=="stream"))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TranscriptionEvent::Segment { .. }))
        );
    });
}
#[test]
fn malformed_offsets_and_native_errors_preserve_the_original_body_and_http_status() {
    futures_executor::block_on(async {
        let native = gemini(vec![
            json!({"audioTranscription":{"text":"x","words":[{"word":"x","startOffset":"invalid"}]}}),
        ]);
        let client = Client::new(Mock::new(vec![Reply::json(native.clone())]));
        let error = client
            .transcribe_audio(&request(TranscriptionDialect::Gemini))
            .await
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidData);
        assert_eq!(error.status, Some(200));
        assert_eq!(error.body.as_deref(), Some(native.to_string().as_str()));
        let error = json!({"error":{"code":402,"message":"pay"}});
        let client = Client::new(Mock::new(vec![Reply::sse(vec![error.clone()])]));
        let events = client
            .stream_transcription(&request(TranscriptionDialect::Openai))
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;
        assert!(events.iter().any(|e|matches!(e,TranscriptionEvent::Error{error:e}if e.kind==ErrorKind::PaymentRequired&&e.status==Some(200)&&e.body.as_deref()==Some(error.to_string().as_str()))));
    });
}

#[cfg(feature = "s3")]
#[path = "support/s3.rs"]
mod store;
#[cfg(feature = "s3")]
#[tokio::test]
async fn configured_s3_transcription_client_keeps_openai_uploads_inline() {
    let fixture = store::fixture(store::config());
    let client = Client::new(Mock::new(vec![
        Reply::json(json!({"text":"heard"})),
        Reply::json(gemini(vec![json!({"audioTranscription":{"text":"heard"}})])),
    ]))
    .with_s3(fixture.store);
    let mut req = request(TranscriptionDialect::Openai);
    req.audio.content.source = Asset::Reference {
        reference: "audio-blob".into(),
    };
    let before = serde_json::to_value(&req).unwrap();
    let result = client
        .transcribe_audio(&req, |id| async move {
            assert_eq!(id, "audio-blob");
            Ok(s3::AssetData {
                bytes: vec![1, 2, 3],
                media_type: "audio/wav".into(),
            })
        })
        .await
        .unwrap();
    assert_eq!(result.text, "heard");
    assert!(fixture.transport.requests.lock().unwrap().is_empty());
    assert_eq!(
        fixture
            .probe
            .presigns
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    assert_eq!(serde_json::to_value(&req).unwrap(), before);
    req.model.dialect = TranscriptionDialect::Gemini;
    client
        .transcribe_audio(&req, |_| async {
            Ok(s3::AssetData {
                bytes: vec![1, 2, 3],
                media_type: "audio/wav".into(),
            })
        })
        .await
        .unwrap();
    let recorded = fixture.transport.requests.lock().unwrap();
    assert!(
        recorded
            .iter()
            .any(|r| r.method == http::Method::PUT && r.mime.as_deref() == Some("audio/wav"))
    );
    assert!(
        recorded
            .iter()
            .all(|r| r.uri.host() == Some("account.r2.cloudflarestorage.com"))
    );
}
#[test]
fn native_known_speakers_select_diarized_output_even_when_portable_flag_is_unset() {
    futures_executor::block_on(async {
        let native = json!({"duration":1,"task":"transcribe","text":"hello","segments":[{"id":"a","start":0,"end":1,"speaker":"Alice","text":"hello","type":"transcript.text.segment"}]});
        let client = Client::new(Mock::new(vec![Reply::json(native)]));
        let mut req = request(TranscriptionDialect::Openai);
        req.provider_options.openai.known_speaker_names = Some(vec!["Alice".into()]);
        let result = client
            .stream_transcription(&req)
            .await
            .unwrap()
            .aggregate()
            .await
            .unwrap();
        assert_eq!(result.segments[0].speaker.as_deref(), Some("Alice"));
    });
}
