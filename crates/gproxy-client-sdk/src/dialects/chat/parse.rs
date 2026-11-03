use super::*;
pub fn parse(
    response: r::GenerateContentResponseBody,
    audio_format: Option<AudioFormat>,
) -> Result<GenerateResult> {
    let warnings = if response.choices.len() > 1 {
        vec![Warning::new(
            WarningKind::Compatibility,
            "response.choices",
            "Only the first choice is returned",
        )]
    } else {
        Vec::new()
    };
    let response_metadata = ResponseMetadata {
        id: Some(response.id.clone()),
        model: Some(response.model.clone()),
        timestamp: Some(response.created),
    };
    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| Error::invalid("Chat response has no choice"))?;
    let finish_reason = finish_reason(choice.finish_reason);
    let content = parse_message(choice.message, audio_format)?
        .into_iter()
        .map(|(_, block)| block)
        .collect();
    Ok(GenerateResult {
        response: response_metadata,
        warnings,
        sources: Vec::new(),
        content,
        finish_reason,
        usage: parse_usage(response.usage.as_ref())?,
    })
}

pub(crate) fn parse_message(
    message: r::ResponseMessage,
    format: Option<AudioFormat>,
) -> Result<Vec<(u32, Block)>> {
    parse_message_inner(message, format, None)
}

pub(super) fn parse_message_inner(
    mut message: r::ResponseMessage,
    format: Option<AudioFormat>,
    supplied_audio: Option<Block>,
) -> Result<Vec<(u32, Block)>> {
    let mut result = Vec::new();
    parse_reasoning(&mut message, &mut result)?;
    let annotations = message
        .annotations
        .take()
        .unwrap_or_default()
        .into_iter()
        .map(citations::parse_chat)
        .collect::<Result<_>>()?;
    if let Some(text) = message.content.take() {
        result.push((
            1,
            block(
                Content::Text(Text {
                    refusal: false,
                    text,
                    citations: annotations,
                }),
                Metadata::default(),
            )?,
        ));
    }
    if let Some(refusal) = message.refusal.take() {
        result.push((
            2,
            block(
                Content::Text(Text {
                    refusal: true,
                    text: refusal,
                    citations: Vec::new(),
                }),
                Metadata {
                    ..Default::default()
                },
            )?,
        ));
    }
    if let Some(Some(mut audio)) = message.audio.take() {
        let source = Asset::Inline {
            bytes: base64::engine::general_purpose::STANDARD
                .decode(&audio.data)
                .map_err(Error::invalid)?,
        };
        let transcript = Some(std::mem::take(&mut audio.transcript));
        audio.data.clear();
        let native = AudioMetadata {
            id: Some(audio.id),
            expires_at: Some(audio.expires_at),
            rest: audio.rest,
        };
        result.push((
            3,
            block(
                Content::Audio(Audio {
                    source,
                    format: format.ok_or_else(|| {
                        Error::invalid("Chat audio decoding requires the requested audio format")
                    })?,
                    transcript,
                }),
                Metadata {
                    audio: Some(native),
                    ..Default::default()
                },
            )?,
        ));
    }
    if let Some(audio) = supplied_audio {
        result.push((3, audio));
    }
    for (index, call) in message
        .tool_calls
        .take()
        .unwrap_or_default()
        .into_iter()
        .enumerate()
    {
        match call {
            c::MessageToolCall::Function(value) => {
                let content = Content::ToolCall(ToolCall {
                    id: value.id,
                    name: value.function.name,
                    arguments: value.function.arguments,
                    provider_executed: false,
                });
                result.push((
                    4 + index as u32,
                    block(
                        content,
                        Metadata {
                            call_type: Some(value.type_),
                            rest: value.rest,
                            function_rest: value.function.rest,
                            ..Default::default()
                        },
                    )?,
                ));
            }
            c::MessageToolCall::Custom(value) => {
                #[derive(Serialize)]
                struct Custom {
                    custom: c::CustomToolCall,
                }
                result.push((
                    4 + index as u32,
                    with_metadata(Content::Opaque, Vendor::Openai, Custom { custom: value })?,
                ));
            }
        }
    }
    if let Some(value) = message.function_call.take() {
        result.push((
            4,
            block(
                Content::ToolCall(ToolCall {
                    id: "legacy_function".into(),
                    name: value.name,
                    arguments: value.arguments,
                    provider_executed: false,
                }),
                Metadata {
                    legacy: true,
                    function_rest: value.rest,
                    ..Default::default()
                },
            )?,
        ));
    }
    let native = MessageMetadata {
        reasoning_details: message.reasoning_details,
        reasoning_content: message.reasoning_content,
        reasoning: message.reasoning,
        rest: message.rest,
    };
    if let Some((_, first)) = result.first_mut() {
        let mut meta = metadata(first)?.unwrap_or_default();
        meta.message = Some(native);
        first
            .provider_metadata
            .entry(Vendor::Openai)
            .or_default()
            .extend(encode_rest(Envelope { chat: Some(meta) })?);
    }
    Ok(result)
}

