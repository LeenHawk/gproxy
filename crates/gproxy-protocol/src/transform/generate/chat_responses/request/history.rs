use super::messages::id;
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
            let text = match content {
                r::MessageContent::Text(text) => text,
                r::MessageContent::Parts(parts) => parts
                    .into_iter()
                    .map(|part| match part {
                        r::InputContent::Text(part) => Ok(part.text),
                        r::InputContent::Image(_) | r::InputContent::File(_) => {
                            Err(TransformError::unsupported(
                                "input.content",
                                "non-user Chat roles cannot carry image/file parts",
                            ))
                        }
                    })
                    .filter_map(|value| crate::transform::optional(value).transpose())
                    .collect::<Result<Vec<_>, _>>()?
                    .join(""),
            };
            Ok(match role {
                r::MessageRole::System => c::ChatMessage::System(
                    c::SystemMessage::builder(c::SystemRole::System, c::TextContent::Text(text))
                        .build(),
                ),
                r::MessageRole::Developer => c::ChatMessage::Developer(
                    c::DeveloperMessage::builder(
                        c::DeveloperRole::Developer,
                        c::TextContent::Text(text),
                    )
                    .build(),
                ),
                r::MessageRole::Assistant => {
                    let mut message =
                        c::AssistantMessage::builder(c::AssistantRole::Assistant).build();
                    message.content = Some(Some(c::AssistantContent::Text(text)));
                    c::ChatMessage::Assistant(message)
                }
                r::MessageRole::User => unreachable!(),
            })
        }
    }
}
fn tool(call_id: String, output: String) -> Result<c::ChatMessage, TransformError> {
    Ok(c::ChatMessage::Tool(
        c::ToolMessage::builder(
            c::ToolRole::Tool,
            c::TextContent::Text(output),
            id(call_id)?,
        )
        .build(),
    ))
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
    for item in items {
        match item {
            r::InputItem::Easy(message) => {
                if message.phase.flatten().is_some() {
                    report.omitted("input.phase", "Chat has no message phase field");
                }
                messages.push(role(message.role, message.content, report)?);
            }
            r::InputItem::Message(message) => messages.push(role(
                match message.role {
                    r::InputMessageRole::System => r::MessageRole::System,
                    r::InputMessageRole::Developer => r::MessageRole::Developer,
                    r::InputMessageRole::User => r::MessageRole::User,
                },
                r::MessageContent::Parts(message.content),
                report,
            )?),
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
                let mut output = c::AssistantMessage::builder(c::AssistantRole::Assistant).build();
                output.content = Some(Some(c::AssistantContent::Parts(parts)));
                messages.push(c::ChatMessage::Assistant(output));
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
                let mut message = c::AssistantMessage::builder(c::AssistantRole::Assistant).build();
                message.tool_calls = Some(vec![c::MessageToolCall::Function(
                    c::ChatToolCall::builder(
                        id(call.call_id)?,
                        c::FunctionCall::builder(call.arguments, call.name).build(),
                        c::ChatToolCallType::Function,
                    )
                    .build(),
                )]);
                messages.push(c::ChatMessage::Assistant(message));
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
                let mut message = c::AssistantMessage::builder(c::AssistantRole::Assistant).build();
                message.tool_calls = Some(vec![c::MessageToolCall::Custom(
                    c::CustomToolCall::builder(
                        id(call.call_id)?,
                        c::CustomCall::builder(call.input, call.name).build(),
                        c::CustomToolCallType::Custom,
                    )
                    .build(),
                )]);
                messages.push(c::ChatMessage::Assistant(message));
            }
            r::InputItem::FunctionCallOutput(output) => {
                if output.namespace.flatten().is_some()
                    || output
                        .caller
                        .flatten()
                        .is_some_and(|caller| matches!(caller, r::Caller::Program(_)))
                {
                    continue;
                }
                let value = match output.output {
                    r::FunctionOutput::Text(text) => text,
                    r::FunctionOutput::Content(parts) => parts
                        .into_iter()
                        .map(|part| match part {
                            r::FunctionOutputContent::Text(part) => Ok(part.text),
                            r::FunctionOutputContent::Image(_)
                            | r::FunctionOutputContent::File(_) => {
                                Err(TransformError::unsupported(
                                    "tool.output",
                                    "Chat tool messages cannot carry images/files",
                                ))
                            }
                        })
                        .filter_map(|value| crate::transform::optional(value).transpose())
                        .collect::<Result<Vec<_>, _>>()?
                        .join(""),
                };
                messages.push(tool(output.call_id, value)?);
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
                    r::CustomOutput::Text(text) => text,
                    r::CustomOutput::Content(parts) => parts
                        .into_iter()
                        .map(|part| match part {
                            r::InputContent::Text(part) => Ok(part.text),
                            r::InputContent::Image(_) | r::InputContent::File(_) => {
                                Err(TransformError::unsupported(
                                    "tool.output",
                                    "Chat tool messages cannot carry images/files",
                                ))
                            }
                        })
                        .filter_map(|value| crate::transform::optional(value).transpose())
                        .collect::<Result<Vec<_>, _>>()?
                        .join(""),
                };
                messages.push(tool(output.call_id, value)?);
            }
            r::InputItem::ItemReference(_) => {
                return Err(TransformError::missing_metadata(
                    "input.item_reference history",
                ));
            }
            r::InputItem::Reasoning(_)
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
