use super::support::*;
use protocol::{gemini::live as l, openai::realtime as o};

#[test]
fn native_configuration_overrides_and_vendor_warnings_use_typed_wire() {
    let mut c = config(RealtimeDialect::Openai);
    c.voice = Some("portable".into());
    c.instructions = Some("portable instructions".into());
    c.turn_detection = TurnDetection::SemanticVad;
    c.input_transcription = Some(RealtimeInputTranscription {
        model: Some("gpt-transcribe".into()),
        language: Some("en".into()),
        prompt: Some("names".into()),
    });
    c.tools = vec![Tool {
        name: "weather".into(),
        description: "forecast".into(),
        input_schema: json!({"type":"object"}),
    }];
    c.tool_choice = Some(ToolChoice::Tool {
        name: "weather".into(),
    });
    let mut native =
        o::RealtimeSessionCreateRequest::builder(o::RealtimeSessionCreateRequestType::Realtime)
            .instructions(Some("native".into()))
            .build();
    native.audio = Some(Some(
        o::RealtimeAudioConfig::builder()
            .output(Some(
                o::RealtimeAudioConfigOutput::builder()
                    .voice(Some(o::RealtimeAudioConfigOutputVoice::Text(
                        "native-voice".into(),
                    )))
                    .build(),
            ))
            .build(),
    ));
    c.provider_options.openai.session = Some(native);
    c.provider_options
        .openai
        .rest
        .insert("future_setting".into(), json!(3));
    c.provider_options.gemini.session_resumption = Some(
        l::SessionResumptionConfig::builder()
            .handle("other-vendor")
            .build(),
    );
    let rendered = realtime::openai::render(&c).unwrap();
    let value = serde_json::to_value(rendered.request).unwrap();
    let s = &value["session"];
    assert_eq!(s["instructions"], "native");
    assert_eq!(s["audio"]["output"]["voice"], "native-voice");
    assert_eq!(s["audio"]["input"]["format"]["type"], "audio/pcm");
    assert_eq!(s["audio"]["input"]["format"]["rate"], 24000);
    assert_eq!(
        s["audio"]["input"]["turn_detection"]["type"],
        "semantic_vad"
    );
    assert_eq!(
        s["audio"]["input"]["transcription"]["model"],
        "gpt-transcribe"
    );
    assert_eq!(s["tool_choice"]["name"], "weather");
    assert_eq!(s["tools"][0]["name"], "weather");
    assert_eq!(s["future_setting"], 3);
    assert!(rendered.warnings.iter().any(|w| w.field == "instructions"));
    assert!(rendered.warnings.iter().any(|w| w.field == "voice"));
    assert!(
        rendered
            .warnings
            .iter()
            .any(|w| w.field == "provider_options.gemini")
    );
    assert!(!value.to_string().contains("other-vendor"));
}
#[test]
fn realtime_audio_constraints_and_native_g711() {
    let mut c = config(RealtimeDialect::Openai);
    for format in [
        AudioFormat::pcm(16000, 1),
        AudioFormat::pcm(24000, 2),
        AudioFormat::Encoded {
            media_type: "audio/wav".into(),
        },
    ] {
        c.input_audio_format = format;
        assert_eq!(
            realtime::openai::render(&c).unwrap_err().kind,
            ErrorKind::InvalidData
        );
    }
    for (mime, expected) in [("audio/pcmu", "audio/pcmu"), ("audio/pcma", "audio/pcma")] {
        c.input_audio_format = AudioFormat::Encoded {
            media_type: mime.into(),
        };
        let v = serde_json::to_value(realtime::openai::render(&c).unwrap().request).unwrap();
        assert_eq!(v["session"]["audio"]["input"]["format"]["type"], expected);
    }
    let mut g = config(RealtimeDialect::Gemini);
    g.input_audio_format = AudioFormat::pcm(24000, 1);
    assert!(realtime::gemini::render(&g).is_err());
    g.input_audio_format = AudioFormat::pcm(16000, 1);
    g.output_audio_format = AudioFormat::pcm(16000, 1);
    assert!(realtime::gemini::render(&g).is_err());
}
#[test]
fn gemini_native_setup_controls_and_diagnostics() {
    let mut c = config(RealtimeDialect::Gemini);
    c.instructions = Some("instructions".into());
    c.voice = Some("Puck".into());
    c.turn_detection = TurnDetection::SemanticVad;
    c.tool_choice = Some(ToolChoice::Required);
    c.provider_options.gemini.generation_config = Some(
        l::LiveGenerationConfig::builder()
            .response_modalities(vec![protocol::gemini::Modality::Audio])
            .max_output_tokens(16)
            .build(),
    );
    c.provider_options.gemini.session_resumption = Some(
        l::SessionResumptionConfig::builder()
            .handle("resume-here")
            .build(),
    );
    let rendered = realtime::gemini::render(&c).unwrap();
    let value = serde_json::to_value(rendered.request).unwrap();
    let setup = &value["setup"];
    assert_eq!(setup["generationConfig"]["maxOutputTokens"], 16);
    assert_eq!(
        setup["generationConfig"]["speechConfig"]["voiceConfig"]["prebuiltVoiceConfig"]["voiceName"],
        "Puck"
    );
    assert_eq!(
        setup["systemInstruction"]["parts"][0]["text"],
        "instructions"
    );
    assert_eq!(setup["sessionResumption"]["handle"], "resume-here");
    assert!(rendered.warnings.iter().any(|w| w.field == "tool_choice"));
    assert!(
        rendered
            .warnings
            .iter()
            .any(|w| w.field == "turn_detection")
    );
    assert!(
        rendered
            .warnings
            .iter()
            .any(|w| w.field == "output_modalities")
    );
}
#[test]
fn seeded_history_is_sent_after_ready_without_requesting_a_response() {
    futures_executor::block_on(async {
        let client = Client::new(Script::new(vec![ready()]));
        let mut c = config(RealtimeDialect::Openai);
        c.history = Some(Conversation::new(vec![Message {
            role: Role::User,
            content: vec![Block::text("history")],
        }]));
        let mut session = client.connect_realtime(&c).await.unwrap();
        assert_eq!(client.backend.values().len(), 1);
        assert!(matches!(
            session.events.next().await.unwrap().kind,
            RealtimeEventKind::Ready
        ));
        let sent = client.backend.values();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1]["item"]["content"][0]["text"], "history");
        assert_eq!(sent[1]["type"], "conversation.item.create");
    });
}
