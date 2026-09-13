use gproxy_protocol::openai::{audio as a, images as i};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
fn round<T: Serialize + DeserializeOwned>(v: Value) -> T {
    let x: T = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(serde_json::to_value(&x).unwrap(), v);
    x
}
fn segment() -> Value {
    json!({"id":0,"avg_logprob":-0.5,"compression_ratio":0.3,"end":1.5,"no_speech_prob":0.1,"seek":0,"start":0.0,"temperature":0.2,"text":"hello","tokens":[1,2]})
}
fn tokens() -> Value {
    json!({"type":"tokens","input_tokens":2,"output_tokens":3,"total_tokens":5,"input_token_details":{"audio_tokens":1,"text_tokens":1}})
}
fn image_usage() -> Value {
    json!({"input_tokens":2,"input_tokens_details":{"image_tokens":1,"text_tokens":1},"output_tokens":3,"total_tokens":5,"output_tokens_details":{"image_tokens":2,"text_tokens":1}})
}
#[test]
fn speech_request_and_both_official_sse_events_are_typed() {
    let p = round::<a::CreateSpeechRequestBody>(
        json!({"model":"future-tts","input":"hello","voice":{"id":"voice_1"},"instructions":"calm","response_format":"pcm","speed":1.2,"stream_format":"sse"}),
    );
    assert!(p.rest.is_empty());
    match p.voice {
        a::SpeechVoice::Custom(v) => assert!(v.rest.is_empty()),
        _ => panic!(),
    }
    // Official openapi.json SpeechAudioDeltaEvent / SpeechAudioDoneEvent examples.
    let p = round::<a::SpeechAudioEvent>(
        json!({"type":"speech.audio.delta","audio":"base64-encoded-audio-data"}),
    );
    match p {
        a::SpeechAudioEvent::Delta(v) => assert!(v.rest.is_empty()),
        _ => panic!(),
    }
    let p = round::<a::SpeechAudioEvent>(
        json!({"type":"speech.audio.done","usage":{"input_tokens":14,"output_tokens":101,"total_tokens":115}}),
    );
    match p {
        a::SpeechAudioEvent::Done(v) => {
            assert!(v.rest.is_empty());
            assert!(v.usage.rest.is_empty());
        }
        _ => panic!(),
    }
    assert!(
        serde_json::from_value::<a::SpeechAudioEvent>(json!({"type":"audio.delta","audio":"x"}))
            .is_err()
    );
    assert!(
        serde_json::from_value::<a::SpeechAudioEvent>(json!({"type":"speech.audio.done"})).is_err()
    );
    for field in ["instructions", "response_format", "speed", "stream_format"] {
        let mut v = json!({"model":"m","input":"x","voice":"future-voice"});
        round::<a::CreateSpeechRequestBody>(v.clone());
        v[field] = Value::Null;
        assert!(serde_json::from_value::<a::CreateSpeechRequestBody>(v).is_err());
    }
}
#[test]
fn all_transcription_and_translation_json_formats_have_known_fields() {
    let p = round::<a::TranscriptionJson>(
        json!({"text":"hello","languages":[{"code":"en"}],"logprobs":[{}, {"token":"hello","bytes":[1],"logprob":-0.5}],"usage":tokens()}),
    );
    assert!(p.rest.is_empty());
    assert!(p.languages.unwrap()[0].rest.is_empty());
    for l in p.logprobs.unwrap() {
        assert!(l.rest.is_empty());
    }
    let p = round::<a::TranscriptionVerbose>(
        json!({"duration":1.5,"language":"english","text":"hello","segments":[segment()],"usage":{"seconds":2,"type":"duration"},"words":[{"word":"hello","start":0.0,"end":1.5}]}),
    );
    assert!(p.rest.is_empty());
    assert!(p.segments.unwrap()[0].rest.is_empty());
    assert!(p.words.unwrap()[0].rest.is_empty());
    assert!(p.usage.unwrap().rest.is_empty());
    let p = round::<a::TranscriptionDiarized>(
        json!({"duration":1.5,"segments":[{"id":"seg_1","end":1.5,"speaker":"A","start":0.0,"text":"hello","type":"transcript.text.segment"}],"task":"transcribe","text":"hello","usage":tokens()}),
    );
    assert!(p.rest.is_empty());
    assert!(p.segments[0].rest.is_empty());
    let p = round::<a::TranslationJson>(json!({"text":"hello"}));
    assert!(p.rest.is_empty());
    let p = round::<a::TranslationVerbose>(
        json!({"duration":1.5,"language":"english","text":"hello","segments":[segment()]}),
    );
    assert!(p.rest.is_empty());
    assert!(p.segments.unwrap()[0].rest.is_empty());
    for usage in [tokens(), json!({"seconds":1.5,"type":"duration"})] {
        let p = round::<a::TranscriptionUsage>(usage);
        match p {
            a::TranscriptionUsage::Tokens(v) => {
                assert!(v.rest.is_empty());
                assert!(v.input_token_details.unwrap().rest.is_empty());
            }
            a::TranscriptionUsage::Duration(v) => assert!(v.rest.is_empty()),
            #[cfg(not(feature = "exhaustive"))]
            _ => panic!(),
        }
    }
    assert!(
        serde_json::from_value::<a::TranscriptionVerbose>(json!({"text":"x","duration":1}))
            .is_err()
    );
    assert!(
        serde_json::from_value::<a::TranscriptionVerbose>(
            json!({"text":"x","duration":1,"language":"en","usage":tokens()})
        )
        .is_err()
    );
    for field in ["speaker", "type"] {
        let mut v = json!({"id":"s","end":1,"start":0,"speaker":"A","text":"x","type":"transcript.text.segment"});
        v.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<a::TranscriptionDiarizedSegment>(v).is_err());
    }
}
#[test]
fn transcription_sse_delta_done_segment_keep_exact_native_shapes() {
    let delta = round::<a::TranscriptionStreamEvent>(
        json!({"type":"transcript.text.delta","delta":"hello","logprobs":[{}],"segment_id":"s"}),
    );
    match delta {
        a::TranscriptionStreamEvent::Text(a::TranscriptionTextStreamEvent::Delta(v)) => {
            assert!(v.rest.is_empty());
            assert!(v.logprobs.unwrap()[0].rest.is_empty());
        }
        _ => panic!(),
    }
    let p = round::<a::TranscriptionStreamEvent>(
        json!({"type":"transcript.text.done","text":"hello","languages":[{"code":"en"}],"logprobs":[{}],"usage":tokens()}),
    );
    match p {
        a::TranscriptionStreamEvent::Text(a::TranscriptionTextStreamEvent::Done(v)) => {
            assert!(v.rest.is_empty())
        }
        _ => panic!(),
    }
    let p = round::<a::TranscriptionStreamEvent>(
        json!({"type":"transcript.text.segment","id":"seg_002","start":5.2,"end":12.8,"text":"Hi, I need help with diarization.","speaker":"A"}),
    );
    assert!(matches!(p, a::TranscriptionStreamEvent::Segment(_)));
    assert!(serde_json::from_value::<a::TranscriptionStreamEvent>(json!({"type":"transcript.text.done","text":"x","usage":{"type":"duration","seconds":1}})).is_err());
    let wrong = json!({"type":"duration","input_tokens":1,"output_tokens":2,"total_tokens":3});
    assert!(serde_json::from_value::<a::TranscriptionUsage>(wrong).is_err());
}
fn create_image() -> Value {
    json!({"prompt":"otter","background":"transparent","model":"future-image","moderation":"auto","n":1,"output_compression":80,"output_format":"png","partial_images":1,"quality":"high","response_format":"b64_json","size":"custom-size","stream":true,"style":"natural","user":"u"})
}
fn edit_image() -> Value {
    json!({"images":[{"file_id":"f","image_url":"https://x"}],"prompt":"edit","background":"auto","input_fidelity":"high","mask":{"image_url":"https://mask"},"model":"future-image","moderation":"low","n":1,"output_compression":80,"output_format":"webp","partial_images":1,"quality":"auto","size":"1024x1024","stream":false,"user":"u"})
}
#[test]
fn image_requests_and_response_distinguish_nullable_and_closed_sets() {
    let p = round::<i::CreateImageRequestBody>(create_image());
    assert!(p.rest.is_empty());
    let p = round::<i::EditImageJsonBody>(edit_image());
    assert!(p.rest.is_empty());
    assert!(p.images[0].rest.is_empty());
    assert!(p.mask.unwrap().rest.is_empty());
    for base in [create_image(), edit_image()] {
        let edit = base.get("images").is_some();
        for field in base.as_object().unwrap().keys() {
            if ["prompt", "images", "user", "mask"].contains(&field.as_str()) {
                continue;
            }
            let mut v = base.clone();
            v[field] = Value::Null;
            if edit {
                round::<i::EditImageJsonBody>(v.clone());
            } else {
                round::<i::CreateImageRequestBody>(v.clone());
            }
            v.as_object_mut().unwrap().remove(field);
            if edit {
                round::<i::EditImageJsonBody>(v);
            } else {
                round::<i::CreateImageRequestBody>(v);
            }
        }
    }
    let p = round::<i::ImagesResponse>(
        json!({"created":123,"background":"opaque","data":[{"b64_json":"abc","url":"https://x","revised_prompt":"otter"}],"output_format":"png","quality":"high","size":"1024x1024","usage":image_usage()}),
    );
    assert!(p.rest.is_empty());
    assert!(p.data.unwrap()[0].rest.is_empty());
    let u = p.usage.unwrap();
    assert!(u.rest.is_empty());
    assert!(u.input_tokens_details.rest.is_empty());
    assert!(u.output_tokens_details.unwrap().rest.is_empty());
    round::<i::ImagesResponse>(json!({"created":1}));
    for (field, value) in [
        ("background", json!("auto")),
        ("quality", json!("hd")),
        ("size", json!("auto")),
        ("data", Value::Null),
    ] {
        let mut v = json!({"created":1});
        v[field] = value;
        assert!(serde_json::from_value::<i::ImagesResponse>(v).is_err());
    }
    let mut v = edit_image();
    v["quality"] = json!("hd");
    assert!(serde_json::from_value::<i::EditImageJsonBody>(v).is_err());
    let mut v = edit_image();
    v["mask"] = Value::Null;
    assert!(serde_json::from_value::<i::EditImageJsonBody>(v).is_err());
}
fn partial() -> Value {
    json!({"b64_json":"abc","background":"auto","created_at":123,"output_format":"png","partial_image_index":0,"quality":"xhigh","size":"custom-size"})
}
#[test]
fn generation_and_edit_sse_cover_all_required_fields() {
    for (tag, edit, done) in [
        ("image_generation.partial_image", false, false),
        ("image_generation.completed", false, true),
        ("image_edit.partial_image", true, false),
        ("image_edit.completed", true, true),
    ] {
        let mut v = partial();
        v["type"] = json!(tag);
        if done {
            v.as_object_mut().unwrap().remove("partial_image_index");
            v["usage"] = image_usage();
        }
        if edit {
            let p = round::<i::ImageEditStreamEvent>(v.clone());
            match p {
                i::ImageEditStreamEvent::Partial(v) => assert!(v.rest.is_empty()),
                i::ImageEditStreamEvent::Completed(v) => {
                    assert!(v.rest.is_empty());
                    assert!(v.usage.rest.is_empty());
                }
                #[cfg(not(feature = "exhaustive"))]
                _ => panic!(),
            }
        } else {
            let p = round::<i::ImageGenerationStreamEvent>(v.clone());
            match p {
                i::ImageGenerationStreamEvent::Partial(v) => assert!(v.rest.is_empty()),
                i::ImageGenerationStreamEvent::Completed(v) => {
                    assert!(v.rest.is_empty());
                    assert!(v.usage.rest.is_empty());
                }
                #[cfg(not(feature = "exhaustive"))]
                _ => panic!(),
            }
        }
        for field in v.as_object().unwrap().keys() {
            let mut missing = v.clone();
            missing.as_object_mut().unwrap().remove(field);
            let mut null = v.clone();
            null[field] = Value::Null;
            for invalid in [missing, null] {
                if edit {
                    assert!(
                        serde_json::from_value::<i::ImageEditStreamEvent>(invalid).is_err(),
                        "{tag} {field}"
                    );
                } else {
                    assert!(
                        serde_json::from_value::<i::ImageGenerationStreamEvent>(invalid).is_err(),
                        "{tag} {field}"
                    );
                }
            }
        }
    }
}
fn closed<T: Serialize + DeserializeOwned>(values: &[&str]) {
    for v in values {
        round::<T>(json!(v));
    }
    assert!(serde_json::from_value::<T>(json!("unknown")).is_err());
}
#[test]
fn format_and_configuration_enums_use_source_values() {
    closed::<a::AudioResponseFormat>(&["mp3", "opus", "aac", "flac", "wav", "pcm"]);
    closed::<a::SpeechStreamFormat>(&["sse", "audio"]);
    closed::<a::TranscriptionResponseFormat>(&[
        "json",
        "text",
        "srt",
        "verbose_json",
        "vtt",
        "diarized_json",
    ]);
    closed::<a::TranslationResponseFormat>(&["json", "text", "srt", "verbose_json", "vtt"]);
    closed::<a::TranscriptionTimestampGranularity>(&["word", "segment"]);
    closed::<a::TranscriptionInclude>(&["logprobs"]);
    closed::<a::TranscriptionTask>(&["transcribe"]);
    assert!(
        serde_json::from_value::<a::TranslationResponseFormat>(json!("diarized_json")).is_err()
    );
    let p = round::<a::ChunkingStrategy>(
        json!({"type":"server_vad","prefix_padding_ms":100,"silence_duration_ms":500,"threshold":0.5}),
    );
    match p {
        a::ChunkingStrategy::Vad(v) => assert!(v.rest.is_empty()),
        _ => panic!(),
    };
    round::<a::ChunkingStrategy>(json!("auto"));
    closed::<i::ImageBackground>(&["transparent", "opaque", "auto"]);
    closed::<i::ImageModeration>(&["low", "auto"]);
    closed::<i::ImageOutputFormat>(&["png", "jpeg", "webp"]);
    closed::<i::ImageQuality>(&["standard", "hd", "low", "medium", "high", "auto"]);
    closed::<i::EditedImageQuality>(&["low", "medium", "high", "auto"]);
    closed::<i::MultipartImageQuality>(&[
        "standard", "low", "medium", "high", "xhigh", "max", "auto",
    ]);
    closed::<i::StreamImageQuality>(&["low", "medium", "high", "xhigh", "max", "auto"]);
    closed::<i::ImageResponseFormat>(&["url", "b64_json"]);
    closed::<i::ImageStyle>(&["vivid", "natural"]);
    closed::<i::InputFidelity>(&["high", "low"]);
}
#[test]
fn unknown_extensions_do_not_redirect_typed_unions() {
    let mut v = create_image();
    v["future"] = json!({"data":1});
    let p = round::<i::CreateImageRequestBody>(v);
    assert_eq!(p.rest.len(), 1);
    let p = round::<a::TranscriptionUsage>(
        json!({"type":"duration","seconds":1,"input_tokens":2,"output_tokens":3,"total_tokens":5}),
    );
    match p {
        a::TranscriptionUsage::Duration(v) => assert_eq!(v.rest.len(), 3),
        _ => panic!(),
    }
    let p = round::<a::TranscriptionJson>(json!({"text":"x","duration":1}));
    assert!(p.rest.contains_key("duration"));
    let p = round::<i::ImageReference>(json!({"fileId":"x"}));
    assert!(p.file_id.is_none());
    assert!(p.rest.contains_key("fileId"));
}

