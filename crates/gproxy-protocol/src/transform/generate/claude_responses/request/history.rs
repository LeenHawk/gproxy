use crate::{
    transform::{Report, TransformError},
    wire::{claude::content as c, openai::responses::input as r},
};
fn text(value: String) -> c::ContentBlock {
    c::ContentBlock::Text(c::TextBlock::builder(c::TextBlockType::Tag, value).build())
}
pub(crate) fn to_claude(
    input: Option<r::Input>,
    mut context: super::ClaudeRequestContext,
    report: &mut Report,
) -> Result<(Vec<c::Message>, Vec<c::TextBlock>), TransformError> {
    let items = match input {
        None => Vec::new(),
        Some(r::Input::Text(text)) => vec![r::InputItem::Easy(
            r::EasyInputMessage::builder(r::MessageContent::Text(text), r::MessageRole::User)
                .build(),
        )],
        Some(r::Input::Items(items)) => items,
    };
    let mut messages = Vec::new();
    let mut system = Vec::new();
    let mut position = crate::transform::instructions::Position::default();
    for item in items {
        let (role, blocks) = match item {
            r::InputItem::Easy(message) => {
                let parts = match message.content {
                    r::MessageContent::Text(value) => vec![text(value)],
                    r::MessageContent::Parts(parts) => parts
                        .into_iter()
                        .map(super::media::to_claude)
                        .collect::<Result<Vec<_>, _>>()?,
                };
                (message.role, parts)
            }
            r::InputItem::Message(message) => (
                match message.role {
                    r::InputMessageRole::User => r::MessageRole::User,
                    r::InputMessageRole::System => r::MessageRole::System,
                    r::InputMessageRole::Developer => r::MessageRole::Developer,
                },
                message
                    .content
                    .into_iter()
                    .map(super::media::to_claude)
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            r::InputItem::OutputMessage(message) => {
                let mut blocks = Vec::new();
                for part in message.content {
                    match part {
                        r::OutputContent::Text(part) => {
                            if !part.annotations.is_empty() || !part.logprobs.is_empty() {
                                report.omitted(
                                    "output_history.metadata",
                                    "Claude history lacks matching annotations/logprobs",
                                );
                            }
                            blocks.push(text(part.text));
                        }
                        r::OutputContent::Refusal(part) => {
                            blocks.push(text(part.refusal));
                            report.changed(
                                "refusal",
                                "Claude history represents refusal as assistant text",
                            );
                        }
                    }
                }
                (r::MessageRole::Assistant, blocks)
            }
            r::InputItem::FunctionCall(call) => {
                if call.call_id.is_empty()
                    || call.namespace.is_some()
                    || call
                        .caller
                        .flatten()
                        .is_some_and(|v| matches!(v, r::Caller::Program(_)))
                {
                    return Err(TransformError::unsupported(
                        "function_call",
                        "empty/scoped call needs host identity adapter",
                    ));
                }
                let input: serde_json::Map<String, serde_json::Value> =
                    serde_json::from_str(&call.arguments).map_err(|error| {
                        TransformError::shape("function_call.arguments", error.to_string())
                    })?;
                (
                    r::MessageRole::Assistant,
                    vec![c::ContentBlock::ToolUse(
                        c::ToolUseBlock::builder(
                            c::ToolUseBlockType::Tag,
                            call.call_id,
                            serde_json::Value::Object(input),
                            call.name,
                        )
                        .build(),
                    )],
                )
            }
            r::InputItem::FunctionCallOutput(output) => {
                if output.call_id.is_empty()
                    || output.namespace.flatten().is_some()
                    || output
                        .caller
                        .flatten()
                        .is_some_and(|v| matches!(v, r::Caller::Program(_)))
                {
                    return Err(TransformError::unsupported(
                        "function_call_output",
                        "empty/scoped call result needs host identity adapter",
                    ));
                }
                let result = super::media::result_to_claude(output.output)?;
                (
                    r::MessageRole::User,
                    vec![c::ContentBlock::ToolResult(
                        c::ToolResultBlock::builder(c::ToolResultBlockType::Tag, output.call_id)
                            .content(result)
                            .build(),
                    )],
                )
            }
            r::InputItem::McpCall(call) => {
                if call.id.is_empty() || call.approval_request_id.flatten().is_some() {
                    return Err(TransformError::unsupported(
                        "mcp_call",
                        "empty identity or approval state needs host binding",
                    ));
                }
                let input: serde_json::Value = serde_json::from_str(&call.arguments)
                    .map_err(|e| TransformError::shape("mcp.arguments", e.to_string()))?;
                if !input.is_object() {
                    return Err(TransformError::shape(
                        "mcp.arguments",
                        "object input required",
                    ));
                }
                let mut blocks = vec![c::ContentBlock::McpToolUse(
                    c::McpToolUseBlock::builder(
                        c::McpToolUseType::Tag,
                        call.id.clone(),
                        input,
                        call.name,
                        call.server_label,
                    )
                    .build(),
                )];
                if call.error.as_ref().and_then(Option::as_ref).is_some()
                    && call.output.as_ref().and_then(Option::as_ref).is_some()
                {
                    return Err(TransformError::shape(
                        "mcp_call",
                        "error and output conflict",
                    ));
                }
                let is_error = call.error.as_ref().and_then(Option::as_ref).is_some();
                let text = call.error.flatten().or(call.output.flatten());
                if let Some(text) = text {
                    blocks.push(c::ContentBlock::McpToolResult(
                        c::McpToolResultBlock::builder(c::McpToolResultType::Tag, call.id)
                            .content(c::McpResultContent::Text(text))
                            .is_error(is_error)
                            .build(),
                    ));
                }
                (r::MessageRole::Assistant, blocks)
            }
            r::InputItem::Reasoning(reasoning) => {
                let model = context
                    .target
                    .as_ref()
                    .map(|target| target.model.clone())
                    .ok_or_else(|| TransformError::missing_metadata("reasoning target"))?;
                (
                    r::MessageRole::Assistant,
                    vec![c::ContentBlock::Thinking(super::restore_reasoning(
                        reasoning,
                        &model,
                        &mut context,
                    )?)],
                )
            }
            r::InputItem::ItemReference(_) => {
                return Err(TransformError::missing_metadata(
                    "item_reference resolved history",
                ));
            }
            r::InputItem::Compaction(_)
            | r::InputItem::ComputerCall(_)
            | r::InputItem::ComputerCallOutput(_)
            | r::InputItem::WebSearchCall(_)
            | r::InputItem::FileSearchCall(_)
            | r::InputItem::ImageGenerationCall(_)
            | r::InputItem::CodeInterpreterCall(_)
            | r::InputItem::CustomToolCall(_)
            | r::InputItem::CustomToolCallOutput(_)
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
            | r::InputItem::CompactionTrigger(_)
            | r::InputItem::Program(_)
            | r::InputItem::ProgramOutput(_) => {
                return Err(TransformError::unsupported(
                    "input.item",
                    "Responses hosted/custom/opaque execution needs native adapter",
                ));
            }
        };
        let leading = position.leading(matches!(
            role,
            r::MessageRole::System | r::MessageRole::Developer
        ));
        match role {
            r::MessageRole::System | r::MessageRole::Developer => {
                if blocks
                    .iter()
                    .any(|block| !matches!(block, c::ContentBlock::Text(_)))
                {
                    return Err(TransformError::unsupported(
                        "system",
                        "Claude system prompt is text only",
                    ));
                }
                if leading {
                    system.extend(blocks.into_iter().filter_map(|block| match block {
                        c::ContentBlock::Text(text) => Some(text),
                        _ => None,
                    }));
                } else {
                    messages.push(
                        c::Message::builder(c::Role::System, c::MessageContent::Blocks(blocks))
                            .build(),
                    );
                }
            }
            r::MessageRole::User => messages.push(
                c::Message::builder(c::Role::User, c::MessageContent::Blocks(blocks)).build(),
            ),
            r::MessageRole::Assistant => messages.push(
                c::Message::builder(c::Role::Assistant, c::MessageContent::Blocks(blocks)).build(),
            ),
        }
    }
    Ok((messages, system))
}
