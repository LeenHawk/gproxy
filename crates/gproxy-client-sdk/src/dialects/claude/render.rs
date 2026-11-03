use super::*;

pub fn render(
    request: &GenerateRequest,
    streaming: bool,
) -> Result<Rendered<r::GenerateContentRequestBody>> {
    crate::assets::resolve::validate_request(request)?;
    if request
        .settings
        .output_modalities
        .iter()
        .any(|m| !matches!(m, Modality::Text | Modality::Unknown(_)))
    {
        return Err(Error::invalid("Claude Messages supports text output only"));
    }
    let opts = &request.provider_options.claude;
    let max = opts
        .max_tokens
        .or(request.settings.max_output_tokens.map(i64::from))
        .ok_or_else(|| {
            Error::invalid("Claude requires max_output_tokens or provider_options.max_tokens")
        })?;
    let mut body =
        r::GenerateContentRequestBody::builder(max, Vec::new(), request.model.id.clone()).build();
    body.thinking = opts.thinking.clone();
    body.cache_control = opts.cache_control.clone();
    body.container = opts.container.clone();
    body.context_management = opts.context_management.clone();
    body.output_config = opts.output_config.clone();
    body.service_tier = opts.service_tier;
    body.speed = opts.speed;
    body.tools = opts.tools.clone();
    body.rest = opts.rest.clone();
    body.model = request.model.id.clone();
    body.max_tokens = max;
    body.stream = Some(streaming);
    let mut messages = Vec::new();
    let mut system = Vec::new();
    for message in &request.messages {
        if message.is_record_only() || matches!(&message.role, Role::Unknown(_)) {
            continue;
        }
        let mut content = Vec::new();
        for block in message.model_content() {
            if let Some(block) = render_block(block)? {
                content.push(block);
            }
        }
        if message.role == Role::System {
            for block in content {
                match block {
                    c::ContentBlock::Text(text) => system.push(text),
                    _ => return Err(Error::invalid("Claude system prompts require text blocks")),
                }
            }
        } else {
            let role = if message.role == Role::Assistant {
                c::Role::Assistant
            } else {
                c::Role::User
            };
            messages.push(c::Message::builder(role, c::MessageContent::Blocks(content)).build());
        }
    }
    body.messages = messages;
    if !system.is_empty() {
        body.system = Some(q::SystemPrompt::Blocks(system));
    }
    let sampling = Sampling {
        temperature: request.settings.temperature,
        top_p: request.settings.top_p,
    };
    body.rest.extend(encode_rest(sampling)?);
    if !request.settings.stop_sequences.is_empty() {
        body.stop_sequences = Some(request.settings.stop_sequences.clone());
    }
    if !request.tools.is_empty() {
        let tools = request
            .tools
            .iter()
            .map(|tool| {
                let schema: t::JsonSchema =
                    serde_json::from_value(tool.input_schema.clone()).map_err(Error::invalid)?;
                Ok(t::ToolUnion::Custom(
                    t::Tool::builder(schema, tool.name.clone())
                        .description(tool.description.clone())
                        .build(),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        body.tools.get_or_insert_with(Vec::new).extend(tools);
    }
    if let Some(choice) = &request.settings.tool_choice {
        body.tool_choice = match choice {
            ToolChoice::Auto => Some(q::ToolChoice::Auto(q::ToolChoiceAuto::builder().build())),
            ToolChoice::None => Some(q::ToolChoice::None(q::ToolChoiceNone::builder().build())),
            ToolChoice::Required => Some(q::ToolChoice::Any(q::ToolChoiceAny::builder().build())),
            ToolChoice::Tool { name } => Some(q::ToolChoice::Tool(
                q::ToolChoiceTool::builder(name.clone()).build(),
            )),
            ToolChoice::Unknown(_) => None,
        };
    }
    if let Some(schema) = &request.settings.output_schema
        && opts
            .output_config
            .as_ref()
            .and_then(|c| c.format.as_ref())
            .is_none()
    {
        let output = body
            .output_config
            .get_or_insert_with(|| q::OutputConfig::builder().build());
        output.format = Some(
            q::JsonOutputFormat::builder(q::JsonOutputFormatType::JsonSchema, schema.clone())
                .build(),
        );
    }
    crate::assets::resolve::check_claude_request(&body)?;
    let mut warnings = crate::dialects::shared::warnings::for_request(request, None)?;
    warnings.extend(crate::dialects::shared::reasoning::claude(
        request, &mut body,
    ));
    Ok(Rendered {
        request: body,
        warnings,
    })
}

#[derive(Serialize)]
pub(super) struct Sampling {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) top_p: Option<f64>,
}
mod blocks;
use blocks::*;

fn render_image(block: &ContentBlock<impl Sized>, image: &Image) -> Result<c::ImageBlock> {
    media::check_file_vendor(&image.source, Vendor::Claude)?;
    let meta = read_native::<ImageMetadata>(block, Vendor::Claude)?.unwrap_or_default();
    let source = match &image.source {
        Asset::Inline { bytes } => {
            let mime = match image.media_type.as_deref() {
                Some("image/png") => c::ImageMediaType::Png,
                Some("image/jpeg") => c::ImageMediaType::Jpeg,
                Some("image/gif") => c::ImageMediaType::Gif,
                Some("image/webp") => c::ImageMediaType::Webp,
                _ => {
                    return Err(Error::invalid(
                        "Claude images require JPEG, PNG, GIF or WebP",
                    ));
                }
            };
            c::ImageSource::Base64(
                c::Base64Source::builder(
                    base64::engine::general_purpose::STANDARD.encode(bytes),
                    mime,
                )
                .rest(meta.source_rest)
                .build(),
            )
        }
        Asset::Url { url } => c::ImageSource::Url(
            c::UrlSource::builder(url.clone())
                .rest(meta.source_rest)
                .build(),
        ),
        Asset::ProviderFile { id, .. } => c::ImageSource::File(
            c::FileSource::builder(id.clone())
                .rest(meta.source_rest)
                .build(),
        ),
        Asset::Unknown(_) => return Err(Error::invalid("unknown persisted media source")),
        Asset::Reference { .. } => {
            return Err(Error::invalid(
                "resolve application references before rendering",
            ));
        }
    };
    let mut value = c::ImageBlock::builder(c::ImageBlockType::Tag, source)
        .rest(meta.rest)
        .build();
    value.cache_control = media::claude_cache(block)?;
    Ok(value)
}

fn render_document(
    block: &ContentBlock<impl Sized>,
    document: &Document,
) -> Result<c::DocumentBlock> {
    let meta = read_native::<DocumentMetadata>(block, Vendor::Claude)?.unwrap_or_default();
    let source = match &document.source {
        DocumentSource::Unknown(_) => {
            return Err(Error::invalid("unknown persisted document source"));
        }
        DocumentSource::Text { text } => c::DocumentSource::Text(
            c::PlainTextSource::builder(text.clone(), c::TextMediaType::Plain)
                .rest(meta.source_rest)
                .build(),
        ),
        DocumentSource::Chunks { chunks } => {
            let content = if meta.chunk_text && chunks.len() == 1 {
                match &chunks[0].content {
                    ChunkContent::Unknown(_) => {
                        return Err(Error::invalid("unknown persisted document chunk"));
                    }
                    ChunkContent::Text(text) => c::DocumentContent::Text(text.text.clone()),
                    ChunkContent::Image(_) => c::DocumentContent::Blocks(
                        chunks.iter().map(render_chunk).collect::<Result<_>>()?,
                    ),
                }
            } else {
                c::DocumentContent::Blocks(chunks.iter().map(render_chunk).collect::<Result<_>>()?)
            };
            c::DocumentSource::Content(
                c::ContentSource::builder(content)
                    .rest(meta.source_rest)
                    .build(),
            )
        }
        DocumentSource::Asset { source } => {
            media::check_file_vendor(source, Vendor::Claude)?;
            match source {
                Asset::Inline { bytes } => match document.media_type.as_deref() {
                    Some("application/pdf") => c::DocumentSource::Base64(
                        c::PdfBase64Source::builder(
                            base64::engine::general_purpose::STANDARD.encode(bytes),
                            c::PdfMediaType::Pdf,
                        )
                        .rest(meta.source_rest)
                        .build(),
                    ),
                    Some("text/plain") => c::DocumentSource::Text(
                        c::PlainTextSource::builder(
                            std::str::from_utf8(bytes).map_err(Error::invalid)?.into(),
                            c::TextMediaType::Plain,
                        )
                        .rest(meta.source_rest)
                        .build(),
                    ),
                    _ => {
                        return Err(Error::invalid(
                            "Claude inline documents require PDF or text/plain",
                        ));
                    }
                },
                Asset::Url { url } => c::DocumentSource::Url(
                    c::UrlSource::builder(url.clone())
                        .rest(meta.source_rest)
                        .build(),
                ),
                Asset::ProviderFile { id, .. } => c::DocumentSource::File(
                    c::FileSource::builder(id.clone())
                        .rest(meta.source_rest)
                        .build(),
                ),
                Asset::Unknown(_) => return Err(Error::invalid("unknown persisted media source")),
                Asset::Reference { .. } => {
                    return Err(Error::invalid(
                        "resolve application references before rendering",
                    ));
                }
            }
        }
    };
    let mut value = c::DocumentBlock::builder(c::DocumentBlockType::Tag, source)
        .rest(meta.rest)
        .build();
    value.title = document.title.clone();
    value.context = document.context.clone();
    value.citations = meta.citations;
    if let Some(enabled) = document.citations {
        value
            .citations
            .get_or_insert_with(|| c::CitationsConfig::builder().build())
            .enabled = Some(enabled);
    }
    value.cache_control = media::claude_cache(block)?;
    Ok(value)
}
