use super::*;
use gproxy_protocol::claude::{content as c, count_tokens as q, generate_content as r, tools as t};

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum ReasoningMetadata {
    #[serde(rename = "thinking")]
    Thinking {
        signature: String,
        #[serde(flatten)]
        rest: Rest,
    },
    #[serde(rename = "redacted_thinking")]
    Redacted {
        data: String,
        #[serde(flatten)]
        rest: Rest,
    },
}

pub fn render(request: &GenerateRequest, streaming: bool) -> Result<r::GenerateContentRequestBody> {
    if request
        .settings
        .output_modalities
        .iter()
        .any(|m| *m != Modality::Text)
    {
        return Err(Error::invalid("Claude Messages supports text output only"));
    }
    let opts: r::GenerateContentRequestBodyOptions = options(request)?;
    let max = request
        .settings
        .max_output_tokens
        .map(i64::from)
        .or(opts.max_tokens)
        .ok_or_else(|| {
            Error::invalid("Claude requires max_output_tokens or provider_options.max_tokens")
        })?;
    let mut body =
        r::GenerateContentRequestBody::builder(max, Vec::new(), request.model.id.clone()).build();
    opts.apply_to(&mut body);
    body.model = request.model.id.clone();
    body.max_tokens = max;
    body.stream = Some(streaming);
    let mut messages = Vec::new();
    let mut system = Vec::new();
    for message in &request.messages {
        let mut content = Vec::new();
        for block in &message.content {
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
    let sampling = r::LegacySampling {
        temperature: request.settings.temperature,
        top_p: request.settings.top_p,
    };
    body.rest.extend(encode_rest(sampling)?);
    if !request.settings.stop_sequences.is_empty() {
        body.stop_sequences = Some(request.settings.stop_sequences.clone());
    }
    if !request.tools.is_empty() {
        body.tools = Some(
            request
                .tools
                .iter()
                .map(|tool| {
                    let schema: t::JsonSchema = serde_json::from_value(tool.input_schema.clone())
                        .map_err(Error::invalid)?;
                    Ok(t::ToolUnion::Custom(
                        t::Tool::builder(schema, tool.name.clone())
                            .description(tool.description.clone())
                            .build(),
                    ))
                })
                .collect::<Result<_>>()?,
        );
    }
    if let Some(choice) = &request.settings.tool_choice {
        body.tool_choice = Some(match choice {
            ToolChoice::Auto => q::ToolChoice::Auto(q::ToolChoiceAuto::builder().build()),
            ToolChoice::None => q::ToolChoice::None(q::ToolChoiceNone::builder().build()),
            ToolChoice::Required => q::ToolChoice::Any(q::ToolChoiceAny::builder().build()),
            ToolChoice::Tool { name } => {
                q::ToolChoice::Tool(q::ToolChoiceTool::builder(name.clone()).build())
            }
        });
    }
    if let Some(schema) = &request.settings.output_schema {
        let output = body
            .output_config
            .get_or_insert_with(|| q::OutputConfig::builder().build());
        let mut format =
            q::JsonOutputFormat::builder(q::JsonOutputFormatType::JsonSchema, schema.clone())
                .build();
        if let Some(original) = output.format.take() {
            format.rest = original.rest;
        }
        output.format = Some(format);
    }
    Ok(body)
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "fields", rename_all = "snake_case")]
enum DocumentSourceMetadata {
    Base64(c::PdfBase64SourceOptions),
    Text(c::PlainTextSourceOptions),
    Url(c::UrlSourceOptions),
    File(c::FileSourceOptions),
    Content {
        #[serde(default)]
        text_form: bool,
        #[serde(flatten)]
        fields: c::ContentSourceOptions,
    },
}
#[derive(Serialize, Deserialize)]
struct DocumentMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<DocumentSourceMetadata>,
    #[serde(flatten)]
    fields: c::DocumentBlockOptions,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "fields", rename_all = "snake_case")]
enum ServerResultMetadata {
    Search(c::WebSearchToolResultBlockOptions),
    Fetch(c::WebFetchToolResultBlockOptions),
    Code(c::CodeExecutionToolResultBlockOptions),
    Bash(c::BashCodeExecutionToolResultBlockOptions),
    Edit(c::TextEditorCodeExecutionToolResultBlockOptions),
    Tools(c::ToolSearchToolResultBlockOptions),
    Advisor(c::AdvisorToolResultBlockOptions),
}

