use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::{items, results, shape};
use crate::common::responses;

use super::message;

pub(super) fn typed_item(
    item: openai::TypedResponseItem,
) -> Result<claude::MessageParam, TransformError> {
    match item {
        openai::TypedResponseItem::FunctionCall {
            arguments,
            call_id,
            name,
            ..
        } => Ok(message(
            claude::MessageRoleKnown::Assistant,
            vec![tool_use(call_id, name, arguments)?],
        )),
        openai::TypedResponseItem::CustomToolCall {
            call_id,
            input,
            name,
            ..
        } => Ok(message(
            claude::MessageRoleKnown::Assistant,
            vec![tool_use(call_id, name, input)?],
        )),
        openai::TypedResponseItem::FunctionCallOutput {
            call_id, output, ..
        }
        | openai::TypedResponseItem::CustomToolCallOutput {
            call_id, output, ..
        } => Ok(message(
            claude::MessageRoleKnown::User,
            vec![claude::ContentBlockParam::ToolResult(
                claude::ToolResultBlock {
                    tool_use_id: call_id,
                    type_: claude::ToolResultBlockType::ToolResult,
                    cache_control: None,
                    content: Some(response_output(output)?),
                    is_error: None,
                    rest: Default::default(),
                },
            )],
        )),
        openai::TypedResponseItem::Reasoning {
            content,
            encrypted_content,
            ..
        } => {
            let thinking = content
                .into_iter()
                .flatten()
                .map(|part| part.text)
                .collect::<String>();
            Ok(message(
                claude::MessageRoleKnown::Assistant,
                vec![claude::ContentBlockParam::Thinking(claude::ThinkingBlock {
                    signature: encrypted_content,
                    thinking,
                    type_: claude::ThinkingBlockType::Thinking,
                    rest: Default::default(),
                })],
            ))
        }
        openai::TypedResponseItem::Compaction {
            encrypted_content, ..
        } => Ok(message(
            claude::MessageRoleKnown::Assistant,
            vec![claude::ContentBlockParam::Compaction(
                claude::CompactionBlock {
                    content: None,
                    encrypted_content: Some(encrypted_content),
                    type_: claude::CompactionBlockType::Compaction,
                    cache_control: None,
                    rest: Default::default(),
                },
            )],
        )),
        other @ (openai::TypedResponseItem::FileSearchCall { .. }
        | openai::TypedResponseItem::ComputerCall { .. }
        | openai::TypedResponseItem::ComputerCallOutput { .. }
        | openai::TypedResponseItem::WebSearchCall { .. }
        | openai::TypedResponseItem::ToolSearchCall { .. }
        | openai::TypedResponseItem::ToolSearchOutput { .. }
        | openai::TypedResponseItem::AdditionalTools { .. }
        | openai::TypedResponseItem::ImageGenerationCall { .. }
        | openai::TypedResponseItem::CodeInterpreterCall { .. }
        | openai::TypedResponseItem::LocalShellCall { .. }
        | openai::TypedResponseItem::LocalShellCallOutput { .. }
        | openai::TypedResponseItem::ShellCall { .. }
        | openai::TypedResponseItem::ShellCallOutput { .. }
        | openai::TypedResponseItem::ApplyPatchCall { .. }
        | openai::TypedResponseItem::ApplyPatchCallOutput { .. }
        | openai::TypedResponseItem::McpListTools { .. }
        | openai::TypedResponseItem::McpApprovalRequest { .. }
        | openai::TypedResponseItem::McpApprovalResponse { .. }
        | openai::TypedResponseItem::McpCall { .. }
        | openai::TypedResponseItem::Program { .. }
        | openai::TypedResponseItem::ProgramOutput { .. }
        | openai::TypedResponseItem::MultiAgentCall { .. }
        | openai::TypedResponseItem::MultiAgentCallOutput { .. }
        | openai::TypedResponseItem::AgentMessage { .. }
        | openai::TypedResponseItem::CompactionTrigger { .. }
        | openai::TypedResponseItem::ItemReference { .. }) => {
            if let Some(call) = items::openai_call(other.clone())? {
                return Ok(message(
                    claude::MessageRoleKnown::Assistant,
                    vec![items::request_block(call)],
                ));
            }
            if let Some(result) = results::openai_result(other.clone()) {
                return Ok(message(
                    claude::MessageRoleKnown::User,
                    vec![results::result_block(result)],
                ));
            }
            Err(TransformError::unsupported(
                "OpenAI Responses item",
                serde_json::to_string(&other)?,
            ))
        }
    }
}

fn tool_use(
    id: String,
    name: String,
    arguments: String,
) -> Result<claude::ContentBlockParam, TransformError> {
    Ok(claude::ContentBlockParam::ToolUse(claude::ToolUseBlock {
        id,
        input: shape::arguments_object(&arguments)?,
        name,
        type_: claude::ToolUseBlockType::ToolUse,
        cache_control: None,
        caller: None,
        rest: Default::default(),
    }))
}

fn response_output(
    output: openai::ResponseOutput,
) -> Result<claude::ToolResultContent, TransformError> {
    match output {
        openai::ResponseOutput::Text(text) => Ok(claude::ToolResultContent::Text(text)),
        openai::ResponseOutput::Parts(parts) => {
            let parts = parts.into_iter().map(tool_output_to_input).collect();
            let blocks = responses::input_to_claude(parts)?;
            let blocks = blocks
                .into_iter()
                .map(|block| serde_json::from_value(serde_json::to_value(block)?))
                .collect::<Result<_, serde_json::Error>>()?;
            Ok(claude::ToolResultContent::Blocks(blocks))
        }
    }
}

fn tool_output_to_input(
    part: openai::ResponseToolOutputContentPart,
) -> openai::ResponseInputContentPart {
    match part {
        openai::ResponseToolOutputContentPart::InputText(part) => {
            openai::ResponseInputContentPart::InputText(part)
        }
        openai::ResponseToolOutputContentPart::InputImage(part) => {
            openai::ResponseInputContentPart::InputImage(part)
        }
        openai::ResponseToolOutputContentPart::InputFile(part) => {
            openai::ResponseInputContentPart::InputFile(part)
        }
    }
}
