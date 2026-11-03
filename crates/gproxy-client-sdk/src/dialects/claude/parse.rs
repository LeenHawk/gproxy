use super::*;

pub fn parse(response: r::GenerateContentResponseBody) -> Result<GenerateResult> {
    let metadata = response_metadata(&response);
    let sources = response
        .content
        .iter()
        .map(crate::dialects::shared::sources::claude_block)
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    let sources = crate::dialects::shared::sources::unique(sources);
    let refused = response.stop_reason == r::StopReason::Refusal;
    let mut content = response
        .content
        .into_iter()
        .map(parse_block)
        .collect::<Result<Vec<_>>>()?;
    if refused {
        for block in &mut content {
            if let Content::Text(text) = &mut block.content {
                text.refusal = true;
            }
        }
    }
    Ok(GenerateResult {
        response: metadata,
        finish_reason: finish_reason(response.stop_reason),
        usage: parse_usage(&response.usage)?,
        content,
        warnings: Vec::new(),
        sources,
    })
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
    let (kind, raw) = match reason {
        r::StopReason::EndTurn => (FinishReasonKind::Stop, "end_turn"),
        r::StopReason::StopSequence => (FinishReasonKind::Stop, "stop_sequence"),
        r::StopReason::MaxTokens => (FinishReasonKind::Length, "max_tokens"),
        r::StopReason::ToolUse => (FinishReasonKind::ToolCalls, "tool_use"),
        r::StopReason::Refusal => (FinishReasonKind::ContentBlocked, "refusal"),
        r::StopReason::PauseTurn => (FinishReasonKind::Other, "pause_turn"),
        r::StopReason::Compaction => (FinishReasonKind::Other, "compaction"),
        r::StopReason::ModelContextWindowExceeded => {
            (FinishReasonKind::Length, "model_context_window_exceeded")
        }
    };
    FinishReason::new(kind, Some(raw.into()))
}

pub(crate) fn response_metadata(response: &r::GenerateContentResponseBody) -> ResponseMetadata {
    ResponseMetadata {
        id: Some(response.id.clone()),
        model: Some(response.model.clone()),
        timestamp: None,
    }
}
mod content;
use content::*;

pub(crate) use content::parse_block;

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
            refusal: false,
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
