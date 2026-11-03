use super::*;
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GeminiRealtimeOptions {
    #[cfg_attr(feature = "ts", ts(as = "Option<Value>"))]
    pub generation_config: Option<l::LiveGenerationConfig>,
    #[cfg_attr(feature = "ts", ts(as = "Option<Value>"))]
    pub system_instruction: Option<l::LiveContent>,
    #[cfg_attr(feature = "ts", ts(as = "Option<Value>"))]
    pub tools: Option<Vec<g::Tool>>,
    #[cfg_attr(feature = "ts", ts(as = "Option<Value>"))]
    pub realtime_input_config: Option<l::RealtimeInputConfig>,
    #[cfg_attr(feature = "ts", ts(as = "Option<Value>"))]
    pub session_resumption: Option<l::SessionResumptionConfig>,
    #[cfg_attr(feature = "ts", ts(as = "Option<Value>"))]
    pub context_window_compression: Option<l::ContextWindowCompressionConfig>,
    #[cfg_attr(feature = "ts", ts(as = "Option<Value>"))]
    pub input_audio_transcription: Option<g::audio::AudioTranscriptionConfig>,
    #[cfg_attr(feature = "ts", ts(as = "Option<Value>"))]
    pub output_audio_transcription: Option<g::audio::AudioTranscriptionConfig>,
    #[cfg_attr(feature = "ts", ts(as = "Option<Value>"))]
    pub proactivity: Option<l::ProactivityConfig>,
    #[cfg_attr(feature = "ts", ts(as = "Option<Value>"))]
    pub history_config: Option<l::HistoryConfig>,
    #[cfg_attr(feature = "ts", ts(as = "Option<Value>"))]
    pub avatar_config: Option<l::AvatarConfig>,
    #[cfg_attr(feature = "ts", ts(as = "Option<Value>"))]
    pub safety_settings: Option<Vec<g::content::SafetySetting>>,
    #[serde(flatten)]
    pub rest: protocol::Rest,
}
fn pcm(value: &AudioFormat, rate: u32) -> Result<()> {
    match value {
        AudioFormat::Pcm {
            media_type,
            sample_rate,
            channels: 1,
        } if *sample_rate == rate && (media_type == "audio/pcm" || media_type == "audio/l16") => {
            Ok(())
        }
        _ => Err(Error::invalid(format!(
            "Gemini Live needs {rate} Hz mono PCM; no transcoding is performed"
        ))),
    }
}
pub fn render(config: &RealtimeConfig) -> Result<Rendered<l::LiveClientMessage>> {
    pcm(&config.input_audio_format, 16000)?;
    pcm(&config.output_audio_format, 24000)?;
    let mut warnings = Vec::new();
    let options = &config.provider_options.gemini;
    if config.provider_options.openai != openai::OpenaiRealtimeOptions::default() {
        warn(
            &mut warnings,
            "provider_options.openai",
            "OpenAI options are not sent to Gemini Live",
        );
    }
    let model = if config.model.id.starts_with("models/") {
        config.model.id.clone()
    } else {
        format!("models/{}", config.model.id)
    };
    let mut setup = l::BidiGenerateContentSetup::builder(model).build();
    let mut generation = options
        .generation_config
        .clone()
        .unwrap_or_else(|| l::LiveGenerationConfig::builder().build());
    if generation.response_modalities.is_some() {
        override_warning(&mut warnings, "output_modalities");
    } else {
        let has_audio = config.output_modalities.contains(&Modality::Audio);
        let mut modalities = Vec::new();
        for modality in &config.output_modalities {
            match modality {
                Modality::Audio => modalities.push(g::Modality::Audio),
                Modality::Text if !has_audio => modalities.push(g::Modality::Text),
                Modality::Text => warn(
                    &mut warnings,
                    "output_modalities",
                    "Audio includes output transcription; simultaneous text modality omitted",
                ),
                _ => warn(
                    &mut warnings,
                    "output_modalities",
                    "Unsupported Live output modality omitted",
                ),
            }
        }
        modalities.dedup();
        if modalities.is_empty() {
            return Err(Error::invalid("Live needs a text or audio output modality"));
        }
        generation.response_modalities = Some(modalities);
    }
    if let Some(voice) = &config.voice {
        let speech = generation
            .speech_config
            .get_or_insert_with(|| l::LiveSpeechConfig::builder().build());
        if speech.voice_config.is_some() || speech.multi_speaker_voice_config.is_some() {
            override_warning(&mut warnings, "voice");
        } else {
            speech.voice_config = Some(
                l::LiveVoiceConfig::builder()
                    .prebuilt_voice_config(
                        l::LivePrebuiltVoiceConfig::builder()
                            .voice_name(voice.clone())
                            .build(),
                    )
                    .build(),
            );
        }
    }
    setup.generation_config = Some(generation);
    let mut instructions = config
        .instructions
        .as_ref()
        .map(|text| l::LivePart::builder().text(text.clone()).build())
        .into_iter()
        .collect::<Vec<_>>();
    if let Some(Conversation::V1 { messages }) = &config.history {
        for message in messages.iter().filter(|m| m.role == Role::System) {
            for block in message.model_content() {
                if block.cache.is_some() {
                    warn(
                        &mut warnings,
                        "history.cache",
                        "Live does not support cache breakpoints",
                    );
                }
                if !block.provider_metadata.is_empty() {
                    warn(
                        &mut warnings,
                        "history.provider_metadata",
                        "Prior-session system metadata is not sent to Live",
                    );
                }
                match &block.content {
                    Content::Text(text) => {
                        instructions.push(l::LivePart::builder().text(text.text.clone()).build())
                    }
                    _ => return Err(Error::invalid("Live system history must contain text")),
                }
            }
        }
    }
    if !instructions.is_empty() {
        setup.system_instruction = Some(l::LiveContent::builder().parts(instructions).build());
    }
    if config.tool_choice.is_some() {
        warn(
            &mut warnings,
            "tool_choice",
            "Live setup has no function-calling choice field; tools remain under model control",
        );
    }
    setup.tools = Some(vec![
        g::Tool::builder()
            .function_declarations(
                config
                    .tools
                    .iter()
                    .map(|t| {
                        g::FunctionDeclaration::builder(t.name.clone(), t.description.clone())
                            .parameters_json_schema(t.input_schema.clone())
                            .build()
                    })
                    .collect::<Vec<_>>(),
            )
            .build(),
    ]);
    let disabled = match config.turn_detection {
        TurnDetection::Manual => true,
        TurnDetection::ServerVad => false,
        _ => {
            warn(
                &mut warnings,
                "turn_detection",
                "Live uses automatic activity detection for this turn mode",
            );
            false
        }
    };
    setup.realtime_input_config = Some(
        l::RealtimeInputConfig::builder()
            .automatic_activity_detection(
                l::AutomaticActivityDetection::builder()
                    .disabled(disabled)
                    .build(),
            )
            .build(),
    );
    if let Some(transcription) = &config.input_transcription {
        if transcription.model.is_some() {
            warn(
                &mut warnings,
                "input_transcription.model",
                "Live does not select a separate transcription model",
            );
        }
        if transcription.prompt.is_some() {
            warn(
                &mut warnings,
                "input_transcription.prompt",
                "Live input transcription has no prompt field",
            );
        }
        let mut native = g::audio::AudioTranscriptionConfig::builder().build();
        if let Some(language) = &transcription.language {
            native.language_hints = Some(
                g::audio::LanguageHints::builder()
                    .language_codes(vec![language.clone()])
                    .build(),
            );
        }
        setup.input_audio_transcription = Some(native);
    }
    // Transcripts are needed for persistence because raw audio is never archived.
    setup.output_audio_transcription = Some(g::audio::AudioTranscriptionConfig::builder().build());
    macro_rules! native {
        ($($field:ident),+ $(,)?) => { $(
            if options.$field.is_some() {
                if setup.$field.is_some() { override_warning(&mut warnings, stringify!($field)); }
                setup.$field = options.$field.clone();
            }
        )+ };
    }
    native!(
        system_instruction,
        tools,
        realtime_input_config,
        session_resumption,
        context_window_compression,
        input_audio_transcription,
        output_audio_transcription,
        proactivity,
        history_config,
        avatar_config,
        safety_settings
    );
    setup.rest = options.rest.clone();
    Ok(Rendered {
        request: l::LiveClientMessage::builder().setup(setup).build(),
        warnings,
    })
}
