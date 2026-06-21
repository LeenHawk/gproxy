use crate::protocol::{claude, openai};
use crate::transform::TransformContext;

use super::DEFAULT_MODEL;
use super::tools::{
    arguments_to_json_object, code_interpreter_input, response_server_tool_use_block,
    serializable_to_json_object, shell_input, string_input_json_object,
};
use super::util::join_text;

pub fn response(
    input: openai::CompactedResponseObject,
    _: &TransformContext,
) -> claude::CreateMessageResponseBody {
    claude::CreateMessageResponseBody {
        id: input.id,
        type_: claude::MessageObjectType::Known(claude::MessageObjectTypeKnown::Message),
        role: claude::AssistantRole::Known(claude::AssistantRoleKnown::Assistant),
        content: compact_output_to_claude_content(input.output),
        model: claude::ClaudeModel::Unknown(DEFAULT_MODEL.to_owned()),
        stop_reason: claude::StopReason::Known(claude::StopReasonKnown::Compaction),
        stop_sequence: None,
        usage: openai_usage_to_claude(input.usage),
        container: None,
        context_management: None,
        diagnostics: None,
        stop_details: None,
        extra: Default::default(),
    }
}

fn compact_output_to_claude_content(
    output: Vec<openai::CompactResponseItem>,
) -> Vec<claude::ContentBlock> {
    output
        .into_iter()
        .flat_map(compact_item_to_claude_content)
        .collect()
}

fn compact_item_to_claude_content(item: openai::CompactResponseItem) -> Vec<claude::ContentBlock> {
    match item {
        openai::CompactResponseItem::Message(message) => compact_message_to_claude_content(message),
        openai::CompactResponseItem::Typed(openai::TypedResponseItem::Compaction {
            encrypted_content,
            ..
        }) => vec![claude::ContentBlock::Compaction(
            claude::ResponseCompactionBlock {
                content: None,
                encrypted_content,
                type_: claude::CompactionBlockType::Compaction,
                extra: Default::default(),
            },
        )],
        openai::CompactResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
            arguments,
            call_id,
            name,
            id,
            ..
        }) => vec![claude::ContentBlock::ToolUse(
            claude::ResponseToolUseBlock {
                id: id.unwrap_or(call_id),
                input: arguments_to_json_object(&arguments),
                name,
                type_: claude::ToolUseBlockType::ToolUse,
                caller: None,
                extra: Default::default(),
            },
        )],
        openai::CompactResponseItem::Typed(openai::TypedResponseItem::CustomToolCall {
            call_id,
            input,
            name,
            id,
            ..
        }) => vec![claude::ContentBlock::ToolUse(
            claude::ResponseToolUseBlock {
                id: id.unwrap_or(call_id),
                input: string_input_json_object(input),
                name,
                type_: claude::ToolUseBlockType::ToolUse,
                caller: None,
                extra: Default::default(),
            },
        )],
        openai::CompactResponseItem::Typed(openai::TypedResponseItem::WebSearchCall {
            id,
            action,
            ..
        }) => vec![claude::ContentBlock::ServerToolUse(
            response_server_tool_use_block(
                id,
                serializable_to_json_object(&action),
                claude::ServerToolUseNameKnown::WebSearch,
            ),
        )],
        openai::CompactResponseItem::Typed(openai::TypedResponseItem::CodeInterpreterCall {
            id,
            code,
            container_id,
            ..
        }) => vec![claude::ContentBlock::ServerToolUse(
            response_server_tool_use_block(
                id,
                code_interpreter_input(code, container_id),
                claude::ServerToolUseNameKnown::CodeExecution,
            ),
        )],
        openai::CompactResponseItem::Typed(openai::TypedResponseItem::LocalShellCall {
            action,
            call_id,
            ..
        }) => vec![claude::ContentBlock::ServerToolUse(
            response_server_tool_use_block(
                call_id,
                serializable_to_json_object(&action),
                claude::ServerToolUseNameKnown::BashCodeExecution,
            ),
        )],
        openai::CompactResponseItem::Typed(openai::TypedResponseItem::ShellCall {
            action,
            call_id,
            environment: None,
            ..
        }) => vec![claude::ContentBlock::ServerToolUse(
            response_server_tool_use_block(
                call_id,
                serializable_to_json_object(&action),
                claude::ServerToolUseNameKnown::BashCodeExecution,
            ),
        )],
        openai::CompactResponseItem::Typed(openai::TypedResponseItem::ShellCall {
            action,
            call_id,
            environment: Some(environment),
            ..
        }) => vec![claude::ContentBlock::ServerToolUse(
            response_server_tool_use_block(
                call_id,
                shell_input(action, environment),
                claude::ServerToolUseNameKnown::BashCodeExecution,
            ),
        )],
        openai::CompactResponseItem::Typed(openai::TypedResponseItem::McpCall {
            id,
            arguments,
            name,
            server_label,
            output,
            error,
            ..
        }) => {
            let mut blocks = vec![claude::ContentBlock::McpToolUse(
                claude::ResponseMcpToolUseBlock {
                    id: id.clone(),
                    input: arguments_to_json_object(&arguments),
                    name,
                    server_name: server_label,
                    type_: claude::ResponseMcpToolUseBlockType::McpToolUse,
                    extra: Default::default(),
                },
            )];
            if let Some(result) = response_mcp_result_block(id, output, error) {
                blocks.push(claude::ContentBlock::McpToolResult(result));
            }
            blocks
        }
        openai::CompactResponseItem::Typed(openai::TypedResponseItem::Reasoning {
            id,
            summary,
            content,
            encrypted_content,
            ..
        }) => reasoning_to_claude_content(id, summary, content, encrypted_content),
        _ => Vec::new(),
    }
}

