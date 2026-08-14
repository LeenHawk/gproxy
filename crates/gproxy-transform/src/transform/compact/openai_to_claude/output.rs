use crate::protocol::{claude, openai};
use crate::transform::TransformContext;

use super::DEFAULT_MODEL;
use super::tools::{
    apply_patch_to_text_editor_input, arguments_to_json_object, code_interpreter_input,
    local_shell_to_bash_input, response_server_tool_use_block, shell_to_bash_input,
    string_input_json_object, web_action_to_claude,
};
use super::util::join_text;

pub fn response(
    input: openai::CompactedResponseObject,
    _: &TransformContext,
) -> claude::CreateMessageResponseBody {
    crate::protocol::wire!(claude::CreateMessageResponseBody {
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
    })
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
        }) => vec![claude::ContentBlock::Compaction(crate::protocol::wire!(
            claude::ResponseCompactionBlock {
                content: None,
                encrypted_content,
                type_: claude::CompactionBlockType::Compaction,
                extra: Default::default(),
            }
        ))],
        openai::CompactResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
            arguments,
            call_id,
            name,
            id,
            ..
        }) => vec![claude::ContentBlock::ToolUse(crate::protocol::wire!(
            claude::ResponseToolUseBlock {
                id: id.unwrap_or(call_id),
                input: arguments_to_json_object(&arguments),
                name,
                type_: claude::ToolUseBlockType::ToolUse,
                caller: None,
                extra: Default::default(),
            }
        ))],
        openai::CompactResponseItem::Typed(openai::TypedResponseItem::CustomToolCall {
            call_id,
            input,
            name,
            id,
            ..
        }) => vec![claude::ContentBlock::ToolUse(crate::protocol::wire!(
            claude::ResponseToolUseBlock {
                id: id.unwrap_or(call_id),
                input: string_input_json_object(input),
                name,
                type_: claude::ToolUseBlockType::ToolUse,
                caller: None,
                extra: Default::default(),
            }
        ))],
        openai::CompactResponseItem::Typed(openai::TypedResponseItem::WebSearchCall {
            id,
            action,
            ..
        }) => {
            let (name, input) = web_action_to_claude(action);
            vec![claude::ContentBlock::ServerToolUse(
                response_server_tool_use_block(id, input, name),
            )]
        }
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
        }) => vec![response_tool_use_block(
            call_id,
            local_shell_to_bash_input(action),
            "bash",
        )],
        openai::CompactResponseItem::Typed(openai::TypedResponseItem::ShellCall {
            action,
            call_id,
            environment: None,
            ..
        }) => vec![response_tool_use_block(
            call_id,
            shell_to_bash_input(action, None),
            "bash",
        )],
        openai::CompactResponseItem::Typed(openai::TypedResponseItem::ShellCall {
            action,
            call_id,
            environment: Some(environment),
            ..
        }) => vec![response_tool_use_block(
            call_id,
            shell_to_bash_input(action, Some(environment)),
            "bash",
        )],
        openai::CompactResponseItem::Typed(openai::TypedResponseItem::ApplyPatchCall {
            call_id,
            operation,
            ..
        }) => vec![response_tool_use_block(
            call_id,
            apply_patch_to_text_editor_input(operation),
            "str_replace_based_edit_tool",
        )],
        openai::CompactResponseItem::Typed(openai::TypedResponseItem::ToolSearchCall {
            arguments,
            id,
            call_id,
            execution,
            ..
        }) => vec![claude::ContentBlock::ServerToolUse(
            response_server_tool_use_block(
                id.or(call_id).unwrap_or_else(|| "tool_search".to_owned()),
                match arguments {
                    serde_json::Value::Object(map) => map.into_iter().collect(),
                    value => {
                        let mut input = claude::JsonObject::new();
                        input.insert("value".to_owned(), value);
                        input
                    }
                },
                if matches!(execution, Some(openai::ToolSearchExecution::Client)) {
                    claude::ServerToolUseNameKnown::ToolSearchToolRegex
                } else {
                    claude::ServerToolUseNameKnown::ToolSearchToolBm25
                },
            ),
        )],
        openai::CompactResponseItem::Typed(openai::TypedResponseItem::ToolSearchOutput {
            tools,
            id,
            call_id,
            ..
        }) => vec![claude::ContentBlock::ToolSearchToolResult(
            crate::protocol::wire!(claude::ResponseToolSearchToolResultBlock {
                content: claude::ResponseToolSearchToolResultContent::Result(
                    crate::protocol::wire!(claude::ResponseToolSearchToolSearchResultBlock {
                        tool_references: response_tool_references(tools),
                        type_: claude::ResponseToolSearchToolSearchResultBlockType::ToolSearchToolSearchResult,
                        extra: Default::default(),
                    }),
                ),
                tool_use_id: call_id.or(id).unwrap_or_else(|| "tool_search".to_owned()),
                type_: claude::ToolSearchToolResultBlockType::ToolSearchToolResult,
                extra: Default::default(),
            }),
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
            let mut blocks = vec![claude::ContentBlock::McpToolUse(crate::protocol::wire!(
                claude::ResponseMcpToolUseBlock {
                    id: id.clone(),
                    input: arguments_to_json_object(&arguments),
                    name,
                    server_name: server_label,
                    type_: claude::ResponseMcpToolUseBlockType::McpToolUse,
                    extra: Default::default(),
                }
            ))];
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

fn response_tool_use_block(
    id: String,
    input: claude::JsonObject,
    name: &str,
) -> claude::ContentBlock {
    claude::ContentBlock::ToolUse(crate::protocol::wire!(claude::ResponseToolUseBlock {
        id,
        input,
        name: name.to_owned(),
        type_: claude::ToolUseBlockType::ToolUse,
        caller: None,
        extra: Default::default(),
    }))
}

fn response_tool_references(
    tools: Vec<openai::ResponseTool>,
) -> Vec<claude::ResponseToolReferenceBlock> {
    tools
        .into_iter()
        .filter_map(|tool| match tool {
            openai::ResponseTool::Function { name, .. }
            | openai::ResponseTool::Custom { name, .. } => Some(name),
            _ => None,
        })
        .map(|tool_name| {
            crate::protocol::wire!(claude::ResponseToolReferenceBlock {
                tool_name,
                type_: claude::ResponseToolReferenceBlockType::ToolReference,
                extra: Default::default(),
            })
        })
        .collect()
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

    Some(claude::ContentBlock::Text(crate::protocol::wire!(
        claude::ResponseTextBlock {
            citations: None,
            text,
            type_: claude::TextBlockType::Text,
            extra: Default::default(),
        }
    )))
}

fn response_mcp_result_block(
    tool_use_id: String,
    output: Option<String>,
    error: Option<String>,
) -> Option<claude::ResponseMcpToolResultBlock> {
    let is_error = error.is_some();
    let content = error.or(output)?;
    Some(crate::protocol::wire!(claude::ResponseMcpToolResultBlock {
        content: claude::ResponseMcpToolResultContent::String(content),
        is_error,
        tool_use_id,
        type_: claude::ResponseMcpToolResultBlockType::McpToolResult,
        extra: Default::default(),
    }))
}

fn reasoning_to_claude_content(
    _id: Option<String>,
    summary: Vec<openai::ResponseReasoningSummaryPart>,
    content: Option<Vec<openai::ResponseReasoningTextPart>>,
    encrypted_content: Option<String>,
) -> Vec<claude::ContentBlock> {
    let mut blocks = Vec::new();
    let thinking = join_text(content.into_iter().flatten().map(|part| part.text));
    if !thinking.is_empty() {
        if let Some(signature) = encrypted_content.filter(|value| !value.is_empty()) {
            blocks.push(claude::ContentBlock::Thinking(crate::protocol::wire!(
                claude::ThinkingBlock {
                    signature,
                    thinking,
                    type_: claude::ThinkingBlockType::Thinking,
                }
            )));
        } else {
            blocks.push(claude::ContentBlock::Text(crate::protocol::wire!(
                claude::ResponseTextBlock {
                    citations: None,
                    text: thinking,
                    type_: claude::TextBlockType::Text,
                    extra: Default::default(),
                }
            )));
        }
    } else if let Some(encrypted_content) = encrypted_content {
        blocks.push(claude::ContentBlock::RedactedThinking(
            crate::protocol::wire!(claude::RedactedThinkingBlock {
                data: encrypted_content,
                type_: claude::RedactedThinkingBlockType::RedactedThinking,
            }),
        ));
    }

    blocks.extend(summary.into_iter().map(|part| {
        claude::ContentBlock::Text(crate::protocol::wire!(claude::ResponseTextBlock {
            citations: None,
            text: part.text,
            type_: claude::TextBlockType::Text,
            extra: Default::default(),
        }))
    }));
    blocks
}

fn openai_usage_to_claude(usage: openai::ResponseUsage) -> claude::Usage {
    let details = usage.input_tokens_details;
    let cached = details.as_ref().map_or(0, |details| details.cached_tokens);
    let cache_write = details
        .as_ref()
        .map_or(0, |details| details.cache_write_tokens);
    crate::protocol::wire!(claude::Usage {
        input_tokens: Some(u64::from(
            usage
                .input_tokens
                .saturating_sub(cached)
                .saturating_sub(cache_write),
        )),
        output_tokens: Some(u64::from(usage.output_tokens)),
        cache_creation_input_tokens: details
            .as_ref()
            .filter(|details| details.cache_write_tokens > 0)
            .map(|details| u64::from(details.cache_write_tokens)),
        cache_read_input_tokens: details.map(|details| u64::from(details.cached_tokens)),
        cache_creation: None,
        output_tokens_details: Some(crate::protocol::wire!(claude::OutputTokensDetails {
            thinking_tokens: u64::from(usage.output_tokens_details.reasoning_tokens),
            extra: Default::default(),
        })),
        server_tool_use: None,
        iterations: None,
        inference_geo: None,
        service_tier: None,
        speed: None,
        extra: Default::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_usage_subtracts_cache_from_openai_input_total() {
        let usage = openai_usage_to_claude(crate::protocol::wire!(openai::ResponseUsage {
            input_tokens: 100,
            output_tokens: 20,
            total_tokens: 120,
            input_tokens_details: Some(crate::protocol::wire!(
                openai::ResponseInputTokensDetails {
                    cached_tokens: 60,
                    cache_write_tokens: 10,
                    extra: Default::default(),
                }
            )),
            output_tokens_details: crate::protocol::wire!(openai::ResponseOutputTokensDetails {
                reasoning_tokens: 5,
                extra: Default::default(),
            }),
            extra: Default::default(),
        }));
        assert_eq!(usage.input_tokens, Some(30));
        assert_eq!(usage.cache_read_input_tokens, Some(60));
        assert_eq!(usage.cache_creation_input_tokens, Some(10));
    }
}
