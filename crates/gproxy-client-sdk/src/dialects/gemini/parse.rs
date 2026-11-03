use super::*;
pub fn parse(response: g::GenerateContentResponseBody) -> Result<GenerateResult> {
    let metadata = response_metadata(&response);
    let warnings = if response.candidates.as_ref().is_some_and(|v| v.len() > 1) {
        vec![Warning::new(
            WarningKind::Compatibility,
            "response.candidates",
            "Only the first candidate is returned",
        )]
    } else {
        Vec::new()
    };
    check_response(&response)?;
    let usage = parse_usage(response.usage_metadata.as_ref())?;
    let candidate = response.candidates.into_iter().flatten().next();
    let reason = candidate.as_ref().and_then(|c| c.finish_reason);
    let sources = candidate
        .as_ref()
        .map(crate::dialects::shared::sources::gemini_candidate)
        .transpose()?
        .unwrap_or_default();
    let sources = crate::dialects::shared::sources::unique(sources);
    let links = candidate
        .as_ref()
        .map(citations::gemini_citations)
        .transpose()?
        .unwrap_or_default();
    let mut content = Vec::new();
    for (index, part) in candidate
        .into_iter()
        .filter_map(|c| c.content)
        .flat_map(|c| c.parts.into_iter().flatten())
        .enumerate()
    {
        if let Some(block) = content.last_mut()
            && merge_transcription(block, &part)?.is_some()
        {
            continue;
        }
        content.push(parse_block(part, index as u32)?);
    }
    for (index, citation) in links {
        if let Some(Block {
            content: Content::Text(text),
            ..
        }) = content.get_mut(index)
        {
            text.citations.push(citation);
        }
    }
    let has_tools = content
        .iter()
        .any(|b| matches!(b.content, Content::ToolCall(_)));
    Ok(GenerateResult {
        response: metadata,
        warnings,
        sources,
        content,
        finish_reason: finish_reason(reason, has_tools),
        usage,
    })
}

pub(crate) fn check_response(response: &g::GenerateContentResponseBody) -> Result<()> {
    if response.error.is_some() {
        return Err(Error::from_event(
            serde_json::to_string(response).map_err(Error::invalid)?,
        ));
    }
    if response
        .prompt_feedback
        .as_ref()
        .and_then(|p| p.block_reason)
        .is_some_and(|r| r != g::BlockReason::Unspecified)
    {
        return Err(Error::content_blocked(
            serde_json::to_string(response).map_err(Error::invalid)?,
        ));
    }
    Ok(())
}

pub(crate) fn parse_usage(value: Option<&g::UsageMetadata>) -> Result<Usage> {
    let Some(value) = value else {
        return Ok(Usage::default());
    };
    Ok(Usage {
        input_tokens: optional_count(value.prompt_token_count)?,
        output_tokens: optional_count(value.candidates_token_count)?,
        reasoning_tokens: optional_count(value.thoughts_token_count)?,
        cache_read_tokens: optional_count(value.cached_content_token_count)?,
        cache_write_tokens: None,
    })
}

