use gproxy_protocol::gemini::live::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
fn round<T: Serialize + DeserializeOwned>(v: Value) -> T {
    let p: T = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(serde_json::to_value(&p).unwrap(), v);
    p
}
fn setup() -> Value {
    json!({"model":"models/gemini-live","generationConfig":{"responseModalities":["AUDIO"],"temperature":0.5,"maxOutputTokens":128,"mediaResolution":"MEDIA_RESOLUTION_LOW","enableAffectiveDialog":true,"translationConfig":{"echoTargetLanguage":false,"targetLanguageCode":"es"},"audioTranscriptionConfig":{"languageCodes":["en"]},"speechConfig":{"languageCode":"en-US","voiceConfig":{"replicatedVoiceConfig":{"mimeType":"audio/wav","voiceSampleAudio":"AA==","consentAudio":"AQ==","voiceConsentSignature":{"signature":"sig"}}}}},"systemInstruction":{"parts":[{"text":"help"}]},"tools":[{"functionDeclarations":[{"name":"lookup","description":"lookup"}]}],"realtimeInputConfig":{"automaticActivityDetection":{"disabled":false,"startOfSpeechSensitivity":"START_SENSITIVITY_HIGH","prefixPaddingMs":0,"endOfSpeechSensitivity":"END_SENSITIVITY_LOW","silenceDurationMs":500},"activityHandling":"START_OF_ACTIVITY_INTERRUPTS","turnCoverage":"TURN_INCLUDES_AUDIO_ACTIVITY_AND_ALL_VIDEO"},"sessionResumption":{"handle":"handle"},"contextWindowCompression":{"triggerTokens":"1000","slidingWindow":{"targetTokens":500}},"inputAudioTranscription":{"languageCodes":["en"],"languageAuto":{},"languageHints":{"languageCodes":["en"]},"customVocabulary":["Gemini"],"adaptationPhrases":["gproxy"],"wordTimestamp":true,"diarization":true,"mode":"VERBATIM"},"outputAudioTranscription":{"mode":"SMART"},"proactivity":{"proactiveAudio":true},"historyConfig":{"initialHistoryInClientContent":true},"avatarConfig":{"avatarName":"a","customizedAvatar":{"imageMimeType":"image/jpeg","imageData":"AA=="},"audioBitrateBps":128000,"videoBitrateBps":1000000},"safetySettings":[{"category":"HARM_CATEGORY_HATE_SPEECH","threshold":"BLOCK_NONE"}]})
}
#[test]
fn setup_all_current_developer_fields_and_nested_audio_are_typed() {
    let p = round::<LiveClientMessage>(json!({"setup":setup()}));
    assert!(p.rest.is_empty());
    let s = p.setup.unwrap();
    assert!(s.rest.is_empty());
    let g = s.generation_config.unwrap();
    assert!(g.rest.is_empty());
    assert!(g.translation_config.unwrap().rest.is_empty());
    assert!(g.audio_transcription_config.unwrap().rest.is_empty());
    let speech = g.speech_config.unwrap();
    assert!(speech.rest.is_empty());
    let v = speech.voice_config.unwrap();
    assert!(v.rest.is_empty());
    let r = v.replicated_voice_config.unwrap();
    assert!(r.rest.is_empty());
    assert!(r.voice_consent_signature.unwrap().rest.is_empty());
    let rt = s.realtime_input_config.unwrap();
    assert!(rt.rest.is_empty());
    assert!(rt.automatic_activity_detection.unwrap().rest.is_empty());
    assert!(s.session_resumption.unwrap().rest.is_empty());
    let c = s.context_window_compression.unwrap();
    assert!(c.rest.is_empty());
    assert!(c.sliding_window.unwrap().rest.is_empty());
    assert!(s.proactivity.unwrap().rest.is_empty());
    assert!(s.history_config.unwrap().rest.is_empty());
    let avatar = s.avatar_config.unwrap();
    assert!(avatar.rest.is_empty());
    assert!(avatar.customized_avatar.unwrap().rest.is_empty());
    let t = s.input_audio_transcription.unwrap();
    assert!(t.rest.is_empty());
    assert!(t.language_auto.unwrap().rest.is_empty());
    assert!(t.language_hints.unwrap().rest.is_empty());
    assert!(s.output_audio_transcription.unwrap().rest.is_empty());
    assert!(serde_json::from_value::<BidiGenerateContentSetup>(json!({})).is_err());
}
#[test]
fn client_content_realtime_and_tool_responses_keep_sibling_keys() {
    let wire = json!({"clientContent":{"turns":[{"role":"user","parts":[{"text":"hello"}]}],"turnComplete":false},"realtimeInput":{"mediaChunks":[{"mimeType":"audio/pcm","data":"AA=="}],"audio":{"mimeType":"audio/pcm","data":"AQ=="},"audioStreamEnd":false,"video":{"mimeType":"image/jpeg","data":"Ag=="},"text":"hi","activityStart":{},"activityEnd":{},"mediaResolution":"MEDIA_RESOLUTION_LOW"},"toolResponse":{"functionResponses":[{"id":"call","name":"lookup","response":{"result":"ok"}}]}});
    let p = round::<LiveClientMessage>(wire);
    assert!(p.rest.is_empty());
    assert!(p.client_content.unwrap().rest.is_empty());
    let rt = p.realtime_input.unwrap();
    assert!(rt.rest.is_empty());
    assert!(rt.audio.unwrap().rest.is_empty());
    assert!(rt.video.unwrap().rest.is_empty());
    assert!(rt.activity_start.unwrap().rest.is_empty());
    assert!(rt.activity_end.unwrap().rest.is_empty());
    assert!(rt.media_chunks.unwrap()[0].rest.is_empty());
    assert!(p.tool_response.unwrap().rest.is_empty());
}
fn transcription() -> Value {
    json!({"text":"hello","finished":true,"languageCode":"en","speakerLabel":"A","words":[{"word":"hello","startOffset":"0s","endOffset":"0.5s"}]})
}
#[test]
fn all_server_siblings_usage_and_new_sdk_fields_are_typed() {
    let detail = json!({"modality":"AUDIO","tokenCount":1});
    let wire = json!({"setupComplete":{"sessionId":"session","voiceConsentSignature":{"signature":"s"}},"serverContent":{"modelTurn":{"role":"model","parts":[{"text":"hi"}]},"generationComplete":true,"turnComplete":true,"interrupted":false,"groundingMetadata":{"webSearchQueries":["x"],"searchEntryPoint":{"renderedContent":"html"}},"inputTranscription":transcription(),"outputTranscription":transcription(),"interimInputTranscription":transcription(),"urlContextMetadata":{"urlMetadata":[{"retrievedUrl":"https://x","urlRetrievalStatus":"URL_RETRIEVAL_STATUS_SUCCESS"}]},"waitingForInput":false,"turnCompleteReason":"NEED_MORE_INPUT","interactionStatus":"IDLE","speechState":0},"toolCall":{"functionCalls":[{"id":"call","name":"lookup","args":{"x":1}}]},"toolCallCancellation":{"ids":["call"]},"goAway":{"timeLeft":"30s"},"sessionResumptionUpdate":{"newHandle":"new","resumable":false,"lastConsumedClientMessageIndex":"12"},"usageMetadata":{"promptTokenCount":1,"cachedContentTokenCount":0,"responseTokenCount":2,"toolUsePromptTokenCount":0,"thoughtsTokenCount":1,"totalTokenCount":4,"promptTokensDetails":[detail.clone()],"cacheTokensDetails":[detail.clone()],"responseTokensDetails":[detail.clone()],"toolUsePromptTokensDetails":[detail],"serviceTier":"standard"},"voiceActivityDetectionSignal":{"vadSignalType":"VAD_SIGNAL_TYPE_SOS"},"voiceActivity":{"type":"ACTIVITY_END","audioOffset":"1s"}});
    let p = round::<LiveServerMessage>(wire);
    assert!(p.rest.is_empty());
    let setup = p.setup_complete.unwrap();
    assert!(setup.rest.is_empty());
    assert!(setup.voice_consent_signature.unwrap().rest.is_empty());
    let content = p.server_content.unwrap();
    assert!(content.rest.is_empty());
    assert!(content.grounding_metadata.unwrap().rest.is_empty());
    assert!(content.url_context_metadata.unwrap().rest.is_empty());
    for t in [
        content.input_transcription.unwrap(),
        content.output_transcription.unwrap(),
        content.interim_input_transcription.unwrap(),
    ] {
        assert!(t.rest.is_empty());
        assert!(t.words.unwrap()[0].rest.is_empty());
    }
    assert!(p.tool_call.unwrap().rest.is_empty());
    assert!(p.tool_call_cancellation.unwrap().rest.is_empty());
    assert!(p.go_away.unwrap().rest.is_empty());
    assert!(p.session_resumption_update.unwrap().rest.is_empty());
    let u = p.usage_metadata.unwrap();
    assert!(u.rest.is_empty());
    assert!(u.prompt_tokens_details.unwrap()[0].rest.is_empty());
    assert!(u.cache_tokens_details.unwrap()[0].rest.is_empty());
    assert!(u.response_tokens_details.unwrap()[0].rest.is_empty());
    assert!(u.tool_use_prompt_tokens_details.unwrap()[0].rest.is_empty());
    assert!(p.voice_activity.unwrap().rest.is_empty());
    assert!(p.voice_activity_detection_signal.unwrap().rest.is_empty());
}
#[test]
fn proto_defaults_aliases_false_zero_empty_and_null_have_distinct_wire_rules() {
    macro_rules! empty{($($ty:ty),*)=>{$(let p: $ty=serde_json::from_value(json!({})).unwrap();assert_eq!(serde_json::to_value(p).unwrap(),json!({}));)*};}
    empty!(
        LiveClientMessage,
        LiveServerMessage,
        BidiGenerateContentClientContent,
        BidiGenerateContentRealtimeInput,
        BidiGenerateContentToolResponse,
        BidiGenerateContentSetupComplete,
        BidiGenerateContentServerContent,
        BidiGenerateContentToolCall,
        BidiGenerateContentToolCallCancellation,
        GoAway,
        SessionResumptionUpdate,
        BidiGenerateContentTranscription,
        LiveBlob,
        RealtimeInputConfig,
        AutomaticActivityDetection,
        SessionResumptionConfig,
        ContextWindowCompressionConfig,
        SlidingWindow,
        AudioTranscriptionConfig,
        LiveUsageMetadata,
        LiveActivityStart,
        LiveActivityEnd,
        LanguageAuto,
        LanguageHints,
        HistoryConfig,
        ProactivityConfig,
        VoiceActivity,
        VoiceActivityDetectionSignal,
        WordInfo,
        LiveGenerationConfig
    );
    let wire = json!({"client_content":{"turn_complete":false,"turns":[]},"realtime_input":{"audio_stream_end":false,"activity_start":null},"setup":null});
    let p: LiveClientMessage = serde_json::from_value(wire).unwrap();
    assert_eq!(
        serde_json::to_value(p).unwrap(),
        json!({"clientContent":{"turnComplete":false,"turns":[]},"realtimeInput":{"audioStreamEnd":false}})
    );
    let p:LiveServerMessage=serde_json::from_value(json!({"server_content":{"generation_complete":false,"input_transcription":{"language_code":"en","finished":false}},"session_resumption_update":{"new_handle":"","resumable":false,"last_consumed_client_message_index":0},"usage_metadata":{"prompt_token_count":0}})).unwrap();
    assert_eq!(
        serde_json::to_value(p).unwrap(),
        json!({"serverContent":{"generationComplete":false,"inputTranscription":{"languageCode":"en","finished":false}},"sessionResumptionUpdate":{"newHandle":"","resumable":false,"lastConsumedClientMessageIndex":0},"usageMetadata":{"promptTokenCount":0}})
    );
    let p:AudioTranscriptionConfig=serde_json::from_value(json!({"language_codes":null,"custom_vocabulary":[],"word_timestamp":false,"diarization":null})).unwrap();
    assert_eq!(
        serde_json::to_value(p).unwrap(),
        json!({"customVocabulary":[],"wordTimestamp":false})
    );
    for value in [json!("1000"), json!(1000)] {
        round::<LiveInt64>(value);
    }
    assert!(serde_json::from_value::<LiveInt64>(json!("bad")).is_err());
}
#[test]
fn source_enums_and_unknown_extensions_do_not_create_new_discriminators() {
    for value in [
        "ACTIVITY_HANDLING_UNSPECIFIED",
        "START_OF_ACTIVITY_INTERRUPTS",
        "NO_INTERRUPTION",
    ] {
        round::<ActivityHandling>(json!(value));
    }
    for value in [
        "TURN_COVERAGE_UNSPECIFIED",
        "TURN_INCLUDES_ONLY_ACTIVITY",
        "TURN_INCLUDES_ALL_INPUT",
        "TURN_INCLUDES_AUDIO_ACTIVITY_AND_ALL_VIDEO",
    ] {
        round::<TurnCoverage>(json!(value));
    }
    for value in [
        "START_SENSITIVITY_UNSPECIFIED",
        "START_SENSITIVITY_HIGH",
        "START_SENSITIVITY_LOW",
    ] {
        round::<StartSensitivity>(json!(value));
    }
    for value in [
        "END_SENSITIVITY_UNSPECIFIED",
        "END_SENSITIVITY_HIGH",
        "END_SENSITIVITY_LOW",
    ] {
        round::<EndSensitivity>(json!(value));
    }
    for value in ["MODE_UNSPECIFIED", "VERBATIM", "SMART"] {
        round::<AudioTranscriptionMode>(json!(value));
    }
    for value in [
        "INTERACTION_STATUS_UNSPECIFIED",
        "IN_PROGRESS",
        "REQUIRES_ACTION",
        "IDLE",
    ] {
        round::<InteractionStatus>(json!(value));
    }
    for value in [
        "VAD_SIGNAL_TYPE_UNSPECIFIED",
        "VAD_SIGNAL_TYPE_SOS",
        "VAD_SIGNAL_TYPE_EOS",
    ] {
        round::<VadSignalType>(json!(value));
    }
    for value in ["TYPE_UNSPECIFIED", "ACTIVITY_START", "ACTIVITY_END"] {
        round::<VoiceActivityType>(json!(value));
    }
    assert!(serde_json::from_value::<AudioTranscriptionMode>(json!("future")).is_err());
    let p = round::<LiveServerMessage>(
        json!({"futureEvent":{"x":1},"usageMetadata":{"totalTokenCount":1}}),
    );
    assert_eq!(p.rest.len(), 1);
    let p = round::<BidiGenerateContentSetupComplete>(json!({"future":true}));
    assert_eq!(p.rest.len(), 1);
    round::<SpeechState>(json!("UNPUBLISHED_STATE"));
    round::<SpeechState>(json!(2));
    // The SDK explicitly excludes these Vertex-only options from mldev conversion.
    let p = round::<SessionResumptionConfig>(json!({"transparent":true}));
    assert!(p.rest.contains_key("transparent"));
}
#[test]
fn typed_builders_preserve_handshake_and_independent_websocket_connection() {
    let setup = BidiGenerateContentSetup::builder("models/live".into())
        .proactivity(ProactivityConfig::builder().proactive_audio(false).build())
        .generation_config(
            LiveGenerationConfig::builder()
                .enable_affective_dialog(true)
                .build(),
        )
        .build();
    let msg = LiveClientMessage::builder().setup(setup).build();
    assert_eq!(
        serde_json::to_value(msg).unwrap(),
        json!({"setup":{"model":"models/live","proactivity":{"proactiveAudio":false},"generationConfig":{"enableAffectiveDialog":true}}})
    );
    let request: LiveRequest = gproxy_protocol::WireRequest {
        method: http::Method::GET,
        path: "/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent"
            .into(),
        query: Some("key=placeholder".into()),
        headers: http::HeaderMap::new(),
        body: (),
    };
    assert_eq!(request.method, http::Method::GET);
    assert!(request.path.starts_with("/ws/"));
    assert!(request.query.is_some());
    assert!(request.headers.is_empty());
    assert_eq!(request.body, ());
    let response: LiveResponse = gproxy_protocol::WireResponse {
        status: http::StatusCode::SWITCHING_PROTOCOLS,
        headers: http::HeaderMap::new(),
        body: (),
    };
    assert_eq!(response.status, http::StatusCode::SWITCHING_PROTOCOLS);
    assert_eq!(response.body, ());
    fn duplex(_: gproxy_protocol::WebSocket) {}
    let _ = duplex;
}

