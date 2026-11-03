use super::*;
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

pub(super) fn parse_text(value: r::ResponseTextBlock) -> Result<ContentBlock<Text>> {
    let citations = value.citations.flatten();
    let empty_citations = citations.as_ref().is_some_and(Vec::is_empty);
    let citations = citations
        .unwrap_or_default()
        .into_iter()
        .map(citations::parse_claude)
        .collect::<Result<_>>()?;
    with_native(
        Text {
            refusal: false,
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

pub(super) fn parse_image(value: c::ImageBlock) -> Result<ContentBlock<Image>> {
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

pub(super) fn parse_document(value: c::DocumentBlock) -> Result<DocumentBlock> {
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
                        refusal: false,
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
