use super::*;

pub fn parse(response: r::GenerateContentResponseBody) -> Result<GenerateResult> {
    let metadata = response_metadata(&response);
    let sources = response
        .output
        .iter()
        .map(crate::dialects::shared::sources::openai_item)
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    let sources = crate::dialects::shared::sources::unique(sources);
    let (finish_reason, usage) = finish_metadata(&response)?;
    let mut content = Vec::new();
    for (index, item) in response.output.into_iter().enumerate() {
        content.extend(parse_item(item, index as u32)?);
    }
    Ok(GenerateResult {
        response: metadata,
        warnings: Vec::new(),
        sources,
        content,
        finish_reason,
        usage,
    })
}

pub(crate) fn response_metadata(response: &r::GenerateContentResponseBody) -> ResponseMetadata {
    ResponseMetadata {
        id: Some(response.id.clone()),
        model: Some(response.model.clone()),
        timestamp: Some(response.created_at),
    }
}

pub(crate) fn parse_usage(response: &r::GenerateContentResponseBody) -> Result<Usage> {
    let usage = match response.usage.as_ref().and_then(Option::as_ref) {
        None => Usage::default(),
        Some(value) => Usage {
            input_tokens: Some(count(value.input_tokens)?),
            output_tokens: Some(count(value.output_tokens)?),
            reasoning_tokens: Some(count(value.output_tokens_details.reasoning_tokens)?),
            cache_read_tokens: Some(count(value.input_tokens_details.cached_tokens)?),
            cache_write_tokens: Some(count(value.input_tokens_details.cache_write_tokens)?),
        },
    };
    Ok(usage)
}

pub(crate) fn error_finish_reason(response: &r::GenerateContentResponseBody) -> FinishReason {
    let raw = response
        .incomplete_details
        .as_ref()
        .and_then(|d| d.reason)
        .map(|reason| match reason {
            r::ResponseIncompleteReason::MaxOutputTokens => "max_output_tokens",
            r::ResponseIncompleteReason::ContentFilter => "content_filter",
            r::ResponseIncompleteReason::MaxMessages => "max_messages",
            r::ResponseIncompleteReason::Steered => "steered",
            r::ResponseIncompleteReason::Interrupted => "interrupted",
        })
        .or_else(|| {
            response.status.map(|status| match status {
                r::ResponseStatus::Completed => "completed",
                r::ResponseStatus::Failed => "failed",
                r::ResponseStatus::Incomplete => "incomplete",
                r::ResponseStatus::Cancelled => "cancelled",
                r::ResponseStatus::Queued => "queued",
                r::ResponseStatus::InProgress => "in_progress",
            })
        });
    FinishReason::new(FinishReasonKind::Error, raw.map(str::to_owned))
}

pub(crate) fn finish_metadata(
    response: &r::GenerateContentResponseBody,
) -> Result<(FinishReason, Usage)> {
    if response.error.is_some() {
        return Err(Error::from_event(
            serde_json::to_string(response).map_err(Error::invalid)?,
        ));
    }
    let (mut reason, raw) = if let Some(incomplete) =
        response.incomplete_details.as_ref().and_then(|d| d.reason)
    {
        let (kind, raw) = match incomplete {
            r::ResponseIncompleteReason::MaxOutputTokens => {
                (FinishReasonKind::Length, "max_output_tokens")
            }
            r::ResponseIncompleteReason::ContentFilter => {
                (FinishReasonKind::ContentBlocked, "content_filter")
            }
            r::ResponseIncompleteReason::MaxMessages => (FinishReasonKind::Length, "max_messages"),
            r::ResponseIncompleteReason::Steered => (FinishReasonKind::Interrupted, "steered"),
            r::ResponseIncompleteReason::Interrupted => {
                (FinishReasonKind::Interrupted, "interrupted")
            }
        };
        (kind, Some(raw.into()))
    } else {
        match response.status {
            Some(r::ResponseStatus::Completed) => {
                (FinishReasonKind::Stop, Some("completed".into()))
            }
            Some(r::ResponseStatus::Failed) => (FinishReasonKind::Error, Some("failed".into())),
            Some(r::ResponseStatus::Incomplete) => {
                (FinishReasonKind::Interrupted, Some("incomplete".into()))
            }
            Some(r::ResponseStatus::Cancelled) => {
                (FinishReasonKind::Interrupted, Some("cancelled".into()))
            }
            Some(r::ResponseStatus::Queued) => {
                (FinishReasonKind::Interrupted, Some("queued".into()))
            }
            Some(r::ResponseStatus::InProgress) => {
                (FinishReasonKind::Interrupted, Some("in_progress".into()))
            }
            None => (FinishReasonKind::Other, None),
        }
    };
    if reason == FinishReasonKind::Stop
        && response
            .output
            .iter()
            .any(|item| matches!(item, r::ResponseOutputItem::FunctionCall(_)))
    {
        reason = FinishReasonKind::ToolCalls;
    }
    let refused = response.output.iter().any(|item| match item {
        r::ResponseOutputItem::Message(message) => message
            .content
            .iter()
            .any(|part| matches!(part, i::OutputContent::Refusal(_))),
        _ => false,
    });
    if refused {
        reason = FinishReasonKind::ContentBlocked;
    }
    let usage = parse_usage(response)?;
    Ok((FinishReason::new(reason, raw), usage))
}