pub(crate) fn finish_reason(reason: r::FinishReason) -> FinishReason {
    let (kind, raw) = match reason {
        r::FinishReason::Stop => (FinishReasonKind::Stop, "stop"),
        r::FinishReason::Length => (FinishReasonKind::Length, "length"),
        r::FinishReason::ToolCalls => (FinishReasonKind::ToolCalls, "tool_calls"),
        r::FinishReason::FunctionCall => (FinishReasonKind::ToolCalls, "function_call"),
        r::FinishReason::ContentFilter => (FinishReasonKind::ContentBlocked, "content_filter"),
    };
    FinishReason::new(kind, Some(raw.into()))
}

pub(crate) fn parse_usage(usage: Option<&r::Usage>) -> Result<Usage> {
    let Some(v) = usage else {
        return Ok(Usage::default());
    };
    Ok(Usage {
        input_tokens: Some(count(v.prompt_tokens)?),
        output_tokens: Some(count(v.completion_tokens)?),
        reasoning_tokens: optional_count_chat(
            v.completion_tokens_details
                .as_ref()
                .and_then(|v| v.reasoning_tokens),
        )?,
        cache_read_tokens: optional_count_chat(
            v.prompt_tokens_details
                .as_ref()
                .and_then(|v| v.cached_tokens),
        )?,
        cache_write_tokens: optional_count_chat(
            v.prompt_tokens_details
                .as_ref()
                .and_then(|v| v.cache_write_tokens),
        )?,
    })
}

pub(super) fn optional_count_chat(value: Option<i64>) -> Result<Option<u64>> {
    value.map(count).transpose()
}

fn parse_reasoning(message: &mut r::ResponseMessage, result: &mut Vec<(u32, Block)>) -> Result<()> {
    let visible = gproxy_protocol::openai::chat::visible_reasoning(
        &message.reasoning_content,
        &message.reasoning,
        &message.reasoning_details,
    );
    if let Some(text) = visible.filter(|s| !s.is_empty()) {
        let source = if message
            .reasoning_content
            .as_ref()
            .and_then(Option::as_ref)
            .is_some_and(|s| !s.is_empty())
        {
            message.reasoning_content = Some(Some(String::new()));
            ReasoningSource::Content
        } else if message
            .reasoning
            .as_ref()
            .and_then(Option::as_ref)
            .is_some_and(|s| !s.is_empty())
        {
            message.reasoning = Some(Some(String::new()));
            ReasoningSource::Reasoning
        } else {
            ReasoningSource::Details
        };
        let mut layout = Vec::new();
        if matches!(source, ReasoningSource::Details) {
            use gproxy_protocol::openai::chat::reasoning::ReasoningDetailKind as K;
            for (index, detail) in message
                .reasoning_details
                .iter_mut()
                .flatten()
                .flatten()
                .enumerate()
            {
                let value = match detail.type_ {
                    K::Text => detail.text.as_mut().and_then(Option::as_mut),
                    K::Summary => detail.summary.as_mut(),
                    K::Encrypted => None,
                };
                if let Some(value) = value {
                    layout.push(ReasoningSegment {
                        index,
                        chars: value.chars().count(),
                    });
                    value.clear();
                }
            }
        }
        result.push((
            0,
            block(
                Content::Reasoning(Reasoning { text }),
                Metadata {
                    reasoning_source: Some(source),
                    reasoning_layout: layout,
                    ..Default::default()
                },
            )?,
        ));
    }

    Ok(())
}