#[test]
fn multipart_forms_keep_streaming_parts_and_all_known_metadata() {
    use gproxy_protocol::connection::{HttpBody, MultipartPart};
    struct Stream;
    impl futures_core::Stream for Stream {
        type Item = Result<bytes::Bytes, gproxy_protocol::connection::TransportError>;
        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            std::task::Poll::Ready(None)
        }
    }
    fn file() -> MultipartPart {
        let mut h = http::HeaderMap::new();
        h.insert(
            "content-disposition",
            http::HeaderValue::from_static("form-data; name=\"file\"; filename=\"voice.wav\""),
        );
        MultipartPart {
            headers: h,
            body: HttpBody::Stream(Box::pin(Stream)),
        }
    }
    let transcription =
        a::CreateTranscriptionMultipartForm::builder(file(), "future-transcribe".into())
            .language("en")
            .languages(vec!["en".into()])
            .keywords(vec!["OpenAI".into()])
            .prompt("hello")
            .response_format(a::TranscriptionResponseFormat::DiarizedJson)
            .temperature(serde_json::Number::from(0))
            .timestamp_granularities(vec![a::TranscriptionTimestampGranularity::Word])
            .include(vec![a::TranscriptionInclude::Logprobs])
            .chunking_strategy(Some(a::ChunkingStrategy::Auto(a::AutomaticChunking::Auto)))
            .known_speaker_names(vec!["A".into()])
            .known_speaker_references(vec!["data:audio/wav;base64,AA==".into()])
            .stream(Some(true))
            .build();
    assert!(matches!(transcription.file.body, HttpBody::Stream(_)));
    assert_eq!(transcription.known_speaker_names.as_ref().unwrap()[0], "A");
    assert_eq!(transcription.stream, Some(Some(true)));
    assert!(transcription.include.is_some());
    let request: a::CreateTranscriptionRequest = gproxy_protocol::WireRequest {
        method: http::Method::POST,
        path: "/v1/audio/transcriptions".into(),
        query: Some("x=1".into()),
        headers: http::HeaderMap::new(),
        body: transcription,
    };
    assert_eq!(request.method, http::Method::POST);
    assert_eq!(request.path, "/v1/audio/transcriptions");
    assert_eq!(request.query.as_deref(), Some("x=1"));
    assert!(request.headers.is_empty());
    assert_eq!(request.body.model, "future-transcribe");
    let translation = a::CreateTranslationMultipartForm::builder(file(), "whisper-1".into())
        .prompt("hello")
        .response_format(a::TranslationResponseFormat::Vtt)
        .temperature(serde_json::Number::from(0))
        .build();
    assert!(matches!(translation.file.body, HttpBody::Stream(_)));
    let form = i::EditImageMultipartForm::builder(vec![file()], "edit".into())
        .mask(file())
        .background(Some(i::ImageBackground::Auto))
        .model(Some("future-image".into()))
        .n(Some(1))
        .output_compression(Some(80))
        .output_format(Some(i::ImageOutputFormat::Png))
        .partial_images(Some(1))
        .input_fidelity(Some(i::InputFidelity::High))
        .quality(Some(i::MultipartImageQuality::Xhigh))
        .size(Some("custom-size".into()))
        .response_format(Some(i::ImageResponseFormat::Url))
        .stream(Some(false))
        .user("u")
        .build();
    assert!(matches!(form.image[0].body, HttpBody::Stream(_)));
    assert!(matches!(form.mask.unwrap().body, HttpBody::Stream(_)));
    assert_eq!(
        form.response_format,
        Some(Some(i::ImageResponseFormat::Url))
    );
    let speech: a::CreateSpeechResponse = gproxy_protocol::WireResponse {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: HttpBody::Stream(Box::pin(Stream)),
    };
    assert_eq!(speech.status, http::StatusCode::OK);
    assert!(speech.headers.is_empty());
    assert!(matches!(speech.body, HttpBody::Stream(_)));
    // text, SRT and VTT are raw text payloads, not invented JSON response objects.
    for bytes in [
        b"hello".as_slice(),
        b"1\n00:00:00,000 --> 00:00:01,000\nhello\n",
        b"WEBVTT\n\n00:00.000 --> 00:01.000\nhello\n",
    ] {
        let response: a::CreateTranslationResponse = gproxy_protocol::WireResponse {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: HttpBody::Bytes(bytes::Bytes::copy_from_slice(bytes)),
        };
        let HttpBody::Bytes(actual) = response.body else {
            panic!()
        };
        assert_eq!(actual.as_ref(), bytes);
    }
    let image: i::CreateImageResponse = gproxy_protocol::WireResponse {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: i::ImagesResponse::builder(1).build(),
    };
    assert_eq!(
        serde_json::to_value(image.body).unwrap(),
        json!({"created":1})
    );
    let created = i::CreateImageRequestBody::builder("otter".into())
        .background(None)
        .build();
    assert_eq!(
        serde_json::to_value(created).unwrap(),
        json!({"prompt":"otter","background":null})
    );
    let speech = a::CreateSpeechRequestBody::builder(
        "m".into(),
        "hello".into(),
        a::SpeechVoice::Name("custom-voice".into()),
    )
    .build();
    assert_eq!(
        serde_json::to_value(speech).unwrap(),
        json!({"model":"m","input":"hello","voice":"custom-voice"})
    );
}

