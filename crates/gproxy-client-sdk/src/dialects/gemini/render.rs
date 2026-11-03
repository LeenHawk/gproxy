use super::*;

pub fn render(request: &GenerateRequest) -> Result<Rendered<g::GenerateContentRequestBody>> {
    crate::assets::resolve::validate_request(request)?;
    let opts = &request.provider_options.gemini;
    let mut body = g::GenerateContentRequestBody::builder(Vec::new()).build();
    body.generation_config = opts.generation_config.clone();
    body.tools = opts.tools.clone();
    body.tool_config = opts.tool_config.clone();
    body.safety_settings = opts.safety_settings.clone();
    body.cached_content = opts.cached_content.clone();
    body.continuation_token = opts.continuation_token.clone();
    body.service_tier = opts.service_tier.clone();
    body.rest = opts.rest.clone();
    render_messages(&request.messages, &mut body)?;
    render_settings(request, &mut body)?;
    render_tools(request, &mut body);
    let mut warnings = crate::dialects::shared::warnings::for_request(request, None)?;
    warnings.extend(crate::dialects::shared::reasoning::gemini(
        request, &mut body,
    ));
    Ok(Rendered {
        request: body,
        warnings,
    })
}
mod content;
use content::*;

fn render_tool_result(
    mut part: g::Part,
    result: &ToolResult,
    calls: &BTreeMap<&str, &str>,
    native: Option<ServerResponseMetadata>,
    response_meta: Option<ResultMetadata>,
    response_json_text: bool,
    had_metadata: bool,
) -> Result<Option<g::Part>> {
    if result.provider_executed {
        let Some(native) = native else {
            return Ok(None);
        };
        let mut value = g::ToolResponse::builder(native.tool_type)
            .rest(native.rest)
            .build();
        if native.id_present {
            value.id = Some(result.id.clone());
        }
        let ToolOutput::Json(output) = &result.output else {
            return Err(Error::invalid("Gemini server tool output requires JSON"));
        };
        if native.response_present || !output.is_null() {
            value.response = Some(serde_json::from_value(output.clone()).map_err(Error::invalid)?);
        }
        part.tool_response = Some(value);
        return Ok(Some(part));
    }
    let name = tool_name(&result.name, &result.id, calls)?;
    let mut parts = Vec::new();
    let response = match &result.output {
        ToolOutput::Unknown(_) => {
            return Err(Error::invalid("unknown persisted tool output"));
        }
        ToolOutput::Json(Value::Object(object)) => object.clone(),
        ToolOutput::Json(value) => encode_rest(if result.is_error {
            ToolOutputJson::Error { error: value }
        } else {
            ToolOutputJson::Result { result: value }
        })?,
        ToolOutput::Blocks(blocks) => {
            let mut text = Vec::new();
            for (index, block) in blocks.iter().enumerate() {
                if response_json_text && index == 0 {
                    continue;
                }
                match &block.content {
                    ToolResultContent::Unknown(_) => {
                        return Err(Error::invalid("unknown persisted tool content"));
                    }
                    ToolResultContent::Text(value) => text.push(value.text.as_str()),
                    ToolResultContent::Image(value) => parts.push(response_part(
                        block,
                        &value.source,
                        value.media_type.as_deref(),
                        None,
                    )?),
                    ToolResultContent::Document(value) => {
                        let (source, mime) = media::document_bytes(value)?;
                        parts.push(response_part(block, &source, mime, value.title.as_deref())?);
                    }
                }
            }
            #[derive(Serialize)]
            struct Output<'a> {
                result: Vec<&'a str>,
            }
            if response_json_text {
                let Some(ContentBlock {
                    content: ToolResultContent::Text(value),
                    ..
                }) = blocks.first()
                else {
                    return Err(Error::invalid("Gemini response JSON text is missing"));
                };
                serde_json::from_str(&value.text).map_err(Error::invalid)?
            } else {
                encode_rest(Output { result: text })?
            }
        }
    };
    let mut value = g::FunctionResponse::builder(name.to_owned(), response).build();
    let include_id = response_meta.as_ref().is_some_and(|m| m.id_present) || !had_metadata;
    let parts_present = response_meta.as_ref().is_some_and(|m| m.parts_present);
    if let Some(native) = response_meta {
        value.will_continue = native.will_continue;
        value.scheduling = native.scheduling;
        value.rest = native.rest;
    }
    if parts_present || !parts.is_empty() {
        value.parts = Some(parts);
    }
    if include_id && !result.id.is_empty() {
        value.id = Some(result.id.clone());
    }
    part.function_response = Some(value);

    Ok(Some(part))
}