#[derive(Serialize, Deserialize)]
struct ToolResultMetadata {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    native_blocks: bool,
    #[serde(flatten)]
    fields: c::ToolResultBlockOptions,
}
pub(crate) fn render_block(block: &Block) -> Result<Option<c::ContentBlock>> {
    Ok(Some(match &block.content {
        Content::Text(value) => c::ContentBlock::Text(render_text(block, value)?),
        Content::Image(value) => c::ContentBlock::Image(render_image(block, value)?),
        Content::Document(value) => c::ContentBlock::Document(render_document(block, value)?),
        Content::Reasoning(Reasoning { text }) => {
            match read_native::<ReasoningMetadata>(block, Vendor::Claude)? {
                Some(ReasoningMetadata::Thinking { signature, rest }) => c::ContentBlock::Thinking(
                    c::ThinkingBlock::builder(c::ThinkingBlockType::Tag, signature, text.clone())
                        .rest(rest)
                        .build(),
                ),
                Some(ReasoningMetadata::Redacted { data, rest }) => {
                    c::ContentBlock::RedactedThinking(
                        c::RedactedThinkingBlock::builder(c::RedactedThinkingBlockType::Tag, data)
                            .rest(rest)
                            .build(),
                    )
                }
                None => return Ok(None),
            }
        }
        Content::ToolCall(call) => {
            if call.provider_executed {
                let Some(mut meta) =
                    read_native::<r::ResponseServerToolUseBlockOptions>(block, Vendor::Claude)?
                else {
                    return Ok(None);
                };
                let name = serde_json::from_value(Value::String(call.name.clone()))
                    .map_err(Error::invalid)?;
                let mut value = c::ServerToolUseBlock::builder(
                    c::ServerToolUseBlockType::Tag,
                    call.id.clone(),
                    serde_json::from_str(&call.arguments).map_err(Error::invalid)?,
                    name,
                )
                .build();
                value.caller = meta.caller.take();
                value.rest = meta.rest;
                value.cache_control = media::claude_cache(block)?;
                c::ContentBlock::ServerToolUse(value)
            } else {
                let mut value = c::ToolUseBlock::builder(
                    c::ToolUseBlockType::Tag,
                    call.id.clone(),
                    serde_json::from_str(&call.arguments).map_err(Error::invalid)?,
                    call.name.clone(),
                )
                .build();
                if let Some(meta) =
                    read_native::<r::ResponseToolUseBlockOptions>(block, Vendor::Claude)?
                {
                    value.toolset_name = meta.toolset_name;
                    value.caller = meta.caller;
                    value.rest = meta.rest;
                }
                value.cache_control = media::claude_cache(block)?;
                c::ContentBlock::ToolUse(value)
            }
        }
        Content::ToolResult(result) => {
            if result.provider_executed {
                return render_server_result(block, result);
            }
            let metadata = read_native::<ToolResultMetadata>(block, Vendor::Claude)?;
            let native_blocks = metadata.as_ref().is_some_and(|m| m.native_blocks);
            let omitted = metadata
                .as_ref()
                .is_some_and(|m| m.fields.is_error.is_none())
                && !result.is_error;
            let mut value =
                c::ToolResultBlock::builder(c::ToolResultBlockType::Tag, result.id.clone()).build();
            if let Some(meta) = metadata {
                let mut fields = meta.fields;
                fields.tool_use_id = None;
                fields.content = None;
                fields.apply_to(&mut value);
            }
            value.content = match &result.output {
                ToolOutput::Json(value) if native_blocks => Some(c::ToolResultContent::Blocks(
                    serde_json::from_value(value.clone()).map_err(Error::invalid)?,
                )),
                ToolOutput::Json(Value::Null) => None,
                ToolOutput::Json(Value::String(text)) => {
                    Some(c::ToolResultContent::Text(text.clone()))
                }
                ToolOutput::Json(json) => Some(c::ToolResultContent::Text(json.to_string())),
                ToolOutput::Blocks(blocks) => Some(c::ToolResultContent::Blocks(
                    blocks.iter().map(render_output).collect::<Result<_>>()?,
                )),
            };
            value.is_error = if omitted { None } else { Some(result.is_error) };
            value.cache_control = media::claude_cache(block)?;
            c::ContentBlock::ToolResult(value)
        }
        Content::Opaque => return read_native(block, Vendor::Claude),
        Content::Audio(_) | Content::Video(_) | Content::File(_) => {
            return Err(Error::invalid(
                "Claude Messages accepts images and documents; this attachment kind is unsupported",
            ));
        }
    }))
}
fn render_text(block: &ContentBlock<impl Sized>, text: &Text) -> Result<c::TextBlock> {
    let mut value = c::TextBlock::builder(c::TextBlockType::Tag, text.text.clone()).build();
    let native = read_native::<r::ResponseTextBlockOptions>(block, Vendor::Claude)?;
    if let Some(meta) = native {
        value.citations = meta.citations.map(|m| m.map(|_| Vec::new()));
        value.rest = meta.rest;
    }
    if !text.citations.is_empty() {
        value.citations = Some(Some(
            text.citations
                .iter()
                .map(citations::render_claude)
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .map(c::TextCitation::Response)
                .collect(),
        ));
    }
    value.cache_control = media::claude_cache(block)?;
    Ok(value)
}
#[derive(Default, Serialize, Deserialize)]
struct ImageMetadata {
    #[serde(default)]
    source_rest: Rest,
    #[serde(flatten)]
    fields: c::ImageBlockOptions,
}
fn render_image(block: &ContentBlock<impl Sized>, image: &Image) -> Result<c::ImageBlock> {
    let source = crate::assets::source(block, &image.source, Vendor::Claude, true)?;
    let meta = read_native::<ImageMetadata>(block, Vendor::Claude)?.unwrap_or_default();
    let native = match source.as_ref() {
        Asset::Inline { bytes } => {
            let mime = match image.media_type.as_deref() {
                Some("image/jpeg") => c::ImageMediaType::Jpeg,
                Some("image/png") => c::ImageMediaType::Png,
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
                .rest(meta.source_rest.clone())
                .build(),
            )
        }
        Asset::Url { url } => c::ImageSource::Url(
            c::UrlSource::builder(url.clone())
                .rest(meta.source_rest.clone())
                .build(),
        ),
        Asset::ProviderFile { id, .. } => c::ImageSource::File(
            c::FileSource::builder(id.clone())
                .rest(meta.source_rest.clone())
                .build(),
        ),
        Asset::Reference { .. } => {
            return Err(Error::invalid(
                "prepare application assets before rendering",
            ));
        }
    };
    let mut value = c::ImageBlock::builder(c::ImageBlockType::Tag, native).build();
    let mut fields = meta.fields;
    fields.source = None;
    fields.cache_control = None;
    fields.apply_to(&mut value);
    value.cache_control = media::claude_cache(block)?;
    Ok(value)
}
fn render_document(
    block: &ContentBlock<impl Sized>,
    document: &Document,
) -> Result<c::DocumentBlock> {
    let meta = read_native::<DocumentMetadata>(block, Vendor::Claude)?;
    let (fields, source_meta) = match meta {
        Some(m) => (Some(m.fields), m.source),
        None => (None, None),
    };
    let source = match &document.source {
        DocumentSource::Asset { source } => {
            let source = crate::assets::source(block, source, Vendor::Claude, true)?;
            match source.as_ref() {
                Asset::Inline { bytes } => match document.media_type.as_deref() {
                    Some("application/pdf") => {
                        let mut source = c::PdfBase64Source::builder(
                            base64::engine::general_purpose::STANDARD.encode(bytes),
                            c::PdfMediaType::Pdf,
                        )
                        .build();
                        if let Some(DocumentSourceMetadata::Base64(m)) = source_meta {
                            source.rest = m.rest;
                        }
                        c::DocumentSource::Base64(source)
                    }
                    Some("text/plain") => {
                        let mut source = c::PlainTextSource::builder(
                            std::str::from_utf8(bytes).map_err(Error::invalid)?.into(),
                            c::TextMediaType::Plain,
                        )
                        .build();
                        if let Some(DocumentSourceMetadata::Text(m)) = source_meta {
                            source.rest = m.rest;
                        }
                        c::DocumentSource::Text(source)
                    }
                    _ => {
                        return Err(Error::invalid(
                            "Claude inline documents require PDF or text/plain",
                        ));
                    }
                },
                Asset::Url { url } => {
                    let mut source = c::UrlSource::builder(url.clone()).build();
                    if let Some(DocumentSourceMetadata::Url(m)) = source_meta {
                        source.rest = m.rest;
                    }
                    c::DocumentSource::Url(source)
                }
                Asset::ProviderFile { id, .. } => {
                    let mut source = c::FileSource::builder(id.clone()).build();
                    if let Some(DocumentSourceMetadata::File(m)) = source_meta {
                        source.rest = m.rest;
                    }
                    c::DocumentSource::File(source)
                }
                Asset::Reference { .. } => {
                    return Err(Error::invalid(
                        "prepare application assets before rendering",
                    ));
                }
            }
        }
        DocumentSource::Text { text } => {
            let original = Asset::Inline {
                bytes: text.as_bytes().to_vec(),
            };
            let prepared = crate::assets::source(block, &original, Vendor::Claude, true)?;
            if let Asset::ProviderFile { id, .. } = prepared.as_ref() {
                c::DocumentSource::File(c::FileSource::builder(id.clone()).build())
            } else {
                let mut source =
                    c::PlainTextSource::builder(text.clone(), c::TextMediaType::Plain).build();
                if let Some(DocumentSourceMetadata::Text(m)) = source_meta {
                    source.rest = m.rest;
                }
                c::DocumentSource::Text(source)
            }
        }
        DocumentSource::Chunks { chunks } => {
            let (text_form, rest) = match source_meta {
                Some(DocumentSourceMetadata::Content { text_form, fields }) => {
                    (text_form, fields.rest)
                }
                _ => (false, Rest::new()),
            };
            let content = if text_form && chunks.len() == 1 {
                match &chunks[0].content {
                    ChunkContent::Text(t) => c::DocumentContent::Text(t.text.clone()),
                    _ => c::DocumentContent::Blocks(
                        chunks.iter().map(render_chunk).collect::<Result<_>>()?,
                    ),
                }
            } else {
                c::DocumentContent::Blocks(chunks.iter().map(render_chunk).collect::<Result<_>>()?)
            };
            c::DocumentSource::Content(c::ContentSource::builder(content).rest(rest).build())
        }
    };
    let mut value = c::DocumentBlock::builder(c::DocumentBlockType::Tag, source).build();
    if let Some(mut fields) = fields {
        fields.source = None;
        fields.title = None;
        fields.context = None;
        fields.cache_control = None;
        fields.apply_to(&mut value);
    }
    value.title = document.title.clone();
    value.context = document.context.clone();
    if let Some(enabled) = document.citations {
        value
            .citations
            .get_or_insert_with(|| c::CitationsConfig::builder().build())
            .enabled = Some(enabled);
    }
    value.cache_control = media::claude_cache(block)?;
    Ok(value)
}
fn render_chunk(block: &DocumentChunk) -> Result<c::DocumentContentBlock> {
    Ok(match &block.content {
        ChunkContent::Text(value) => c::DocumentContentBlock::Text(render_text(block, value)?),
        ChunkContent::Image(value) => c::DocumentContentBlock::Image(render_image(block, value)?),
    })
}
fn render_output(block: &ToolResultBlock) -> Result<c::ToolResultContentBlock> {
    Ok(match &block.content {
        ToolResultContent::Text(value) => {
            c::ToolResultContentBlock::Text(render_text(block, value)?)
        }
        ToolResultContent::Image(value) => {
            c::ToolResultContentBlock::Image(render_image(block, value)?)
        }
        ToolResultContent::Document(value) => {
            c::ToolResultContentBlock::Document(render_document(block, value)?)
        }
    })
}
fn render_server_result(block: &Block, result: &ToolResult) -> Result<Option<c::ContentBlock>> {
    let Some(meta) = read_native::<ServerResultMetadata>(block, Vendor::Claude)? else {
        return Ok(None);
    };
    let ToolOutput::Json(output) = &result.output else {
        return Err(Error::invalid(
            "native server tool results require their JSON output",
        ));
    };
    macro_rules! restore {
        ($meta:ident,$type:ident,$tag:ident,$variant:ident) => {{
            let content = serde_json::from_value(output.clone()).map_err(Error::invalid)?;
            let mut value = c::$type::builder(c::$tag::Tag, content, result.id.clone()).build();
            let mut meta = $meta;
            meta.content = None;
            meta.tool_use_id = None;
            meta.apply_to(&mut value);
            value.cache_control = media::claude_cache(block)?;
            c::ContentBlock::$variant(value)
        }};
    }
    Ok(Some(match meta {
        ServerResultMetadata::Search(m) => restore!(
            m,
            WebSearchToolResultBlock,
            WebSearchToolResultType,
            WebSearchToolResult
        ),
        ServerResultMetadata::Fetch(m) => restore!(
            m,
            WebFetchToolResultBlock,
            WebFetchToolResultType,
            WebFetchToolResult
        ),
        ServerResultMetadata::Code(m) => restore!(
            m,
            CodeExecutionToolResultBlock,
            CodeExecutionToolResultType,
            CodeExecutionToolResult
        ),
        ServerResultMetadata::Bash(m) => restore!(
            m,
            BashCodeExecutionToolResultBlock,
            BashCodeExecutionToolResultType,
            BashCodeExecutionToolResult
        ),
        ServerResultMetadata::Edit(m) => restore!(
            m,
            TextEditorCodeExecutionToolResultBlock,
            TextEditorCodeExecutionToolResultType,
            TextEditorCodeExecutionToolResult
        ),
        ServerResultMetadata::Tools(m) => restore!(
            m,
            ToolSearchToolResultBlock,
            ToolSearchToolResultType,
            ToolSearchToolResult
        ),
        ServerResultMetadata::Advisor(m) => restore!(
            m,
            AdvisorToolResultBlock,
            AdvisorToolResultType,
            AdvisorToolResult
        ),
    }))
}

