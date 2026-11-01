use super::messages::id;
use crate::transform::generate::reasoning_details as rd;
use crate::{
    transform::{Report, TransformError},
    wire::openai::{chat as c, responses::input as r},
};

fn role(
    role: r::MessageRole,
    content: r::MessageContent,
    report: &mut Report,
) -> Result<c::ChatMessage, TransformError> {
    match role {
        r::MessageRole::User => Ok(c::ChatMessage::User(
            c::UserMessage::builder(c::UserRole::User, super::media::to_chat(content, report)?)
                .build(),
        )),
        r::MessageRole::System | r::MessageRole::Developer | r::MessageRole::Assistant => {
            let text = super::messages::text(match content {
                r::MessageContent::Text(text) => c::TextContent::Text(text),
                r::MessageContent::Parts(parts) => c::TextContent::Parts(
                    parts
                        .into_iter()
                        .map(|part| match part {
                            r::InputContent::Text(part) => Ok(super::media::to_chat_text(
                                part.text,
                                part.prompt_cache_breakpoint,
                            )),
                            r::InputContent::Image(_) | r::InputContent::File(_) => {
                                Err(TransformError::unsupported(
                                    "input.content",
                                    "non-user Chat roles cannot carry image/file parts",
                                ))
                            }
                        })
                        .filter_map(|value| crate::transform::optional(value).transpose())
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            });
            Ok(match role {
                r::MessageRole::System => c::ChatMessage::System(
                    c::SystemMessage::builder(c::SystemRole::System, text).build(),
                ),
                r::MessageRole::Developer => c::ChatMessage::Developer(
                    c::DeveloperMessage::builder(c::DeveloperRole::Developer, text).build(),
                ),
                r::MessageRole::Assistant => {
                    let mut message =
                        c::AssistantMessage::builder(c::AssistantRole::Assistant).build();
                    message.content = Some(Some(match text {
                        c::TextContent::Text(text) => c::AssistantContent::Text(text),
                        c::TextContent::Parts(parts) => c::AssistantContent::Parts(
                            parts
                                .into_iter()
                                .map(c::AssistantContentPart::Text)
                                .collect(),
                        ),
                    }));
                    c::ChatMessage::Assistant(message)
                }
                // The enclosing arm matched System | Developer | Assistant only.
                r::MessageRole::User => unreachable!(),
            })
        }
    }
}

fn tool(call_id: String, output: c::TextContent) -> Result<c::ChatMessage, TransformError> {
    Ok(c::ChatMessage::Tool(
        c::ToolMessage::builder(
            c::ToolRole::Tool,
            super::messages::text(output),
            id(call_id)?,
        )
        .build(),
    ))
}

/// The assistant message items fold into: the last message when it is still an
/// open assistant turn, otherwise a new one. Responses spreads one assistant
/// turn over separate reasoning/message/call items; Chat needs them as one
/// message so every tool call precedes its tool messages.
fn assistant<'a>(
    messages: &'a mut Vec<c::ChatMessage>,
    open: &mut bool,
) -> &'a mut c::AssistantMessage {
    if !*open || !matches!(messages.last(), Some(c::ChatMessage::Assistant(_))) {
        messages.push(c::ChatMessage::Assistant(
            c::AssistantMessage::builder(c::AssistantRole::Assistant).build(),
        ));
        *open = true;
    }
    match messages.last_mut() {
        Some(c::ChatMessage::Assistant(message)) => message,
        // Pushed above when the last message was not an assistant.
        _ => unreachable!(),
    }
}

fn append_content(message: &mut c::AssistantMessage, content: c::AssistantContent) {
    fn parts(content: c::AssistantContent) -> Vec<c::AssistantContentPart> {
        match content {
            c::AssistantContent::Text(text) => vec![c::AssistantContentPart::Text(
                c::TextPart::builder(c::TextPartType::Text, text).build(),
            )],
            c::AssistantContent::Parts(parts) => parts,
        }
    }
    message.content = Some(Some(match message.content.take().flatten() {
        None => content,
        Some(existing) => {
            let mut merged = parts(existing);
            merged.extend(parts(content));
            c::AssistantContent::Parts(merged)
        }
    }));
}