#[test]
fn all_documented_turn_completion_reasons_keep_exact_case() {
    for v in [
        "TURN_COMPLETE_REASON_UNSPECIFIED",
        "MALFORMED_FUNCTION_CALL",
        "RESPONSE_REJECTED",
        "NEED_MORE_INPUT",
        "PROHIBITED_INPUT_CONTENT",
        "IMAGE_PROHIBITED_INPUT_CONTENT",
        "INPUT_TEXT_CONTAIN_PROMINENT_PERSON_PROHIBITED",
        "INPUT_IMAGE_CELEBRITY",
        "INPUT_IMAGE_PHOTO_REALISTIC_CHILD_PROHIBITED",
        "INPUT_TEXT_NCII_PROHIBITED",
        "INPUT_OTHER",
        "INPUT_IP_PROHIBITED",
        "BLOCKLIST",
        "UNSAFE_PROMPT_FOR_IMAGE_GENERATION",
        "GENERATED_IMAGE_SAFETY",
        "GENERATED_CONTENT_SAFETY",
        "GENERATED_AUDIO_SAFETY",
        "GENERATED_VIDEO_SAFETY",
        "GENERATED_CONTENT_PROHIBITED",
        "GENERATED_CONTENT_BLOCKLIST",
        "GENERATED_IMAGE_PROHIBITED",
        "GENERATED_IMAGE_CELEBRITY",
        "GENERATED_IMAGE_PROMINENT_PEOPLE_DETECTED_BY_REWRITER",
        "GENERATED_IMAGE_IDENTIFIABLE_PEOPLE",
        "GENERATED_IMAGE_MINORS",
        "OUTPUT_IMAGE_IP_PROHIBITED",
        "GENERATED_OTHER",
        "MAX_REGENERATION_REACHED",
    ] {
        round::<TurnCompleteReason>(json!(v));
    }
    assert!(serde_json::from_value::<TurnCompleteReason>(json!("need_more_input")).is_err());
}