fn compact_message_to_claude_content(
    message: openai::CompactMessageItem,
) -> Vec<claude::ContentBlock> {
    message
        .content
        .into_iter()
        .filter_map(compact_content_part_to_claude)
        .collect()
}

fn compact_content_part_to_claude(
    part: openai::CompactMessageContentPart,
) -> Option<claude::ContentBlock> {
    let text = match part {
        openai::CompactMessageContentPart::Input(openai::ResponseInputContentPart::InputText {
            text,
            ..
        })
        | openai::CompactMessageContentPart::Output(
            openai::ResponseOutputContentPart::OutputText { text, .. },
        )
        | openai::CompactMessageContentPart::Output(
            openai::ResponseOutputContentPart::ReasoningText { text, .. },
        )
        | openai::CompactMessageContentPart::Text(openai::CompactTextContent { text, .. })
        | openai::CompactMessageContentPart::SummaryText(openai::CompactSummaryTextContent {
            text,
            ..
        }) => text,
        openai::CompactMessageContentPart::Output(openai::ResponseOutputContentPart::Refusal {
            refusal,
            ..
        }) => refusal,
        _ => return None,
    };

    Some(claude::ContentBlock::Text(claude::ResponseTextBlock {
        citations: None,
        text,
        type_: claude::TextBlockType::Text,
        extra: Default::default(),
    }))
}

fn response_mcp_result_block(
    tool_use_id: String,
    output: Option<String>,
    error: Option<String>,
) -> Option<claude::ResponseMcpToolResultBlock> {
    let is_error = error.is_some();
    let content = error.or(output)?;
    Some(claude::ResponseMcpToolResultBlock {
        content: claude::ResponseMcpToolResultContent::String(content),
        is_error,
        tool_use_id,
        type_: claude::ResponseMcpToolResultBlockType::McpToolResult,
        extra: Default::default(),
    })
}

fn reasoning_to_claude_content(
    id: String,
    summary: Vec<openai::ResponseReasoningSummaryPart>,
    content: Option<Vec<openai::ResponseReasoningTextPart>>,
    encrypted_content: Option<String>,
) -> Vec<claude::ContentBlock> {
    let mut blocks = Vec::new();
    if let Some(encrypted_content) = encrypted_content {
        blocks.push(claude::ContentBlock::RedactedThinking(
            claude::RedactedThinkingBlock {
                data: encrypted_content,
                type_: claude::RedactedThinkingBlockType::RedactedThinking,
            },
        ));
    }

    let thinking = join_text(content.into_iter().flatten().map(|part| part.text));
    if !thinking.is_empty() {
        blocks.push(claude::ContentBlock::Thinking(claude::ThinkingBlock {
            signature: id,
            thinking,
            type_: claude::ThinkingBlockType::Thinking,
        }));
    }

    blocks.extend(summary.into_iter().map(|part| {
        claude::ContentBlock::Text(claude::ResponseTextBlock {
            citations: None,
            text: part.text,
            type_: claude::TextBlockType::Text,
            extra: Default::default(),
        })
    }));
    blocks
}

fn openai_usage_to_claude(usage: openai::ResponseUsage) -> claude::Usage {
    claude::Usage {
        input_tokens: Some(u64::from(usage.input_tokens)),
        output_tokens: Some(u64::from(usage.output_tokens)),
        cache_creation_input_tokens: None,
        cache_read_input_tokens: usage
            .input_tokens_details
            .map(|details| u64::from(details.cached_tokens)),
        cache_creation: None,
        output_tokens_details: Some(claude::OutputTokensDetails {
            thinking_tokens: u64::from(usage.output_tokens_details.reasoning_tokens),
            extra: Default::default(),
        }),
        server_tool_use: None,
        iterations: None,
        inference_geo: None,
        service_tier: None,
        speed: None,
        extra: Default::default(),
    }
}
