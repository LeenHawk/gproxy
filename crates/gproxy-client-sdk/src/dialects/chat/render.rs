use super::*;

pub fn render(
    request: &GenerateRequest,
    streaming: bool,
) -> Result<Rendered<q::GenerateContentRequestBody>> {
    render_at(request, streaming, media::clock()?)
}

pub fn render_at(
    request: &GenerateRequest,
    streaming: bool,
    now: u64,
) -> Result<Rendered<q::GenerateContentRequestBody>> {
    crate::assets::resolve::validate_request(request)?;
    let opts = &request.provider_options.openai.chat;
    let mut body =
        q::GenerateContentRequestBody::builder(Vec::new(), request.model.id.clone()).build();
    body.audio = opts.audio.clone();
    body.reasoning_effort = opts.reasoning_effort;
    body.response_format = opts.response_format.clone();
    body.tools = opts.tools.clone();
    body.parallel_tool_calls = opts.parallel_tool_calls;
    body.seed = opts.seed;
    body.store = opts.store;
    body.prompt_cache_key = opts.prompt_cache_key.clone();
    body.prompt_cache_options = opts.prompt_cache_options.clone();
    body.prompt_cache_retention = opts.prompt_cache_retention;
    body.service_tier = opts.service_tier;
    body.web_search_options = opts.web_search_options.clone();
    body.rest = request.provider_options.openai.rest.clone();
    body.rest.extend(opts.rest.clone());
    body.model = request.model.id.clone();
    body.stream = Some(Some(streaming));
    if streaming {
        let mut stream_options = opts
            .stream_options
            .clone()
            .flatten()
            .unwrap_or_else(|| q::StreamOptions::builder().build());
        stream_options.include_usage.get_or_insert(true);
        body.stream_options = Some(Some(stream_options));
    }
    let mut messages = Vec::new();
    for message in &request.messages {
        if message.is_record_only() || matches!(&message.role, Role::Unknown(_)) {
            continue;
        }
        if message.role == Role::Assistant {
            render_assistant(message, now, &mut messages)?;
        } else if message.role == Role::Tool {
            render_tool(message, &mut messages)?;
        } else {
            render_input(message, &mut messages)?;
        }
    }
    body.messages = messages;
    if let Some(max) = request.settings.max_output_tokens {
        body.max_completion_tokens = Some(Some(max.into()));
    }
    if let Some(value) = request.settings.temperature {
        body.temperature = Some(Some(value));
    }
    if let Some(value) = request.settings.top_p {
        body.top_p = Some(Some(value));
    }
    if !request.settings.stop_sequences.is_empty() {
        body.stop = Some(Some(q::Stop::Sequences(
            request.settings.stop_sequences.clone(),
        )));
    }
    if !request.settings.output_modalities.is_empty() {
        let modalities = request
            .settings
            .output_modalities
            .iter()
            .filter_map(|m| match m {
                Modality::Text => Some(Ok(q::Modality::Text)),
                Modality::Audio => Some(Ok(q::Modality::Audio)),
                Modality::Image => Some(Err(Error::invalid("Chat does not support image output"))),
                Modality::Unknown(_) => None,
            })
            .collect::<Result<Vec<_>>>()?;
        if !modalities.is_empty() {
            body.modalities = Some(Some(modalities));
        }
    }
    if let Some(speech) = &request.settings.speech
        && opts.audio.as_ref().and_then(Option::as_ref).is_none()
    {
        let voice = q::AudioVoice::String(speech.voice.clone());
        let format = native_format(&speech.format)?;
        body.audio = Some(Some(q::ChatAudioParam::builder(voice, format).build()));
    }
    if !request.tools.is_empty() {
        let tools = request
            .tools
            .iter()
            .map(|tool| {
                let parameters: Rest =
                    serde_json::from_value(tool.input_schema.clone()).map_err(Error::invalid)?;
                let function = t::FunctionDefinition::builder(tool.name.clone())
                    .description(tool.description.clone())
                    .parameters(parameters)
                    .build();
                Ok(t::ChatTool::Function(
                    t::FunctionTool::builder(t::FunctionToolType::Function, function).build(),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        body.tools.get_or_insert_with(Vec::new).extend(tools);
    }
    if let Some(choice) = &request.settings.tool_choice {
        body.tool_choice = match choice {
            ToolChoice::Auto => Some(t::ToolChoice::Mode(t::ToolChoiceMode::Auto)),
            ToolChoice::None => Some(t::ToolChoice::Mode(t::ToolChoiceMode::None)),
            ToolChoice::Required => Some(t::ToolChoice::Mode(t::ToolChoiceMode::Required)),
            ToolChoice::Tool { name } => Some(t::ToolChoice::Function(
                t::NamedFunctionChoice::builder(
                    t::NamedFunctionChoiceType::Function,
                    t::NamedFunction::builder(name.clone()).build(),
                )
                .build(),
            )),
            ToolChoice::Unknown(_) => None,
        };
    }
    if let Some(schema) = &request.settings.output_schema
        && opts.response_format.is_none()
    {
        let schema: Rest = serde_json::from_value(schema.clone()).map_err(Error::invalid)?;
        let mut format = q::JsonSchemaResponseFormat::builder(
            q::JsonSchemaResponseType::JsonSchema,
            q::JsonSchemaFormat::builder("response".into()).build(),
        )
        .build();
        format.json_schema.schema = Some(schema);
        body.response_format = Some(q::ResponseFormat::JsonSchema(format));
    }
    let mut warnings = crate::dialects::shared::warnings::for_request(request, Some(now))?;
    warnings.extend(crate::dialects::shared::reasoning::chat(request, &mut body));
    Ok(Rendered {
        request: body,
        warnings,
    })
}

mod roles;
use roles::*;

fn render_reasoning_details(
    message: &mut c::AssistantMessage,
    text: &str,
    layout: &[ReasoningSegment],
) -> Result<()> {
    use gproxy_protocol::openai::chat::reasoning::ReasoningDetailKind as K;
    let details = message
        .reasoning_details
        .as_mut()
        .and_then(Option::as_mut)
        .ok_or_else(|| Error::invalid("Chat reasoning metadata has no detail records"))?;
    let mut chars = text.chars();
    for (position, segment) in layout.iter().enumerate() {
        let value = if position + 1 == layout.len() {
            chars.by_ref().collect()
        } else {
            chars.by_ref().take(segment.chars).collect()
        };
        let detail = details
            .get_mut(segment.index)
            .ok_or_else(|| Error::invalid("Chat reasoning layout references a missing detail"))?;
        match detail.type_ {
            K::Text => detail.text = Some(Some(value)),
            K::Summary => detail.summary = Some(value),
            K::Encrypted => {
                return Err(Error::invalid(
                    "visible reasoning cannot replace encrypted data",
                ));
            }
        }
    }
    Ok(())
}

fn tool_output(result: &ToolResult) -> Result<c::TextContent> {
    Ok(match &result.output {
        ToolOutput::Unknown(_) => return Err(Error::invalid("unknown persisted tool output")),
        ToolOutput::Json(Value::String(text)) => c::TextContent::Text(text.clone()),
        ToolOutput::Json(value) => c::TextContent::Text(value.to_string()),
        ToolOutput::Blocks(blocks) => {
            let mut parts = Vec::with_capacity(blocks.len());
            for block in blocks {
                let ToolResultContent::Text(text) = &block.content else {
                    return Err(Error::invalid(
                        "Chat tool messages require text output; use Responses for media",
                    ));
                };
                let mut part =
                    c::TextPart::builder(c::TextPartType::Text, text.text.clone()).build();
                part.prompt_cache_breakpoint = media::chat_cache(block)?;
                parts.push(part);
            }
            c::TextContent::Parts(parts)
        }
    })
}

fn user_part(block: &Block) -> Result<c::UserContentPart> {
    Ok(match &block.content {
        Content::Text(text) => {
            let mut value = c::TextPart::builder(c::TextPartType::Text, text.text.clone()).build();
            value.prompt_cache_breakpoint = media::chat_cache(block)?;
            c::UserContentPart::Text(value)
        }
        Content::Image(image) => {
            media::check_file_vendor(&image.source, Vendor::Openai)?;
            let source = &image.source;
            let mut url =
                c::ImageUrl::builder(data_url(source, image.media_type.as_deref())?).build();
            url.detail = media::chat_detail(&image.detail);
            let mut value = c::ImagePart::builder(c::ImagePartType::ImageUrl, url).build();
            value.prompt_cache_breakpoint = media::chat_cache(block)?;
            c::UserContentPart::Image(value)
        }
        Content::Document(document) => {
            let (original, mime) = media::document_bytes(document)?;
            media::check_file_vendor(&original, Vendor::Openai)?;
            let source = original.as_ref();
            chat_file(block, source, mime, document.title.as_deref())?
        }
        Content::File(file) => {
            media::check_file_vendor(&file.source, Vendor::Openai)?;
            let source = &file.source;
            chat_file(
                block,
                source,
                file.media_type.as_deref(),
                file.filename.as_deref(),
            )?
        }
        Content::Audio(audio) => {
            let Asset::Inline { bytes } = &audio.source else {
                return Err(Error::invalid(
                    "Chat input_audio requires inline WAV or MP3 bytes",
                ));
            };
            let format = match &audio.format {
                AudioFormat::Encoded { media_type }
                    if matches!(media_type.as_str(), "audio/wav" | "audio/x-wav") =>
                {
                    c::AudioFormat::Wav
                }
                AudioFormat::Encoded { media_type }
                    if matches!(media_type.as_str(), "audio/mpeg" | "audio/mp3") =>
                {
                    c::AudioFormat::Mp3
                }
                _ => {
                    return Err(Error::invalid(
                        "Chat input audio supports WAV/MP3 only; no transcoding is performed",
                    ));
                }
            };
            let audio = c::InputAudio::builder(
                base64::engine::general_purpose::STANDARD.encode(bytes),
                format,
            )
            .build();
            let mut value =
                c::InputAudioPart::builder(c::InputAudioPartType::InputAudio, audio).build();
            value.prompt_cache_breakpoint = media::chat_cache(block)?;
            c::UserContentPart::InputAudio(value)
        }
        _ => return Err(Error::invalid("this content kind is not a Chat user input")),
    })
}

fn chat_file(
    block: &Block,
    source: &Asset,
    mime: Option<&str>,
    filename: Option<&str>,
) -> Result<c::UserContentPart> {
    let mut file = c::FileRef::builder().build();
    match source {
        Asset::Inline { .. } => {
            file.file_data = Some(data_url(source, mime)?);
            file.filename = filename.map(str::to_owned);
        }
        Asset::ProviderFile {
            vendor: Vendor::Openai,
            id,
        } => file.file_id = Some(id.clone()),
        _ => {
            return Err(Error::invalid(
                "Chat documents require inline bytes; resolve application references before rendering",
            ));
        }
    }
    let mut part = c::FilePart::builder(c::FilePartType::File, file).build();
    part.prompt_cache_breakpoint = media::chat_cache(block)?;
    Ok(c::UserContentPart::File(part))
}