fn render_messages(messages: &[Message], body: &mut g::GenerateContentRequestBody) -> Result<()> {
    let mut contents = Vec::new();
    let mut system = Vec::new();
    let mut calls = BTreeMap::new();
    for message in messages {
        if message.is_record_only() || matches!(&message.role, Role::Unknown(_)) {
            continue;
        }
        let mut parts = Vec::new();
        for block in message.model_content() {
            if let Some(part) = render_block(block, &calls)? {
                parts.push(part);
                parts.extend(trailing_transcripts(block)?);
            }
            if let Content::ToolCall(ToolCall { id, name, .. }) = &block.content {
                calls.insert(id.as_str(), name.as_str());
            }
        }
        if message.role == Role::System {
            system.extend(parts);
        } else {
            contents.push(
                g::Content::builder()
                    .role(if message.role == Role::Assistant {
                        "model".to_owned()
                    } else {
                        "user".to_owned()
                    })
                    .parts(parts)
                    .build(),
            );
        }
    }
    body.contents = contents;
    if !system.is_empty() {
        body.system_instruction = Some(g::Content::builder().parts(system).build());
    }

    Ok(())
}

fn render_settings(
    request: &GenerateRequest,
    body: &mut g::GenerateContentRequestBody,
) -> Result<()> {
    let opts = &request.provider_options.gemini;
    if request.settings != Settings::default() {
        let config = body
            .generation_config
            .get_or_insert_with(|| g::GenerationConfig::builder().build());
        if let Some(value) = request.settings.temperature
            && config.temperature.is_none()
        {
            config.temperature = Some(value);
        }
        if let Some(value) = request.settings.top_p
            && config.top_p.is_none()
        {
            config.top_p = Some(value);
        }
        if let Some(value) = request.settings.max_output_tokens
            && config.max_output_tokens.is_none()
        {
            config.max_output_tokens = Some(value.into());
        }
        if !request.settings.stop_sequences.is_empty() && config.stop_sequences.is_none() {
            config.stop_sequences = Some(request.settings.stop_sequences.clone());
        }
        if let Some(schema) = &request.settings.output_schema
            && config.response_json_schema.is_none()
            && config.response_schema.is_none()
            && config.response_mime_type.is_none()
        {
            config.response_mime_type = Some("application/json".into());
            config.response_json_schema = Some(schema.clone());
        }
    }
    if !request.settings.output_modalities.is_empty()
        && opts
            .generation_config
            .as_ref()
            .and_then(|c| c.response_modalities.as_ref())
            .is_none()
    {
        let modalities = request
            .settings
            .output_modalities
            .iter()
            .filter_map(|m| match m {
                Modality::Text => Some(g::Modality::Text),
                Modality::Audio => Some(g::Modality::Audio),
                Modality::Image => Some(g::Modality::Image),
                Modality::Unknown(_) => None,
            })
            .collect::<Vec<_>>();
        if !modalities.is_empty() {
            body.generation_config
                .get_or_insert_with(|| g::GenerationConfig::builder().build())
                .response_modalities = Some(modalities);
        }
    }
    if let Some(speech) = &request.settings.speech {
        match &speech.format {
            AudioFormat::Pcm {
                sample_rate: 24000,
                channels: 1,
                ..
            } => {}
            _ => {
                return Err(Error::invalid(
                    "Gemini speech output is 24 kHz mono PCM; no transcoding is performed",
                ));
            }
        }
        let config = body
            .generation_config
            .get_or_insert_with(|| g::GenerationConfig::builder().build());
        let speech_config = config
            .speech_config
            .get_or_insert_with(|| g::SpeechConfig::builder().build());
        if speech_config.multi_speaker_voice_config.is_none() {
            let voice = speech_config
                .voice_config
                .get_or_insert_with(|| g::VoiceConfig::builder().build());
            let prebuilt = voice
                .prebuilt_voice_config
                .get_or_insert_with(|| g::PrebuiltVoiceConfig::builder().build());
            if prebuilt.voice_name.is_none() {
                prebuilt.voice_name = Some(speech.voice.clone());
            }
        }
    }

    Ok(())
}

fn render_tools(request: &GenerateRequest, body: &mut g::GenerateContentRequestBody) {
    if !request.tools.is_empty() {
        let declarations: Vec<_> = request
            .tools
            .iter()
            .map(|tool| {
                g::FunctionDeclaration::builder(tool.name.clone(), tool.description.clone())
                    .parameters_json_schema(tool.input_schema.clone())
                    .build()
            })
            .collect();
        body.tools.get_or_insert_with(Vec::new).push(
            g::Tool::builder()
                .function_declarations(declarations)
                .build(),
        );
    }
    if let Some(choice) = &request.settings.tool_choice {
        let mode = match choice {
            ToolChoice::Auto => Some(g::FunctionCallingMode::Auto),
            ToolChoice::None => Some(g::FunctionCallingMode::None),
            ToolChoice::Required | ToolChoice::Tool { .. } => Some(g::FunctionCallingMode::Any),
            ToolChoice::Unknown(_) => None,
        };
        if let Some(mode) = mode {
            let config = body
                .tool_config
                .get_or_insert_with(|| g::ToolConfig::builder().build())
                .function_calling_config
                .get_or_insert_with(|| g::FunctionCallingConfig::builder().build());
            config.mode = Some(mode);
            if let ToolChoice::Tool { name } = choice {
                config.allowed_function_names = Some(vec![name.clone()]);
            }
        }
    }
}