pub fn parse(response: r::GenerateContentResponseBody) -> Result<GenerateResult> {
    let finish_reason = finish_reason(response.stop_reason);
    let usage = parse_usage(&response.usage)?;
    let content = response
        .content
        .into_iter()
        .map(parse_block)
        .collect::<Result<_>>()?;
    Ok(GenerateResult {
        content,
        finish_reason,
        usage,
    })
}

fn input_citation(value: c::TextCitation) -> Result<r::ResponseTextCitation> {
    Ok(match value {
        c::TextCitation::Response(v) => v,
        c::TextCitation::Char(v) => r::ResponseTextCitation::Char(
            r::ResponseCharCitation::builder(
                r::ResponseCharCitationType::Tag,
                v.cited_text,
                v.document_index,
                v.end_char_index,
                v.start_char_index,
            )
            .document_title(Some(v.document_title))
            .rest(v.rest)
            .build(),
        ),
        c::TextCitation::Page(v) => r::ResponseTextCitation::Page(
            r::ResponsePageCitation::builder(
                r::ResponsePageCitationType::Tag,
                v.cited_text,
                v.document_index,
                v.end_page_number,
                v.start_page_number,
            )
            .document_title(Some(v.document_title))
            .rest(v.rest)
            .build(),
        ),
        c::TextCitation::ContentBlock(v) => r::ResponseTextCitation::ContentBlock(
            r::ResponseContentBlockCitation::builder(
                r::ResponseContentBlockCitationType::Tag,
                v.cited_text,
                v.document_index,
                v.end_block_index,
                v.start_block_index,
            )
            .document_title(Some(v.document_title))
            .rest(v.rest)
            .build(),
        ),
        c::TextCitation::Web(v) => r::ResponseTextCitation::Web(
            r::ResponseWebCitation::builder(
                r::ResponseWebCitationType::Tag,
                v.cited_text,
                v.encrypted_index,
                v.url,
            )
            .title(Some(v.title))
            .rest(v.rest)
            .build(),
        ),
        c::TextCitation::Search(v) => r::ResponseTextCitation::Search(
            r::ResponseSearchCitation::builder(
                r::ResponseSearchCitationType::Tag,
                v.cited_text,
                v.end_block_index,
                v.search_result_index,
                v.source,
                v.start_block_index,
            )
            .title(Some(v.title))
            .rest(v.rest)
            .build(),
        ),
    })
}
fn parse_text(mut value: r::ResponseTextBlock) -> Result<ContentBlock<Text>> {
    let native = value.citations.take();
    let parsed = native
        .as_ref()
        .and_then(Option::as_ref)
        .into_iter()
        .flatten()
        .cloned()
        .map(citations::parse_claude)
        .collect::<Result<_>>()?;
    let content = Text {
        text: std::mem::take(&mut value.text),
        citations: parsed,
    };
    let mut meta = r::ResponseTextBlockOptions::from(value);
    meta.text = None;
    meta.citations = native.map(|v| v.map(|_| Vec::new()));
    with_native(content, Vendor::Claude, meta)
}
fn parse_input_text(mut value: c::TextBlock) -> Result<ContentBlock<Text>> {
    let cache = value.cache_control.take();
    let response = r::ResponseTextBlock {
        type_: r::ResponseTextBlockType::Tag,
        text: value.text,
        citations: value
            .citations
            .map(|v| {
                v.map(|items| {
                    items
                        .into_iter()
                        .map(input_citation)
                        .collect::<Result<Vec<_>>>()
                })
                .transpose()
            })
            .transpose()?,
        rest: value.rest,
    };
    let mut block = parse_text(response)?;
    media::parse_claude_cache(&mut block, cache)?;
    Ok(block)
}
pub(crate) fn parse_block(block: r::ResponseContentBlock) -> Result<Block> {
    macro_rules! server_result {
        ($value:ident,$options:ident,$tag:ident,$kind:ident) => {{
            let mut value = $value;
            let output =
                ToolOutput::Json(serde_json::to_value(value.content).map_err(Error::invalid)?);
            let content = Content::ToolResult(ToolResult {
                id: std::mem::take(&mut value.tool_use_id),
                name: String::new(),
                output,
                is_error: false,
                provider_executed: true,
            });
            let meta = c::$options {
                type_: Some(c::$tag::Tag),
                rest: value.rest,
                ..Default::default()
            };
            with_native(content, Vendor::Claude, ServerResultMetadata::$kind(meta))
        }};
    }
    match block {
        r::ResponseContentBlock::Text(value) => parse_text(value).map(|b| b.map(Content::Text)),
        r::ResponseContentBlock::Thinking(value) => with_native(
            Content::Reasoning(Reasoning {
                text: value.thinking,
            }),
            Vendor::Claude,
            ReasoningMetadata::Thinking {
                signature: value.signature,
                rest: value.rest,
            },
        ),
        r::ResponseContentBlock::RedactedThinking(value) => with_native(
            Content::Reasoning(Reasoning {
                text: String::new(),
            }),
            Vendor::Claude,
            ReasoningMetadata::Redacted {
                data: value.data,
                rest: value.rest,
            },
        ),
        r::ResponseContentBlock::ToolUse(mut value) => {
            let content = Content::ToolCall(ToolCall {
                id: std::mem::take(&mut value.id),
                name: std::mem::take(&mut value.name),
                arguments: serde_json::to_string(&value.input).map_err(Error::invalid)?,
                provider_executed: false,
            });
            let mut meta = r::ResponseToolUseBlockOptions::from(value);
            meta.id = None;
            meta.name = None;
            meta.input = None;
            with_native(content, Vendor::Claude, meta)
        }
        r::ResponseContentBlock::ServerToolUse(mut value) => {
            let name: String =
                serde_json::from_value(serde_json::to_value(value.name).map_err(Error::invalid)?)
                    .map_err(Error::invalid)?;
            let content = Content::ToolCall(ToolCall {
                id: std::mem::take(&mut value.id),
                name,
                arguments: serde_json::to_string(&value.input).map_err(Error::invalid)?,
                provider_executed: true,
            });
            let mut meta = r::ResponseServerToolUseBlockOptions::from(value);
            meta.id = None;
            meta.input = None;
            meta.name = None;
            with_native(content, Vendor::Claude, meta)
        }
        r::ResponseContentBlock::WebSearchToolResult(value) => {
            let output =
                ToolOutput::Json(serde_json::to_value(value.content).map_err(Error::invalid)?);
            let content = Content::ToolResult(ToolResult {
                id: value.tool_use_id,
                name: String::new(),
                output,
                is_error: false,
                provider_executed: true,
            });
            let meta = c::WebSearchToolResultBlockOptions {
                type_: Some(c::WebSearchToolResultType::Tag),
                caller: value.caller,
                rest: value.rest,
                ..Default::default()
            };
            with_native(content, Vendor::Claude, ServerResultMetadata::Search(meta))
        }
        r::ResponseContentBlock::WebFetchToolResult(value) => {
            let output =
                ToolOutput::Json(serde_json::to_value(value.content).map_err(Error::invalid)?);
            let content = Content::ToolResult(ToolResult {
                id: value.tool_use_id,
                name: String::new(),
                output,
                is_error: false,
                provider_executed: true,
            });
            let meta = c::WebFetchToolResultBlockOptions {
                type_: Some(c::WebFetchToolResultType::Tag),
                caller: value.caller,
                rest: value.rest,
                ..Default::default()
            };
            with_native(content, Vendor::Claude, ServerResultMetadata::Fetch(meta))
        }
        r::ResponseContentBlock::CodeExecutionToolResult(v) => server_result!(
            v,
            CodeExecutionToolResultBlockOptions,
            CodeExecutionToolResultType,
            Code
        ),
        r::ResponseContentBlock::BashCodeExecutionToolResult(v) => server_result!(
            v,
            BashCodeExecutionToolResultBlockOptions,
            BashCodeExecutionToolResultType,
            Bash
        ),
        r::ResponseContentBlock::TextEditorCodeExecutionToolResult(v) => server_result!(
            v,
            TextEditorCodeExecutionToolResultBlockOptions,
            TextEditorCodeExecutionToolResultType,
            Edit
        ),
        r::ResponseContentBlock::ToolSearchToolResult(v) => server_result!(
            v,
            ToolSearchToolResultBlockOptions,
            ToolSearchToolResultType,
            Tools
        ),
        r::ResponseContentBlock::AdvisorToolResult(v) => server_result!(
            v,
            AdvisorToolResultBlockOptions,
            AdvisorToolResultType,
            Advisor
        ),
        other => with_native(Content::Opaque, Vendor::Claude, other),
    }
}