#[test]
fn local_official_json_examples_and_sse_examples_match_verified_contracts() {
    let docs = [
        (
            "transcription",
            include_str!("../../../upstream_docs/openai/docs/Create transcription.md"),
        ),
        (
            "translation",
            include_str!("../../../upstream_docs/openai/docs/Create translation.md"),
        ),
        (
            "image",
            include_str!("../../../upstream_docs/openai/docs/Create image.md"),
        ),
        (
            "edit",
            include_str!("../../../upstream_docs/openai/docs/Create image edit.md"),
        ),
    ];
    let mut json_examples = 0;
    let mut transcription_events = 0;
    let mut abbreviated_images = 0;
    for (kind, doc) in docs {
        for part in doc.split("#### Response\n\n```json\n").skip(1) {
            let body = part.split("\n```").next().unwrap();
            if let Ok(v) = serde_json::from_str::<Value>(body) {
                json_examples += 1;
                match kind {
                    "transcription" => {
                        if v.get("segments").is_some() && v.get("language").is_none() {
                            let p = round::<a::TranscriptionDiarized>(v);
                            assert!(p.rest.is_empty());
                        } else if v.get("language").is_some() {
                            let p = round::<a::TranscriptionVerbose>(v);
                            assert!(p.rest.is_empty());
                        } else {
                            let p = round::<a::TranscriptionJson>(v);
                            assert!(p.rest.is_empty());
                        }
                    }
                    "translation" => {
                        let p = round::<a::TranslationJson>(v);
                        assert!(p.rest.is_empty());
                    }
                    _ => {
                        let p = round::<i::ImagesResponse>(v);
                        assert!(p.rest.is_empty());
                    }
                }
            }
        }
        for line in doc.lines() {
            if let Some(data) = line.strip_prefix("data: ")
                && let Ok(v) = serde_json::from_str::<Value>(data)
            {
                if kind == "transcription" {
                    // Local example line 602 omits the required usage.type.
                    // SDK TranscriptTextDoneEvent.Usage and the OpenAPI
                    // TranscriptTextUsageTokens both require literal tokens.
                    if v["type"] == "transcript.text.done" && v["usage"].get("type").is_none() {
                        assert!(serde_json::from_value::<a::TranscriptionStreamEvent>(v).is_err());
                        continue;
                    }
                    round::<a::TranscriptionStreamEvent>(v);
                    transcription_events += 1;
                } else if kind == "image" {
                    assert!(serde_json::from_value::<i::ImageGenerationStreamEvent>(v).is_err());
                    abbreviated_images += 1;
                } else if kind == "edit" {
                    assert!(serde_json::from_value::<i::ImageEditStreamEvent>(v).is_err());
                    abbreviated_images += 1;
                }
            }
        }
    }
    assert!(json_examples >= 9);
    assert!(transcription_events >= 2);
    assert_eq!(abbreviated_images, 4);
}
#[test]
fn format_field_matrices_reject_missing_required_and_null_optional() {
    fn fields<T: Serialize + DeserializeOwned>(base: Value, required: &[&str], optional: &[&str]) {
        round::<T>(base.clone());
        for field in required {
            let mut v = base.clone();
            v.as_object_mut().unwrap().remove(*field);
            assert!(
                serde_json::from_value::<T>(v).is_err(),
                "{} missing {field}",
                std::any::type_name::<T>()
            );
        }
        for field in optional {
            let mut v = base.clone();
            v[*field] = Value::Null;
            assert!(
                serde_json::from_value::<T>(v).is_err(),
                "{} null {field}",
                std::any::type_name::<T>()
            );
        }
    }
    fields::<a::TranscriptionJson>(
        json!({"text":"x"}),
        &["text"],
        &["languages", "logprobs", "usage"],
    );
    fields::<a::TranscriptionVerbose>(
        json!({"text":"x","duration":1,"language":"en"}),
        &["text", "duration", "language"],
        &["segments", "words", "usage", "task"],
    );
    fields::<a::TranscriptionDiarized>(
        json!({"text":"x","duration":1,"segments":[],"task":"transcribe"}),
        &["text", "duration", "segments", "task"],
        &["usage"],
    );
    fields::<a::TranscriptionSegment>(
        segment(),
        &[
            "id",
            "seek",
            "start",
            "end",
            "text",
            "tokens",
            "temperature",
            "avg_logprob",
            "compression_ratio",
            "no_speech_prob",
        ],
        &[],
    );
    fields::<a::TranscriptionWord>(
        json!({"word":"x","start":0,"end":1}),
        &["word", "start", "end"],
        &[],
    );
    fields::<a::TranscriptionLogprob>(json!({}), &[], &["token", "bytes", "logprob"]);
    fields::<a::TranslationJson>(json!({"text":"x"}), &["text"], &[]);
    fields::<a::TranslationVerbose>(
        json!({"text":"x","duration":1,"language":"en"}),
        &["text", "duration", "language"],
        &["segments"],
    );
    fields::<i::ImagesResponse>(
        json!({"created":1}),
        &["created"],
        &[
            "data",
            "background",
            "quality",
            "size",
            "output_format",
            "usage",
        ],
    );
    fields::<i::ImageUsage>(
        image_usage(),
        &[
            "input_tokens",
            "input_tokens_details",
            "output_tokens",
            "total_tokens",
        ],
        &["output_tokens_details"],
    );
    fields::<i::ImageTokenDetails>(
        json!({"image_tokens":1,"text_tokens":1}),
        &["image_tokens", "text_tokens"],
        &[],
    );
    fields::<i::GeneratedImage>(json!({}), &[], &["b64_json", "url", "revised_prompt"]);
}