#[test]
fn every_live_specific_field_recognizes_proto_spelling_and_null_as_unset() {
    fn check<T: Serialize + DeserializeOwned>(base: Value, fields: &[&str]) {
        for field in fields {
            let mut camel = String::new();
            let mut upper = false;
            for c in field.chars() {
                if c == '_' {
                    upper = true;
                } else if upper {
                    camel.push(c.to_ascii_uppercase());
                    upper = false;
                } else {
                    camel.push(c);
                }
            }
            for key in [field.to_string(), camel] {
                let mut wire = base.clone();
                wire[&key] = Value::Null;
                let p: T = serde_json::from_value(wire).unwrap();
                assert_eq!(
                    serde_json::to_value(p).unwrap(),
                    base,
                    "{} {key}",
                    std::any::type_name::<T>()
                );
            }
        }
    }
    check::<LiveClientMessage>(
        json!({}),
        &["setup", "client_content", "realtime_input", "tool_response"],
    );
    check::<LiveServerMessage>(
        json!({}),
        &[
            "setup_complete",
            "server_content",
            "tool_call",
            "tool_call_cancellation",
            "usage_metadata",
            "go_away",
            "session_resumption_update",
            "voice_activity_detection_signal",
            "voice_activity",
        ],
    );
    check::<BidiGenerateContentSetup>(
        json!({"model":"m"}),
        &[
            "generation_config",
            "system_instruction",
            "tools",
            "realtime_input_config",
            "session_resumption",
            "context_window_compression",
            "input_audio_transcription",
            "output_audio_transcription",
            "proactivity",
            "history_config",
            "avatar_config",
            "safety_settings",
        ],
    );
    check::<BidiGenerateContentClientContent>(json!({}), &["turns", "turn_complete"]);
    check::<BidiGenerateContentRealtimeInput>(
        json!({}),
        &[
            "media_chunks",
            "audio",
            "audio_stream_end",
            "video",
            "text",
            "activity_start",
            "activity_end",
            "media_resolution",
        ],
    );
    check::<BidiGenerateContentToolResponse>(json!({}), &["function_responses"]);
    check::<BidiGenerateContentSetupComplete>(
        json!({}),
        &["session_id", "voice_consent_signature"],
    );
    check::<BidiGenerateContentServerContent>(
        json!({}),
        &[
            "model_turn",
            "generation_complete",
            "turn_complete",
            "interrupted",
            "grounding_metadata",
            "input_transcription",
            "output_transcription",
            "interim_input_transcription",
            "url_context_metadata",
            "waiting_for_input",
            "turn_complete_reason",
            "interaction_status",
            "speech_state",
        ],
    );
    check::<BidiGenerateContentToolCall>(json!({}), &["function_calls"]);
    check::<BidiGenerateContentToolCallCancellation>(json!({}), &["ids"]);
    check::<GoAway>(json!({}), &["time_left"]);
    check::<SessionResumptionUpdate>(
        json!({}),
        &[
            "new_handle",
            "resumable",
            "last_consumed_client_message_index",
        ],
    );
    check::<BidiGenerateContentTranscription>(
        json!({}),
        &[
            "text",
            "finished",
            "language_code",
            "speaker_label",
            "words",
        ],
    );
    check::<WordInfo>(json!({}), &["word", "start_offset", "end_offset"]);
    check::<LiveUsageMetadata>(
        json!({}),
        &[
            "prompt_token_count",
            "cached_content_token_count",
            "response_token_count",
            "tool_use_prompt_token_count",
            "thoughts_token_count",
            "total_token_count",
            "prompt_tokens_details",
            "cache_tokens_details",
            "response_tokens_details",
            "tool_use_prompt_tokens_details",
            "service_tier",
        ],
    );
    check::<VoiceActivityDetectionSignal>(json!({}), &["vad_signal_type"]);
    check::<VoiceActivity>(json!({}), &["type", "audio_offset"]);
    check::<RealtimeInputConfig>(
        json!({}),
        &[
            "automatic_activity_detection",
            "activity_handling",
            "turn_coverage",
        ],
    );
    check::<AutomaticActivityDetection>(
        json!({}),
        &[
            "disabled",
            "start_of_speech_sensitivity",
            "end_of_speech_sensitivity",
            "prefix_padding_ms",
            "silence_duration_ms",
        ],
    );
    check::<SessionResumptionConfig>(json!({}), &["handle"]);
    check::<ContextWindowCompressionConfig>(json!({}), &["trigger_tokens", "sliding_window"]);
    check::<SlidingWindow>(json!({}), &["target_tokens"]);
    check::<AudioTranscriptionConfig>(
        json!({}),
        &[
            "language_codes",
            "language_auto",
            "language_hints",
            "custom_vocabulary",
            "adaptation_phrases",
            "word_timestamp",
            "diarization",
            "mode",
        ],
    );
    check::<LanguageHints>(json!({}), &["language_codes"]);
    check::<ProactivityConfig>(json!({}), &["proactive_audio"]);
    check::<HistoryConfig>(json!({}), &["initial_history_in_client_content"]);
    check::<AvatarConfig>(
        json!({}),
        &[
            "avatar_name",
            "customized_avatar",
            "audio_bitrate_bps",
            "video_bitrate_bps",
        ],
    );
    check::<CustomizedAvatar>(json!({}), &["image_mime_type", "image_data"]);
    check::<LiveSpeechConfig>(
        json!({}),
        &[
            "voice_config",
            "multi_speaker_voice_config",
            "language_code",
        ],
    );
    check::<LiveVoiceConfig>(
        json!({}),
        &["replicated_voice_config", "prebuilt_voice_config"],
    );
    check::<LivePrebuiltVoiceConfig>(json!({}), &["voice_name"]);
    check::<LiveMultiSpeakerVoiceConfig>(json!({}), &["speaker_voice_configs"]);
    check::<LiveSpeakerVoiceConfig>(json!({}), &["speaker", "voice_config"]);
    check::<ReplicatedVoiceConfig>(
        json!({}),
        &[
            "mime_type",
            "voice_sample_audio",
            "consent_audio",
            "voice_consent_signature",
        ],
    );
    check::<VoiceConsentSignature>(json!({}), &["signature"]);
    check::<TranslationConfig>(json!({}), &["echo_target_language", "target_language_code"]);
    check::<LiveGenerationConfig>(
        json!({}),
        &[
            "enable_affective_dialog",
            "translation_config",
            "audio_transcription_config",
            "speech_config",
            "media_resolution",
            "response_modalities",
            "candidate_count",
            "max_output_tokens",
            "top_p",
            "top_k",
            "presence_penalty",
            "frequency_penalty",
            "thinking_config",
        ],
    );
    check::<LivePart>(json!({}), &["audio_transcription", "media_processing"]);
}
#[test]
fn live_parts_and_prebuilt_multispeaker_voices_use_current_developer_shapes() {
    let content = json!({"modelTurn":{"parts":[{"inlineData":{"mimeType":"audio/pcm","data":"AA=="},"audioTranscription":{"text":"hello","languageCode":"en"},"mediaProcessing":"STATIC"}]}});
    let p = round::<BidiGenerateContentServerContent>(content);
    let p = &p.model_turn.as_ref().unwrap().parts.as_ref().unwrap()[0];
    assert!(p.rest.is_empty());
    assert!(p.audio_transcription.as_ref().unwrap().rest.is_empty());
    assert!(p.inline_data.as_ref().unwrap().rest.is_empty());
    let voice = json!({"multiSpeakerVoiceConfig":{"speakerVoiceConfigs":[{"speaker":"A","voiceConfig":{"prebuiltVoiceConfig":{"voiceName":"Kore"}}}]}});
    let p = round::<LiveSpeechConfig>(voice);
    assert!(p.rest.is_empty());
    let p = p.multi_speaker_voice_config.unwrap();
    assert!(p.rest.is_empty());
    let p = &p.speaker_voice_configs.as_ref().unwrap()[0];
    assert!(p.rest.is_empty());
    assert!(p.voice_config.as_ref().unwrap().rest.is_empty());
    assert!(
        p.voice_config
            .as_ref()
            .unwrap()
            .prebuilt_voice_config
            .as_ref()
            .unwrap()
            .rest
            .is_empty()
    );
    for name in ["MEDIA_PROCESSING_UNSPECIFIED", "STATIC", "AGENTIC"] {
        round::<LiveMediaProcessing>(json!(name));
    }
}
