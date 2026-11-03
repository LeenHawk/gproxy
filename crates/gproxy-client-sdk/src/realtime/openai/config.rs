use super::*;
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct OpenaiRealtimeOptions {
    /// A typed native patch, merged into the portable session configuration.
    #[cfg_attr(feature = "ts", ts(as = "Option<Value>"))]
    pub session: Option<o::RealtimeSessionCreateRequest>,
    #[serde(flatten)]
    pub rest: protocol::Rest,
}
pub fn render(config: &RealtimeConfig) -> Result<Rendered<o::SessionUpdateEvent>> {
    let mut warnings = Vec::new();
    if config.provider_options.gemini != gemini::GeminiRealtimeOptions::default() {
        warn(
            &mut warnings,
            "provider_options.gemini",
            "Gemini options are not sent to OpenAI",
        );
    }
    let mut input = o::RealtimeAudioConfigInput::builder()
        .format(Some(audio::format(&config.input_audio_format)?))
        .build();
    input.turn_detection = audio::turn(&config.turn_detection, &mut warnings);
    input.transcription = Some(config.input_transcription.as_ref().map(|t| {
        let mut value = o::AudioTranscription::builder().build();
        value.model = t.model.clone().map(Some);
        value.language = t.language.clone().map(Some);
        value.prompt = t.prompt.clone().map(Some);
        value
    }));
    let mut output = o::RealtimeAudioConfigOutput::builder()
        .format(Some(audio::format(&config.output_audio_format)?))
        .build();
    output.voice = config
        .voice
        .clone()
        .map(|v| Some(o::RealtimeAudioConfigOutputVoice::Text(v)));
    let mut session =
        o::RealtimeSessionCreateRequest::builder(o::RealtimeSessionCreateRequestType::Realtime)
            .build();
    session.instructions = config.instructions.clone().map(Some);
    let has_audio = config.output_modalities.contains(&Modality::Audio);
    let mut modalities = Vec::new();
    for modality in &config.output_modalities {
        match modality {
            Modality::Audio => {
                modalities.push(o::RealtimeSessionCreateRequestOutputModalitiesItem::Audio)
            }
            Modality::Text if !has_audio => {
                modalities.push(o::RealtimeSessionCreateRequestOutputModalitiesItem::Text)
            }
            Modality::Text => warn(
                &mut warnings,
                "output_modalities",
                "Audio includes its transcript; simultaneous text modality omitted",
            ),
            _ => warn(
                &mut warnings,
                "output_modalities",
                "Unsupported Realtime output modality omitted",
            ),
        }
    }
    modalities.dedup();
    if modalities.is_empty() {
        return Err(Error::invalid(
            "Realtime needs a text or audio output modality",
        ));
    }
    session.output_modalities = Some(Some(modalities));
    session.tools = Some(Some(
        config
            .tools
            .iter()
            .map(|tool| {
                o::RealtimeToolsConfigUnion::RealtimeFunctionTool(
                    o::RealtimeFunctionTool::builder()
                        .name(Some(tool.name.clone()))
                        .description(Some(tool.description.clone()))
                        .parameters(Some(tool.input_schema.clone()))
                        .type_(Some(o::RealtimeFunctionToolType::Function))
                        .build(),
                )
            })
            .collect(),
    ));
    session.tool_choice = config.tool_choice.as_ref().and_then(|choice| {
        use o::RealtimeToolChoiceConfig as C;
        Some(Some(match choice {
            ToolChoice::Auto => C::ToolChoiceOptions(o::ToolChoiceOptions::Auto),
            ToolChoice::None => C::ToolChoiceOptions(o::ToolChoiceOptions::None),
            ToolChoice::Required => C::ToolChoiceOptions(o::ToolChoiceOptions::Required),
            ToolChoice::Tool { name } => C::ToolChoiceFunction(
                o::ToolChoiceFunction::builder(name.clone(), o::ToolChoiceFunctionType::Function)
                    .build(),
            ),
            ToolChoice::Unknown(_) => {
                warn(&mut warnings, "tool_choice", "Unknown tool choice omitted");
                return None;
            }
        }))
    });
    if let Some(native) = &config.provider_options.openai.session {
        macro_rules! merge {
            ($target:ident, $source:ident, $($field:ident),+ $(,)?) => { $(
                if $source.$field.is_some() {
                    if $target.$field.is_some() { override_warning(&mut warnings, stringify!($field)); }
                    $target.$field = $source.$field.clone();
                }
            )+ };
        }
        merge!(
            session,
            native,
            instructions,
            include,
            max_output_tokens,
            model,
            output_modalities,
            parallel_tool_calls,
            prompt,
            reasoning,
            tool_choice,
            tools,
            tracing,
            truncation
        );
        if let Some(Some(a)) = &native.audio {
            if let Some(Some(n)) = &a.input {
                merge!(
                    input,
                    n,
                    format,
                    noise_reduction,
                    transcription,
                    turn_detection
                );
                input.rest.extend(n.rest.clone());
            }
            if let Some(Some(n)) = &a.output {
                merge!(output, n, format, voice, speed);
                output.rest.extend(n.rest.clone());
            }
        }
        session.rest.extend(native.rest.clone());
    }
    let mut audio = o::RealtimeAudioConfig::builder()
        .input(Some(input))
        .output(Some(output))
        .build();
    if let Some(native) = &config.provider_options.openai.session {
        match &native.audio {
            Some(None) => {
                override_warning(&mut warnings, "audio");
                session.audio = Some(None);
            }
            Some(Some(n)) => {
                if n.input == Some(None) {
                    override_warning(&mut warnings, "audio.input");
                    audio.input = Some(None);
                }
                if n.output == Some(None) {
                    override_warning(&mut warnings, "audio.output");
                    audio.output = Some(None);
                }
                audio.rest.extend(n.rest.clone());
                session.audio = Some(Some(audio));
            }
            None => session.audio = Some(Some(audio)),
        }
    } else {
        session.audio = Some(Some(audio));
    }
    session
        .rest
        .extend(config.provider_options.openai.rest.clone());
    Ok(Rendered {
        request: update(session),
        warnings,
    })
}
