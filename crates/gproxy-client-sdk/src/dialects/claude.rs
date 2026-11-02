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
    let opts = &request.provider_options.claude;
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

#[derive(Serialize)]
struct Sampling {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct TextMetadata {
    empty_citations: bool,
    #[serde(flatten)]
    rest: Rest,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct ToolMetadata {
    caller: Option<c::Caller>,
    toolset_name: Option<String>,
    #[serde(flatten)]
    rest: Rest,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct ImageMetadata {
    source_rest: Rest,
    #[serde(flatten)]
    rest: Rest,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct DocumentMetadata {
    source_rest: Rest,
    chunk_text: bool,
    citations: Option<c::CitationsConfig>,
    #[serde(flatten)]
    rest: Rest,
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct ToolResultMetadata {
    toolset_name: Option<String>,
    is_error_present: bool,
    native_blocks: bool,
    #[serde(flatten)]
    rest: Rest,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ServerResultKind {
    Search,
    Fetch,
    Code,
    Bash,
    Edit,
    Tools,
    Advisor,
}
#[derive(Serialize, Deserialize)]
struct ServerResultMetadata {
    kind: ServerResultKind,
    caller: Option<c::Caller>,
    #[serde(default, flatten)]
    rest: Rest,
}

pub(crate) fn render_block(block: &Block) -> Result<Option<c::ContentBlock>> {
    Ok(Some(match &block.content {
        Content::Text(text) => c::ContentBlock::Text(render_text(block, text)?),
        Content::Image(image) => c::ContentBlock::Image(render_image(block, image)?),
        Content::Document(document) => c::ContentBlock::Document(render_document(block, document)?),
        Content::Reasoning(reasoning) => {
            match read_native::<ReasoningMetadata>(block, Vendor::Claude)? {
                Some(ReasoningMetadata::Thinking { signature, rest }) => c::ContentBlock::Thinking(
                    c::ThinkingBlock::builder(
                        c::ThinkingBlockType::Tag,
                        signature,
                        reasoning.text.clone(),
                    )
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
            let meta = read_native::<ToolMetadata>(block, Vendor::Claude)?;
            if call.provider_executed {
                let Some(meta) = meta else { return Ok(None) };
                let name = serde_json::from_value(Value::String(call.name.clone()))
                    .map_err(Error::invalid)?;
                let input = serde_json::from_str(&call.arguments).map_err(Error::invalid)?;
                let mut value = c::ServerToolUseBlock::builder(
                    c::ServerToolUseBlockType::Tag,
                    call.id.clone(),
                    input,
                    name,
                )
                .rest(meta.rest)
                .build();
                value.caller = meta.caller;
                value.cache_control = media::claude_cache(block)?;
                c::ContentBlock::ServerToolUse(value)
            } else {
                let meta = meta.unwrap_or_default();
                let input = serde_json::from_str(&call.arguments).map_err(Error::invalid)?;
                let mut value = c::ToolUseBlock::builder(
                    c::ToolUseBlockType::Tag,
                    call.id.clone(),
                    input,
                    call.name.clone(),
                )
                .rest(meta.rest)
                .build();
                value.caller = meta.caller;
                value.toolset_name = meta.toolset_name;
                value.cache_control = media::claude_cache(block)?;
                c::ContentBlock::ToolUse(value)
            }
        }
        Content::ToolResult(result) => {
            if result.provider_executed {
                return render_server_result(block, result);
            }
            let meta = read_native::<ToolResultMetadata>(block, Vendor::Claude)?;
            let mut value =
                c::ToolResultBlock::builder(c::ToolResultBlockType::Tag, result.id.clone()).build();
            let meta = meta.unwrap_or_default();
            value.rest = meta.rest;
            value.toolset_name = meta.toolset_name;
            value.is_error = (meta.is_error_present || result.is_error).then_some(result.is_error);
            value.content = match &result.output {
                ToolOutput::Json(value) if meta.native_blocks => {
                    Some(c::ToolResultContent::Blocks(
                        serde_json::from_value(value.clone()).map_err(Error::invalid)?,
                    ))
                }
                ToolOutput::Json(Value::Null) => None,
                ToolOutput::Json(Value::String(text)) => {
                    Some(c::ToolResultContent::Text(text.clone()))
                }
                ToolOutput::Json(value) => Some(c::ToolResultContent::Text(value.to_string())),
                ToolOutput::Blocks(blocks) => Some(c::ToolResultContent::Blocks(
                    blocks.iter().map(render_output).collect::<Result<_>>()?,
                )),
            };
            value.cache_control = media::claude_cache(block)?;
            c::ContentBlock::ToolResult(value)
        }
        Content::Opaque => return read_native(block, Vendor::Claude),
        Content::File(_) | Content::Audio(_) | Content::Video(_) => {
            return Err(Error::invalid(
                "Claude Messages accepts images and documents; this attachment kind is unsupported",
            ));
        }
    }))
}
fn render_text(block: &ContentBlock<impl Sized>, text: &Text) -> Result<c::TextBlock> {
    let meta = read_native::<TextMetadata>(block, Vendor::Claude)?.unwrap_or_default();
    let mut value = c::TextBlock::builder(c::TextBlockType::Tag, text.text.clone())
        .rest(meta.rest)
        .build();
    let citations = text
        .citations
        .iter()
        .map(citations::render_claude)
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    if !citations.is_empty() || meta.empty_citations {
        value.citations = Some(citations);
    }
    value.cache_control = media::claude_cache(block)?;
    Ok(value)
}
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
        DocumentSource::Text { text } => c::DocumentSource::Text(
            c::PlainTextSource::builder(text.clone(), c::TextMediaType::Plain)
                .rest(meta.source_rest)
                .build(),
        ),
        DocumentSource::Chunks { chunks } => {
            let content = if meta.chunk_text && chunks.len() == 1 {
                match &chunks[0].content {
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
fn render_chunk(block: &DocumentChunk) -> Result<c::DocumentContentBlock> {
    Ok(match &block.content {
        ChunkContent::Text(text) => c::DocumentContentBlock::Text(render_text(block, text)?),
        ChunkContent::Image(image) => c::DocumentContentBlock::Image(render_image(block, image)?),
    })
}
fn render_output(block: &ToolResultBlock) -> Result<c::ToolResultContentBlock> {
    Ok(match &block.content {
        ToolResultContent::Text(text) => c::ToolResultContentBlock::Text(render_text(block, text)?),
        ToolResultContent::Image(image) => {
            c::ToolResultContentBlock::Image(render_image(block, image)?)
        }
        ToolResultContent::Document(document) => {
            c::ToolResultContentBlock::Document(render_document(block, document)?)
        }
    })
}
fn render_server_result(block: &Block, result: &ToolResult) -> Result<Option<c::ContentBlock>> {
    let Some(meta) = read_native::<ServerResultMetadata>(block, Vendor::Claude)? else {
        return Ok(None);
    };
    let ToolOutput::Json(output) = &result.output else {
        return Err(Error::invalid("native server tools require JSON output"));
    };
    macro_rules! restore {
        ($ty:ident,$tag:ident) => {{
            let content = serde_json::from_value(output.clone()).map_err(Error::invalid)?;
            let mut value = c::$ty::builder(c::$tag::Tag, content, result.id.clone())
                .rest(meta.rest)
                .build();
            value.cache_control = media::claude_cache(block)?;
            value
        }};
    }
    let value = match meta.kind {
        ServerResultKind::Search => {
            let mut value = restore!(WebSearchToolResultBlock, WebSearchToolResultType);
            value.caller = meta.caller;
            c::ContentBlock::WebSearchToolResult(value)
        }
        ServerResultKind::Fetch => {
            let mut value = restore!(WebFetchToolResultBlock, WebFetchToolResultType);
            value.caller = meta.caller;
            c::ContentBlock::WebFetchToolResult(value)
        }
        ServerResultKind::Code => c::ContentBlock::CodeExecutionToolResult(restore!(
            CodeExecutionToolResultBlock,
            CodeExecutionToolResultType
        )),
        ServerResultKind::Bash => c::ContentBlock::BashCodeExecutionToolResult(restore!(
            BashCodeExecutionToolResultBlock,
            BashCodeExecutionToolResultType
        )),
        ServerResultKind::Edit => c::ContentBlock::TextEditorCodeExecutionToolResult(restore!(
            TextEditorCodeExecutionToolResultBlock,
            TextEditorCodeExecutionToolResultType
        )),
        ServerResultKind::Tools => c::ContentBlock::ToolSearchToolResult(restore!(
            ToolSearchToolResultBlock,
            ToolSearchToolResultType
        )),
        ServerResultKind::Advisor => c::ContentBlock::AdvisorToolResult(restore!(
            AdvisorToolResultBlock,
            AdvisorToolResultType
        )),
    };
    Ok(Some(value))
}
pub fn parse(response: r::GenerateContentResponseBody) -> Result<GenerateResult> {
    Ok(GenerateResult {
        finish_reason: finish_reason(response.stop_reason),
        usage: parse_usage(&response.usage)?,
        content: response
            .content
            .into_iter()
            .map(parse_block)
            .collect::<Result<_>>()?,
    })
}
pub(crate) fn parse_block(block: r::ResponseContentBlock) -> Result<Block> {
    macro_rules! server {
        ($v:ident,$kind:ident,$caller:expr) => {{
            let content = Content::ToolResult(ToolResult {
                id: $v.tool_use_id,
                name: String::new(),
                output: ToolOutput::Json(serde_json::to_value($v.content).map_err(Error::invalid)?),
                is_error: false,
                provider_executed: true,
            });
            with_native(
                content,
                Vendor::Claude,
                ServerResultMetadata {
                    kind: ServerResultKind::$kind,
                    caller: $caller,
                    rest: $v.rest,
                },
            )
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
        r::ResponseContentBlock::ToolUse(v) => with_native(
            Content::ToolCall(ToolCall {
                id: v.id,
                name: v.name,
                arguments: serde_json::to_string(&v.input).map_err(Error::invalid)?,
                provider_executed: false,
            }),
            Vendor::Claude,
            ToolMetadata {
                caller: v.caller.flatten(),
                toolset_name: v.toolset_name.flatten(),
                rest: v.rest,
            },
        ),
        r::ResponseContentBlock::ServerToolUse(v) => {
            let name =
                serde_json::from_value(serde_json::to_value(v.name).map_err(Error::invalid)?)
                    .map_err(Error::invalid)?;
            with_native(
                Content::ToolCall(ToolCall {
                    id: v.id,
                    name,
                    arguments: serde_json::to_string(&v.input).map_err(Error::invalid)?,
                    provider_executed: true,
                }),
                Vendor::Claude,
                ToolMetadata {
                    caller: v.caller.flatten(),
                    toolset_name: None,
                    rest: v.rest,
                },
            )
        }
        r::ResponseContentBlock::WebSearchToolResult(v) => server!(v, Search, v.caller.flatten()),
        r::ResponseContentBlock::WebFetchToolResult(v) => server!(v, Fetch, v.caller.flatten()),
        r::ResponseContentBlock::CodeExecutionToolResult(v) => server!(v, Code, None),
        r::ResponseContentBlock::BashCodeExecutionToolResult(v) => server!(v, Bash, None),
        r::ResponseContentBlock::TextEditorCodeExecutionToolResult(v) => server!(v, Edit, None),
        r::ResponseContentBlock::ToolSearchToolResult(v) => server!(v, Tools, None),
        r::ResponseContentBlock::AdvisorToolResult(v) => server!(v, Advisor, None),
        other => with_native(Content::Opaque, Vendor::Claude, other),
    }
}
fn parse_text(value: r::ResponseTextBlock) -> Result<ContentBlock<Text>> {
    let citations = value.citations.flatten();
    let empty_citations = citations.as_ref().is_some_and(Vec::is_empty);
    let citations = citations
        .unwrap_or_default()
        .into_iter()
        .map(citations::parse_claude)
        .collect::<Result<_>>()?;
    with_native(
        Text {
            text: value.text,
            citations,
        },
        Vendor::Claude,
        TextMetadata {
            empty_citations,
            rest: value.rest,
        },
    )
}
fn parse_input_text(value: c::TextBlock) -> Result<ContentBlock<Text>> {
    let cache = value.cache_control;
    let empty_citations = value.citations.as_ref().is_some_and(Vec::is_empty);
    let citations = value
        .citations
        .unwrap_or_default()
        .into_iter()
        .map(input_citation)
        .map(|c| c.and_then(citations::parse_claude))
        .collect::<Result<_>>()?;
    let mut block = with_native(
        Text {
            text: value.text,
            citations,
        },
        Vendor::Claude,
        TextMetadata {
            empty_citations,
            rest: value.rest,
        },
    )?;
    media::parse_claude_cache(&mut block, cache)?;
    Ok(block)
}
fn input_citation(value: c::TextCitation) -> Result<r::ResponseTextCitation> {
    Ok(match value {
        c::TextCitation::Char(v) => r::ResponseTextCitation::Char(
            r::ResponseCharCitation::builder(
                r::ResponseCharCitationType::Tag,
                v.cited_text,
                v.document_index,
                v.end_char_index,
                v.start_char_index,
            )
            .document_title(v.document_title)
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
            .document_title(v.document_title)
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
            .document_title(v.document_title)
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
pub fn parse_tool_result(value: c::ToolResultBlock) -> Result<Block> {
    let mut native_blocks = false;
    let output = match value.content {
        Some(c::ToolResultContent::Text(text)) => ToolOutput::Json(Value::String(text)),
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
                    _ => Err(Error::invalid("tool output has no portable representation")),
                })
                .collect::<Result<_>>()?,
        ),
        None => ToolOutput::Json(Value::Null),
    };
    let mut block = with_native(
        Content::ToolResult(ToolResult {
            id: value.tool_use_id,
            name: String::new(),
            output,
            is_error: value.is_error.unwrap_or(false),
            provider_executed: false,
        }),
        Vendor::Claude,
        ToolResultMetadata {
            toolset_name: value.toolset_name,
            is_error_present: value.is_error.is_some(),
            native_blocks,
            rest: value.rest,
        },
    )?;
    media::parse_claude_cache(&mut block, value.cache_control)?;
    Ok(block)
}
fn parse_image(value: c::ImageBlock) -> Result<ContentBlock<Image>> {
    let (source, media_type, source_rest) = match value.source {
        c::ImageSource::Base64(v) => {
            let mime = match v.media_type {
                c::ImageMediaType::Png => "image/png",
                c::ImageMediaType::Jpeg => "image/jpeg",
                c::ImageMediaType::Gif => "image/gif",
                c::ImageMediaType::Webp => "image/webp",
            };
            (
                Asset::Inline {
                    bytes: base64::engine::general_purpose::STANDARD
                        .decode(v.data)
                        .map_err(Error::invalid)?,
                },
                Some(mime.into()),
                v.rest,
            )
        }
        c::ImageSource::Url(v) => (Asset::Url { url: v.url }, None, v.rest),
        c::ImageSource::File(v) => (
            Asset::ProviderFile {
                vendor: Vendor::Claude,
                id: v.file_id,
            },
            None,
            v.rest,
        ),
    };
    let mut block = with_native(
        Image {
            source,
            media_type,
            detail: Detail::Auto,
        },
        Vendor::Claude,
        ImageMetadata {
            source_rest,
            rest: value.rest,
        },
    )?;
    media::parse_claude_cache(&mut block, value.cache_control)?;
    Ok(block)
}
fn parse_document(value: c::DocumentBlock) -> Result<DocumentBlock> {
    let mut chunk_text = false;
    let (source, media_type, source_rest) = match value.source {
        c::DocumentSource::Base64(v) => (
            DocumentSource::Asset {
                source: Asset::Inline {
                    bytes: base64::engine::general_purpose::STANDARD
                        .decode(v.data)
                        .map_err(Error::invalid)?,
                },
            },
            Some("application/pdf".into()),
            v.rest,
        ),
        c::DocumentSource::Text(v) => (
            DocumentSource::Text { text: v.data },
            Some("text/plain".into()),
            v.rest,
        ),
        c::DocumentSource::Url(v) => (
            DocumentSource::Asset {
                source: Asset::Url { url: v.url },
            },
            None,
            v.rest,
        ),
        c::DocumentSource::File(v) => (
            DocumentSource::Asset {
                source: Asset::ProviderFile {
                    vendor: Vendor::Claude,
                    id: v.file_id,
                },
            },
            None,
            v.rest,
        ),
        c::DocumentSource::Content(v) => {
            let chunks = match v.content {
                c::DocumentContent::Text(text) => {
                    chunk_text = true;
                    vec![ContentBlock::new(ChunkContent::Text(Text {
                        text,
                        citations: Vec::new(),
                    }))]
                }
                c::DocumentContent::Blocks(blocks) => blocks
                    .into_iter()
                    .map(|b| match b {
                        c::DocumentContentBlock::Text(v) => {
                            parse_input_text(v).map(|b| b.map(ChunkContent::Text))
                        }
                        c::DocumentContentBlock::Image(v) => {
                            parse_image(v).map(|b| b.map(ChunkContent::Image))
                        }
                    })
                    .collect::<Result<_>>()?,
            };
            (DocumentSource::Chunks { chunks }, None, v.rest)
        }
    };
    let mut citations = value.citations;
    let enabled = citations.as_ref().and_then(|c| c.enabled);
    if let Some(c) = &mut citations {
        c.enabled = None;
    }
    let document = Document {
        source,
        media_type,
        title: value.title,
        context: value.context,
        citations: enabled,
        detail: Detail::Auto,
    };
    let mut block = with_native(
        document,
        Vendor::Claude,
        DocumentMetadata {
            source_rest,
            chunk_text,
            citations,
            rest: value.rest,
        },
    )?;
    media::parse_claude_cache(&mut block, value.cache_control)?;
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