pub fn parse_tool_result(mut value: c::ToolResultBlock) -> Result<Block> {
    let cache = value.cache_control.take();
    let mut native_blocks = false;
    let output = match value.content.take() {
        Some(c::ToolResultContent::Blocks(blocks))
            if blocks.iter().any(|b| {
                !matches!(
                    b,
                    c::ToolResultContentBlock::Text(_)
                        | c::ToolResultContentBlock::Image(_)
                        | c::ToolResultContentBlock::Document(_)
                )
            }) =>
        {
            native_blocks = true;
            ToolOutput::Json(serde_json::to_value(blocks).map_err(Error::invalid)?)
        }
        Some(c::ToolResultContent::Text(text)) => ToolOutput::Json(Value::String(text)),
        Some(c::ToolResultContent::Blocks(blocks)) => ToolOutput::Blocks(
            blocks
                .into_iter()
                .map(|b| match b {
                    c::ToolResultContentBlock::Text(v) => {
                        parse_input_text(v).map(|b| b.map(ToolResultContent::Text))
                    }
                    c::ToolResultContentBlock::Image(v) => {
                        parse_image(v).map(|b| b.map(ToolResultContent::Image))
                    }
                    c::ToolResultContentBlock::Document(v) => {
                        parse_document(v).map(|b| b.map(ToolResultContent::Document))
                    }
                    _ => Err(Error::invalid(
                        "native result block has no portable attachment representation",
                    )),
                })
                .collect::<Result<_>>()?,
        ),
        None => ToolOutput::Json(Value::Null),
    };
    let content = Content::ToolResult(ToolResult {
        id: std::mem::take(&mut value.tool_use_id),
        name: String::new(),
        output,
        is_error: value.is_error.unwrap_or(false),
        provider_executed: false,
    });
    let mut meta = c::ToolResultBlockOptions::from(value);
    meta.tool_use_id = None;
    meta.content = None;
    let mut block = with_native(
        content,
        Vendor::Claude,
        ToolResultMetadata {
            native_blocks,
            fields: meta,
        },
    )?;
    media::parse_claude_cache(&mut block, cache)?;
    Ok(block)
}
fn parse_image(mut value: c::ImageBlock) -> Result<ContentBlock<Image>> {
    let cache = value.cache_control.take();
    let source_rest = match &value.source {
        c::ImageSource::Base64(v) => v.rest.clone(),
        c::ImageSource::Url(v) => v.rest.clone(),
        c::ImageSource::File(v) => v.rest.clone(),
    };
    let (source, mime) = match value.source {
        c::ImageSource::Base64(v) => (
            Asset::Inline {
                bytes: base64::engine::general_purpose::STANDARD
                    .decode(v.data)
                    .map_err(Error::invalid)?,
            },
            Some(
                match v.media_type {
                    c::ImageMediaType::Jpeg => "image/jpeg",
                    c::ImageMediaType::Png => "image/png",
                    c::ImageMediaType::Gif => "image/gif",
                    c::ImageMediaType::Webp => "image/webp",
                }
                .into(),
            ),
        ),
        c::ImageSource::Url(v) => (Asset::Url { url: v.url }, None),
        c::ImageSource::File(v) => (
            Asset::ProviderFile {
                vendor: Vendor::Claude,
                id: v.file_id,
            },
            None,
        ),
    };
    let mut block = ContentBlock::new(Image {
        source,
        media_type: mime,
        detail: Detail::Auto,
    });
    let meta = c::ImageBlockOptions {
        type_: Some(value.type_),
        rest: value.rest,
        ..Default::default()
    };
    block.provider_metadata.insert(
        Vendor::Claude,
        encode_rest(Native {
            native: Some(ImageMetadata {
                source_rest,
                fields: meta,
            }),
        })?,
    );
    media::parse_claude_cache(&mut block, cache)?;
    Ok(block)
}
fn parse_document(mut value: c::DocumentBlock) -> Result<DocumentBlock> {
    let cache = value.cache_control.take();
    let (source, mime, source_meta) = match value.source {
        c::DocumentSource::Base64(mut v) => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&v.data)
                .map_err(Error::invalid)?;
            v.data.clear();
            let mut m = c::PdfBase64SourceOptions::from(v);
            m.data = None;
            m.media_type = None;
            (
                DocumentSource::Asset {
                    source: Asset::Inline { bytes },
                },
                Some("application/pdf".into()),
                DocumentSourceMetadata::Base64(m),
            )
        }
        c::DocumentSource::Text(mut v) => {
            let text = std::mem::take(&mut v.data);
            let mut m = c::PlainTextSourceOptions::from(v);
            m.data = None;
            m.media_type = None;
            (
                DocumentSource::Text { text },
                Some("text/plain".into()),
                DocumentSourceMetadata::Text(m),
            )
        }
        c::DocumentSource::Url(mut v) => {
            let source = DocumentSource::Asset {
                source: Asset::Url {
                    url: std::mem::take(&mut v.url),
                },
            };
            let mut m = c::UrlSourceOptions::from(v);
            m.url = None;
            (source, None, DocumentSourceMetadata::Url(m))
        }
        c::DocumentSource::File(mut v) => {
            let source = DocumentSource::Asset {
                source: Asset::ProviderFile {
                    vendor: Vendor::Claude,
                    id: std::mem::take(&mut v.file_id),
                },
            };
            let mut m = c::FileSourceOptions::from(v);
            m.file_id = None;
            (source, None, DocumentSourceMetadata::File(m))
        }
        c::DocumentSource::Content(v) => {
            let (text_form, chunks) = match v.content {
                c::DocumentContent::Text(text) => (
                    true,
                    vec![ContentBlock::new(ChunkContent::Text(Text {
                        text,
                        citations: Vec::new(),
                    }))],
                ),
                c::DocumentContent::Blocks(blocks) => (
                    false,
                    blocks
                        .into_iter()
                        .map(|block| match block {
                            c::DocumentContentBlock::Text(v) => {
                                parse_input_text(v).map(|b| b.map(ChunkContent::Text))
                            }
                            c::DocumentContentBlock::Image(v) => {
                                parse_image(v).map(|b| b.map(ChunkContent::Image))
                            }
                        })
                        .collect::<Result<_>>()?,
                ),
            };
            let m = c::ContentSourceOptions {
                content: None,
                rest: v.rest,
            };
            (
                DocumentSource::Chunks { chunks },
                None,
                DocumentSourceMetadata::Content {
                    text_form,
                    fields: m,
                },
            )
        }
    };
    let document = Document {
        source,
        media_type: mime,
        title: value.title.clone(),
        context: value.context.clone(),
        citations: value.citations.as_ref().and_then(|v| v.enabled),
        detail: Detail::Auto,
    };
    let mut fields = c::DocumentBlockOptions {
        type_: Some(value.type_),
        source: None,
        title: None,
        context: None,
        citations: value.citations.take(),
        cache_control: None,
        rest: value.rest,
    };
    if let Some(citations) = &mut fields.citations {
        citations.enabled = None;
    }
    let mut block = with_native(
        document,
        Vendor::Claude,
        DocumentMetadata {
            source: Some(source_meta),
            fields,
        },
    )?;
    media::parse_claude_cache(&mut block, cache)?;
    Ok(block)
}