pub(crate) fn finish_reason(reason: Option<g::FinishReason>, has_tools: bool) -> FinishReason {
    let Some(reason) = reason else {
        return FinishReason::new(FinishReasonKind::Other, None);
    };
    let (mut kind, raw) = match reason {
        g::FinishReason::Stop => (FinishReasonKind::Stop, "STOP"),
        g::FinishReason::MaxTokens => (FinishReasonKind::Length, "MAX_TOKENS"),
        g::FinishReason::Safety => (FinishReasonKind::ContentBlocked, "SAFETY"),
        g::FinishReason::Blocklist => (FinishReasonKind::ContentBlocked, "BLOCKLIST"),
        g::FinishReason::ProhibitedContent => {
            (FinishReasonKind::ContentBlocked, "PROHIBITED_CONTENT")
        }
        g::FinishReason::Spii => (FinishReasonKind::ContentBlocked, "SPII"),
        g::FinishReason::ImageSafety => (FinishReasonKind::ContentBlocked, "IMAGE_SAFETY"),
        g::FinishReason::Unspecified => (FinishReasonKind::Other, "FINISH_REASON_UNSPECIFIED"),
        g::FinishReason::Continuation => (FinishReasonKind::Other, "CONTINUATION"),
        g::FinishReason::Recitation => (FinishReasonKind::Other, "RECITATION"),
        g::FinishReason::Language => (FinishReasonKind::Other, "LANGUAGE"),
        g::FinishReason::Other => (FinishReasonKind::Other, "OTHER"),
        g::FinishReason::MalformedFunctionCall => {
            (FinishReasonKind::Other, "MALFORMED_FUNCTION_CALL")
        }
        g::FinishReason::ImageProhibitedContent => {
            (FinishReasonKind::ContentBlocked, "IMAGE_PROHIBITED_CONTENT")
        }
        g::FinishReason::ImageOther => (FinishReasonKind::Other, "IMAGE_OTHER"),
        g::FinishReason::NoImage => (FinishReasonKind::Other, "NO_IMAGE"),
        g::FinishReason::ImageRecitation => (FinishReasonKind::Other, "IMAGE_RECITATION"),
        g::FinishReason::UnexpectedToolCall => (FinishReasonKind::Other, "UNEXPECTED_TOOL_CALL"),
        g::FinishReason::TooManyToolCalls => (FinishReasonKind::Other, "TOO_MANY_TOOL_CALLS"),
        g::FinishReason::MissingThoughtSignature => {
            (FinishReasonKind::Other, "MISSING_THOUGHT_SIGNATURE")
        }
        g::FinishReason::MalformedResponse => (FinishReasonKind::Other, "MALFORMED_RESPONSE"),
        g::FinishReason::Escalation => (FinishReasonKind::Other, "ESCALATION"),
        g::FinishReason::PupLimitedDisabled => (FinishReasonKind::Other, "PUP_LIMITED_DISABLED"),
    };
    if kind == FinishReasonKind::Stop && has_tools {
        kind = FinishReasonKind::ToolCalls;
    }
    FinishReason::new(kind, Some(raw.into()))
}

pub(crate) fn response_metadata(response: &g::GenerateContentResponseBody) -> ResponseMetadata {
    ResponseMetadata {
        id: response.response_id.clone(),
        model: response.model_version.clone(),
        timestamp: None,
    }
}

pub(crate) fn parse_block(part: g::Part, index: u32) -> Result<Block> {
    let mut fields = part;
    let mut meta = PartMetadata::default();
    let detail = media::parse_gemini_detail(fields.media_resolution.as_ref());
    if detail != Detail::Auto
        && let Some(resolution) = fields.media_resolution.as_mut()
    {
        resolution.level = None;
    }

    let content = if let Some(text) = fields.text.take() {
        if fields.thought == Some(true) {
            Content::Reasoning(Reasoning { text })
        } else {
            Content::Text(Text {
                refusal: false,
                text,
                citations: Vec::new(),
            })
        }
    } else if let Some(call) = fields.function_call.take() {
        parse_function_call(call, index, &mut meta)?
    } else if let Some(output) = fields.function_response.take() {
        parse_function_response(output, &mut meta)?
    } else if let Some(blob) = fields.inline_data.take() {
        parse_inline(blob, detail, &mut fields, &mut meta)?
    } else if let Some(file) = fields.file_data.take() {
        parse_file_part(file, detail, &mut fields, &mut meta)?
    } else if let Some(call) = fields.tool_call.take() {
        parse_server_call(call, index, &mut meta)?
    } else if let Some(result) = fields.tool_response.take() {
        parse_server_result(result, &mut meta)?
    } else {
        return with_native(Content::Opaque, Vendor::Gemini, fields);
    };
    if let Content::Video(_) = &content
        && let Some(video) = fields.video_metadata.as_mut()
    {
        video.fps = None;
    }
    meta.fields = part_fields(fields);
    with_native(content, Vendor::Gemini, meta)
}

mod content;
use content::*;
