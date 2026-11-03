use super::*;
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
                ToolOutput::Unknown(_) => {
                    return Err(Error::invalid("unknown persisted tool output"));
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
        Content::Unknown(_) | Content::Interrupted(_) => return Ok(None),
        Content::Opaque => return read_native(block, Vendor::Claude),
        Content::File(_) | Content::Audio(_) | Content::Video(_) => {
            return Err(Error::invalid(
                "Claude Messages accepts images and documents; this attachment kind is unsupported",
            ));
        }
    }))
}

pub(super) fn render_text(block: &ContentBlock<impl Sized>, text: &Text) -> Result<c::TextBlock> {
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

pub(super) fn render_chunk(block: &DocumentChunk) -> Result<c::DocumentContentBlock> {
    Ok(match &block.content {
        ChunkContent::Unknown(_) => return Err(Error::invalid("unknown persisted document chunk")),
        ChunkContent::Text(text) => c::DocumentContentBlock::Text(render_text(block, text)?),
        ChunkContent::Image(image) => c::DocumentContentBlock::Image(render_image(block, image)?),
    })
}

pub(super) fn render_output(block: &ToolResultBlock) -> Result<c::ToolResultContentBlock> {
    Ok(match &block.content {
        ToolResultContent::Unknown(_) => {
            return Err(Error::invalid("unknown persisted tool content"));
        }
        ToolResultContent::Text(text) => c::ToolResultContentBlock::Text(render_text(block, text)?),
        ToolResultContent::Image(image) => {
            c::ToolResultContentBlock::Image(render_image(block, image)?)
        }
        ToolResultContent::Document(document) => {
            c::ToolResultContentBlock::Document(render_document(block, document)?)
        }
    })
}

pub(super) fn render_server_result(
    block: &Block,
    result: &ToolResult,
) -> Result<Option<c::ContentBlock>> {
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