pub(crate) fn parse_usage(value: &r::Usage) -> Result<Usage> {
    Ok(Usage {
        input_tokens: Some(count(value.input_tokens)?),
        output_tokens: Some(count(value.output_tokens)?),
        reasoning_tokens: optional_count(
            value
                .output_tokens_details
                .as_ref()
                .and_then(Option::as_ref)
                .map(|d| d.thinking_tokens),
        )?,
        cache_read_tokens: optional_count(value.cache_read_input_tokens.flatten())?,
        cache_write_tokens: optional_count(value.cache_creation_input_tokens.flatten())?,
    })
}
pub(crate) fn finish_reason(reason: r::StopReason) -> FinishReason {
    match reason {
        r::StopReason::EndTurn | r::StopReason::StopSequence => FinishReason::Stop,
        r::StopReason::MaxTokens => FinishReason::Length,
        r::StopReason::ToolUse => FinishReason::ToolCalls,
        r::StopReason::Refusal => FinishReason::ContentBlocked,
        r::StopReason::PauseTurn => FinishReason::Other("pause_turn".into()),
        r::StopReason::Compaction => FinishReason::Other("compaction".into()),
        r::StopReason::ModelContextWindowExceeded => {
            FinishReason::Other("model_context_window_exceeded".into())
        }
    }
}