pub fn parse_tool_result(output: r::ResponseFunctionCallOutput) -> Result<Block> {
    let output_content = match output.output {
        i::CustomOutput::Text(text) => ToolOutput::Json(Value::String(text)),
        i::CustomOutput::Content(parts) => ToolOutput::Blocks(
            parts
                .into_iter()
                .map(parse_output_part)
                .collect::<Result<_>>()?,
        ),
    };
    let meta = ResultMetadata {
        id: output.id,
        status: output.status,
        name_present: output.name.is_some(),
        call_id_present: output.call_id.is_some(),
        namespace: output.namespace,
        created_by: output.created_by,
        agent: output.agent,
        caller: output.caller,
        rest: output.rest,
    };
    let content = Content::ToolResult(ToolResult {
        id: output.call_id.unwrap_or_default(),
        name: output.name.unwrap_or_default(),
        output: output_content,
        is_error: false,
        provider_executed: false,
    });
    with_native(content, Vendor::Openai, meta)
}

pub(super) fn parse_output_part(part: i::InputContent) -> Result<ToolResultBlock> {
    match part {
        i::InputContent::Text(mut value) => {
            let cache = value.prompt_cache_breakpoint.take();
            let text = std::mem::take(&mut value.text);
            let native = Remainder { rest: value.rest };
            let mut block = with_native(
                ToolResultContent::Text(Text {
                    refusal: false,
                    text,
                    citations: Vec::new(),
                }),
                Vendor::Openai,
                native,
            )?;
            media::parse_responses_cache(&mut block, cache)?;
            Ok(block)
        }
        i::InputContent::Image(mut value) => {
            let file_id_null = value.file_id == Some(None);
            let image_url_null = value.image_url == Some(None);
            let source = if let Some(Some(id)) = value.file_id.take() {
                Asset::ProviderFile {
                    vendor: Vendor::Openai,
                    id,
                }
            } else if let Some(Some(url)) = value.image_url.take() {
                Asset::Url { url }
            } else {
                return Err(Error::invalid("tool image has no source"));
            };
            let detail = match value.detail {
                i::ImageDetail::Low => Detail::Low,
                i::ImageDetail::High => Detail::High,
                i::ImageDetail::Auto => Detail::Auto,
                i::ImageDetail::Original => Detail::Original,
            };
            let cache = value.prompt_cache_breakpoint.take();
            let meta = InputImageMetadata {
                file_id_null,
                image_url_null,
                rest: value.rest,
            };
            let mut block = with_native(
                ToolResultContent::Image(Image {
                    source,
                    media_type: None,
                    detail,
                }),
                Vendor::Openai,
                ImageMetadata::Input(meta),
            )?;
            media::parse_responses_cache(&mut block, cache)?;
            Ok(block)
        }
        i::InputContent::File(mut value) => {
            let file_id_null = value.file_id == Some(None);
            let mut mime = None;
            let mut raw = false;
            let source = if let Some(Some(id)) = value.file_id.take() {
                Asset::ProviderFile {
                    vendor: Vendor::Openai,
                    id,
                }
            } else if let Some(url) = value.file_url.take() {
                Asset::Url { url }
            } else if let Some(data) = value.file_data.take() {
                let payload = if let Some(header) = data.strip_prefix("data:") {
                    let (kind, data) = header
                        .split_once(";base64,")
                        .ok_or_else(|| Error::invalid("file data URI must be base64"))?;
                    mime = Some(kind.to_owned());
                    data
                } else {
                    raw = true;
                    data.as_str()
                };
                Asset::Inline {
                    bytes: base64::engine::general_purpose::STANDARD
                        .decode(payload)
                        .map_err(Error::invalid)?,
                }
            } else {
                return Err(Error::invalid("tool document has no source"));
            };
            let detail = match value.detail {
                Some(i::FileDetail::Low) => Detail::Low,
                Some(i::FileDetail::High) => Detail::High,
                _ => Detail::Auto,
            };
            let document = Document {
                source: DocumentSource::Asset { source },
                media_type: mime,
                title: value.filename.take(),
                context: None,
                citations: None,
                detail,
            };
            let cache = value.prompt_cache_breakpoint.take();
            let detail_present = value.detail.is_some();
            let rest = value.rest;
            let mut block = with_native(
                ToolResultContent::Document(document),
                Vendor::Openai,
                FileMetadata {
                    raw_base64: raw,
                    file_id_null,
                    detail_present,
                    rest,
                },
            )?;
            media::parse_responses_cache(&mut block, cache)?;
            Ok(block)
        }
    }
}
mod items;

pub(crate) use items::parse_item;