pub(super) fn to_chat(
    input: Option<r::Input>,
    report: &mut Report,
) -> Result<Vec<c::ChatMessage>, TransformError> {
    let items = match input {
        None => return Ok(Vec::new()),
        Some(r::Input::Text(text)) => {
            return Ok(vec![role(
                r::MessageRole::User,
                r::MessageContent::Text(text),
                report,
            )?]);
        }
        Some(r::Input::Items(items)) => items,
    };
    let mut messages = Vec::new();
    // Whether the last message is an assistant turn later assistant items join.
    let mut open = false;
    for item in items {
        match item {
            r::InputItem::Easy(message) => {
                if message.phase.flatten().is_some() {
                    report.omitted("input.phase", "Chat has no message phase field");
                }
                match role(message.role, message.content, report)? {
                    c::ChatMessage::Assistant(message) => {
                        if let Some(content) = message.content.flatten() {
                            append_content(assistant(&mut messages, &mut open), content);
                        }
                    }
                    message => {
                        messages.push(message);
                        open = false;
                    }
                }
            }
            r::InputItem::Message(message) => {
                messages.push(role(
                    match message.role {
                        r::InputMessageRole::System => r::MessageRole::System,
                        r::InputMessageRole::Developer => r::MessageRole::Developer,
                        r::InputMessageRole::User => r::MessageRole::User,
                    },
                    r::MessageContent::Parts(message.content),
                    report,
                )?);
                open = false;
            }
            r::InputItem::OutputMessage(message) => {
                let mut parts = Vec::new();
                for part in message.content {
                    match part {
                        r::OutputContent::Text(part) => {
                            if !part.annotations.is_empty() || !part.logprobs.is_empty() {
                                report.omitted(
                                    "input.output_text.annotations/logprobs",
                                    "Chat assistant history has no such fields",
                                );
                            }
                            parts.push(c::AssistantContentPart::Text(
                                c::TextPart::builder(c::TextPartType::Text, part.text).build(),
                            ));
                        }
                        r::OutputContent::Refusal(part) => {
                            parts.push(c::AssistantContentPart::Refusal(
                                c::RefusalPart::builder(c::RefusalType::Refusal, part.refusal)
                                    .build(),
                            ))
                        }
                    }
                }
                append_content(
                    assistant(&mut messages, &mut open),
                    c::AssistantContent::Parts(parts),
                );
            }
            r::InputItem::FunctionCall(call) => {
                if call.namespace.is_some()
                    || call
                        .caller
                        .flatten()
                        .is_some_and(|caller| matches!(caller, r::Caller::Program(_)))
                {
                    continue;
                }
                let call = c::MessageToolCall::Function(
                    c::ChatToolCall::builder(
                        id(call.call_id)?,
                        c::FunctionCall::builder(call.arguments, call.name).build(),
                        c::ChatToolCallType::Function,
                    )
                    .build(),
                );
                assistant(&mut messages, &mut open)
                    .tool_calls
                    .get_or_insert_default()
                    .push(call);
            }
            r::InputItem::CustomToolCall(call) => {
                if call.namespace.is_some()
                    || call
                        .caller
                        .flatten()
                        .is_some_and(|caller| matches!(caller, r::Caller::Program(_)))
                {
                    continue;
                }
                let call = c::MessageToolCall::Custom(
                    c::CustomToolCall::builder(
                        id(call.call_id)?,
                        c::CustomCall::builder(call.input, call.name).build(),
                        c::CustomToolCallType::Custom,
                    )
                    .build(),
                );
                assistant(&mut messages, &mut open)
                    .tool_calls
                    .get_or_insert_default()
                    .push(call);
            }
            r::InputItem::FunctionCallOutput(output) => {
                let call_id = output
                    .call_id
                    .flatten()
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| {
                        TransformError::missing_metadata("function_call_output.call_id")
                    })?;
                if output.namespace.flatten().is_some()
                    || output
                        .caller
                        .flatten()
                        .is_some_and(|caller| matches!(caller, r::Caller::Program(_)))
                {
                    continue;
                }
                let value = match output.output {
                    r::FunctionOutput::Text(text) => c::TextContent::Text(text),
                    r::FunctionOutput::Content(parts) => c::TextContent::Parts(
                        parts
                            .into_iter()
                            .map(|part| match part {
                                r::FunctionOutputContent::Text(part) => {
                                    Ok(super::media::to_chat_text(
                                        part.text,
                                        part.prompt_cache_breakpoint.flatten(),
                                    ))
                                }
                                r::FunctionOutputContent::Image(_)
                                | r::FunctionOutputContent::File(_) => {
                                    Err(TransformError::unsupported(
                                        "tool.output",
                                        "Chat tool messages cannot carry images/files",
                                    ))
                                }
                            })
                            .filter_map(|value| crate::transform::optional(value).transpose())
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                };
                messages.push(tool(call_id, value)?);
                open = false;
            }
            r::InputItem::CustomToolCallOutput(output) => {
                if output
                    .caller
                    .flatten()
                    .is_some_and(|caller| matches!(caller, r::Caller::Program(_)))
                {
                    continue;
                }
                let value = match output.output {
                    r::CustomOutput::Text(text) => c::TextContent::Text(text),
                    r::CustomOutput::Content(parts) => c::TextContent::Parts(
                        parts
                            .into_iter()
                            .map(|part| match part {
                                r::InputContent::Text(part) => Ok(super::media::to_chat_text(
                                    part.text,
                                    part.prompt_cache_breakpoint,
                                )),
                                r::InputContent::Image(_) | r::InputContent::File(_) => {
                                    Err(TransformError::unsupported(
                                        "tool.output",
                                        "Chat tool messages cannot carry images/files",
                                    ))
                                }
                            })
                            .filter_map(|value| crate::transform::optional(value).transpose())
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                };
                messages.push(tool(output.call_id, value)?);
                open = false;
            }
            r::InputItem::ItemReference(_) => {
                return Err(TransformError::missing_metadata(
                    "input.item_reference history",
                ));
            }
            r::InputItem::Reasoning(item) => {
                let index = if open {
                    match messages.last() {
                        Some(c::ChatMessage::Assistant(message)) => message
                            .reasoning_details
                            .as_ref()
                            .and_then(Option::as_ref)
                            .map_or(0, Vec::len)
                            as i64,
                        _ => 0,
                    }
                } else {
                    0
                };
                let details = rd::from_responses(&item, index);
                let text = item
                    .summary
                    .iter()
                    .map(|p| p.text.as_str())
                    .chain(item.content.iter().flatten().map(|p| p.text.as_str()))
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() || !details.is_empty() {
                    let message = assistant(&mut messages, &mut open);
                    if !text.is_empty() {
                        match message.reasoning_content.get_or_insert(None) {
                            Some(existing) => {
                                existing.push('\n');
                                existing.push_str(&text);
                            }
                            slot => *slot = Some(text),
                        }
                    }
                    if !details.is_empty() {
                        message
                            .reasoning_details
                            .get_or_insert(None)
                            .get_or_insert_default()
                            .extend(details);
                    }
                }
            }
            r::InputItem::MultiAgentCall(_)
            | r::InputItem::MultiAgentCallOutput(_)
            | r::InputItem::AgentMessage(_)
            | r::InputItem::ConfigurationUpdate(_)
            | r::InputItem::Compaction(_)
            | r::InputItem::ComputerCall(_)
            | r::InputItem::ComputerCallOutput(_)
            | r::InputItem::WebSearchCall(_)
            | r::InputItem::FileSearchCall(_)
            | r::InputItem::ImageGenerationCall(_)
            | r::InputItem::CodeInterpreterCall(_)
            | r::InputItem::ToolSearchCall(_)
            | r::InputItem::ToolSearchOutput(_)
            | r::InputItem::AdditionalTools(_)
            | r::InputItem::LocalShellCall(_)
            | r::InputItem::LocalShellCallOutput(_)
            | r::InputItem::ShellCall(_)
            | r::InputItem::ShellCallOutput(_)
            | r::InputItem::ApplyPatchCall(_)
            | r::InputItem::ApplyPatchCallOutput(_)
            | r::InputItem::McpListTools(_)
            | r::InputItem::McpApprovalRequest(_)
            | r::InputItem::McpApprovalResponse(_)
            | r::InputItem::McpCall(_)
            | r::InputItem::CompactionTrigger(_)
            | r::InputItem::Program(_)
            | r::InputItem::ProgramOutput(_) => {
                continue;
            }
        }
    }
    Ok(messages)
}
