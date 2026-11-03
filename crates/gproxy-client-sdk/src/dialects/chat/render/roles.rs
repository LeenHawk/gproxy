use super::*;
pub(super) fn render_assistant(
    message: &Message,
    now: u64,
    messages: &mut Vec<c::ChatMessage>,
) -> Result<()> {
    let mut value = c::AssistantMessage::builder(c::AssistantRole::Assistant).build();
    // Apply the message envelope once before portable blocks so moving
    // its owning block cannot overwrite already rendered content.
    for block in message.model_content() {
        if let Some(native) = metadata(block)?.and_then(|m| m.message) {
            value.rest.extend(native.rest);
            value.reasoning_details = native.reasoning_details;
            value.reasoning_content = native.reasoning_content;
            value.reasoning = native.reasoning;
            value.content = Some(None);
            value.refusal = Some(None);
        }
    }
    let mut text = Vec::new();
    let mut parts = false;
    for content in message.model_content() {
        let meta = metadata(content)?.unwrap_or_default();
        match &content.content {
            Content::Text(t) => {
                if t.refusal {
                    value.refusal = Some(Some(t.text.clone()));
                } else {
                    let mut part =
                        c::TextPart::builder(c::TextPartType::Text, t.text.clone()).build();
                    part.prompt_cache_breakpoint = media::chat_cache(content)?;
                    parts |= content.cache.is_some();
                    text.push(c::AssistantContentPart::Text(part));
                }
            }
            Content::Reasoning(reasoning) => match meta.reasoning_source {
                Some(ReasoningSource::Reasoning) => {
                    value
                        .reasoning
                        .get_or_insert_with(|| Some(String::new()))
                        .get_or_insert_with(String::new)
                        .push_str(&reasoning.text);
                }
                Some(ReasoningSource::Details) => {
                    render_reasoning_details(&mut value, &reasoning.text, &meta.reasoning_layout)?
                }
                Some(ReasoningSource::Content) => {
                    value
                        .reasoning_content
                        .get_or_insert_with(|| Some(String::new()))
                        .get_or_insert_with(String::new)
                        .push_str(&reasoning.text);
                }
                // Another vendor's reasoning is not replayed (warned).
                None => {}
            },
            Content::Audio(audio) => {
                if let Some(native) = meta.audio
                    && let (Some(expiry), Some(id)) = (native.expires_at, native.id)
                    && u64::try_from(expiry).is_ok_and(|expiry| expiry > now)
                {
                    value.audio = Some(Some(
                        c::AssistantAudio::builder(id).rest(native.rest).build(),
                    ));
                    continue;
                }
                let transcript=audio.transcript.as_ref().ok_or_else(||Error::invalid("expired or foreign assistant audio requires a transcript for Chat continuation"))?;
                text.push(c::AssistantContentPart::Text(
                    c::TextPart::builder(c::TextPartType::Text, transcript.clone()).build(),
                ));
                value.audio = None;
            }
            Content::ToolResult(result) if result.provider_executed => {}
            Content::ToolCall(call) => {
                if call.provider_executed {
                    continue;
                }
                let function = c::FunctionCall::builder(call.arguments.clone(), call.name.clone())
                    .rest(meta.function_rest)
                    .build();
                if meta.legacy {
                    value.function_call = Some(Some(function));
                } else {
                    let mut tool = c::ChatToolCall::builder(
                        call.id.clone(),
                        function,
                        c::ChatToolCallType::Function,
                    )
                    .rest(meta.rest)
                    .build();
                    tool.type_ = meta.call_type.unwrap_or(c::ChatToolCallType::Function);
                    value
                        .tool_calls
                        .get_or_insert_with(Vec::new)
                        .push(c::MessageToolCall::Function(tool));
                }
            }
            Content::Unknown(_) => {}
            Content::Opaque => {
                #[derive(Deserialize)]
                struct Custom {
                    custom: Option<c::CustomToolCall>,
                }
                if let Some(custom) =
                    read_metadata::<Custom>(content, Vendor::Openai)?.and_then(|v| v.custom)
                {
                    value
                        .tool_calls
                        .get_or_insert_with(Vec::new)
                        .push(c::MessageToolCall::Custom(custom));
                }
            }
            _ => {
                return Err(Error::invalid(
                    "Chat assistant history supports text, reasoning, tool calls and audio references",
                ));
            }
        }
    }
    if !text.is_empty() {
        value.content = Some(Some(if parts {
            c::AssistantContent::Parts(text)
        } else {
            c::AssistantContent::Text(
                text.into_iter()
                    .filter_map(|p| match p {
                        c::AssistantContentPart::Text(t) => Some(t.text),
                        _ => None,
                    })
                    .collect(),
            )
        }));
    }
    messages.push(c::ChatMessage::Assistant(value));

    Ok(())
}
fn push_tool_result(result: &ToolResult, messages: &mut Vec<c::ChatMessage>) -> Result<()> {
    if !result.provider_executed {
        let output = tool_output(result)?;
        messages.push(c::ChatMessage::Tool(
            c::ToolMessage::builder(c::ToolRole::Tool, output, result.id.clone()).build(),
        ));
    }
    Ok(())
}
pub(super) fn render_tool(message: &Message, messages: &mut Vec<c::ChatMessage>) -> Result<()> {
    for content in message.model_content() {
        if let Content::ToolResult(result) = &content.content {
            push_tool_result(result, messages)?;
        } else {
            return Err(Error::invalid(
                "Chat tool messages require tool-result blocks",
            ));
        }
    }

    Ok(())
}
pub(super) fn render_input(message: &Message, messages: &mut Vec<c::ChatMessage>) -> Result<()> {
    let mut parts = Vec::new();
    for content in message.model_content() {
        // Claude-style history carries tool results in user turns; Chat sends
        // them as tool messages, ahead of the turn's remaining user content.
        if message.role == Role::User
            && let Content::ToolResult(result) = &content.content
        {
            push_tool_result(result, messages)?;
            continue;
        }
        parts.push(user_part(content)?);
    }
    if parts.is_empty() && message.role == Role::User {
        return Ok(());
    }
    if message.role == Role::System {
        let text = parts
            .into_iter()
            .map(|p| match p {
                c::UserContentPart::Text(t) => Ok(t),
                _ => Err(Error::invalid("Chat system messages require text")),
            })
            .collect::<Result<_>>()?;
        messages.push(c::ChatMessage::System(
            c::SystemMessage::builder(c::SystemRole::System, c::TextContent::Parts(text)).build(),
        ));
    } else {
        messages.push(c::ChatMessage::User(
            c::UserMessage::builder(c::UserRole::User, c::UserContent::Parts(parts)).build(),
        ));
    }

    Ok(())
}
