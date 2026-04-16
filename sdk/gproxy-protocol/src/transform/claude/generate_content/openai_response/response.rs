use crate::claude::count_tokens::types::{
    BetaCompactionBlockType, BetaMcpToolResultBlockParamContent, BetaMcpToolUseBlockType,
    BetaRequestMcpToolResultBlockType, BetaServerToolUseBlockType, BetaServerToolUseName,
    BetaThinkingBlockType, BetaToolUseBlockType,
};
use crate::claude::create_message::response::ClaudeCreateMessageResponse;
use crate::claude::create_message::types::{
    BetaContentBlock, BetaMessage, BetaMessageRole, BetaMessageType, BetaServiceTier,
    BetaStopReason, BetaTextBlock, BetaTextBlockType, BetaUsage, Model,
};
use crate::claude::types::ClaudeResponseHeaders;
use crate::openai::count_tokens::types::{ResponseInputContent, ResponseOutputContent};
use crate::openai::create_response::response::OpenAiCreateResponseResponse;
use crate::openai::create_response::types::{
    ResponseIncompleteReason, ResponseOutputItem, ResponseServiceTier,
};
use crate::transform::claude::generate_content::utils::{
    beta_usage_from_counts, parse_json_object_or_empty,
};
use crate::transform::claude::utils::beta_error_response_from_status_message;
use crate::transform::utils::TransformError;

fn web_search_tool_use_id(
    id: Option<String>,
    action: &crate::openai::count_tokens::types::ResponseFunctionWebSearchAction,
) -> String {
    id.unwrap_or_else(|| match action {
        crate::openai::count_tokens::types::ResponseFunctionWebSearchAction::Search {
            query,
            queries,
            ..
        } => query
            .clone()
            .or_else(|| queries.as_ref().and_then(|items| items.first().cloned()))
            .unwrap_or_else(|| "web_search".to_string()),
        crate::openai::count_tokens::types::ResponseFunctionWebSearchAction::OpenPage { url } => {
            url.clone()
                .unwrap_or_else(|| "web_search_open_page".to_string())
        }
        crate::openai::count_tokens::types::ResponseFunctionWebSearchAction::FindInPage {
            pattern,
            url,
        } => format!("web_search_find_in_page:{pattern}:{url}"),
    })
}

impl TryFrom<OpenAiCreateResponseResponse> for ClaudeCreateMessageResponse {
    type Error = TransformError;

    fn try_from(value: OpenAiCreateResponseResponse) -> Result<Self, TransformError> {
        Ok(match value {
            OpenAiCreateResponseResponse::Success {
                stats_code,
                headers,
                body,
            } => {
                let mut content = Vec::new();
                let mut has_tool_use = false;
                let mut has_refusal = false;
                let mut has_compaction = false;

                let response_input_content_to_text = |items: Vec<ResponseInputContent>| {
                    items
                        .into_iter()
                        .filter_map(|item| match item {
                            ResponseInputContent::Text(text) => Some(text.text),
                            ResponseInputContent::Image(image) => {
                                if let Some(url) = image.image_url {
                                    Some(url)
                                } else {
                                    image.file_id.map(|file_id| format!("file:{file_id}"))
                                }
                            }
                            ResponseInputContent::File(file) => {
                                if let Some(data) = file.file_data {
                                    Some(data)
                                } else if let Some(url) = file.file_url {
                                    Some(url)
                                } else if let Some(file_id) = file.file_id {
                                    Some(format!("file:{file_id}"))
                                } else {
                                    file.filename
                                }
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };

                for item in body.output {
                    match item {
                        ResponseOutputItem::Message(message) => {
                            for part in message.content {
                                match part {
                                    ResponseOutputContent::Text(text) => {
                                        content.push(BetaContentBlock::Text(BetaTextBlock {
                                            citations: None,
                                            text: text.text,
                                            type_: BetaTextBlockType::Text,
                                        }));
                                    }
                                    ResponseOutputContent::Refusal(refusal) => {
                                        has_refusal = true;
                                        content.push(BetaContentBlock::Text(BetaTextBlock {
                                            citations: None,
                                            text: refusal.refusal,
                                            type_: BetaTextBlockType::Text,
                                        }));
                                    }
                                }
                            }
                        }
                        ResponseOutputItem::FunctionToolCall(call) => {
                            has_tool_use = true;
                            content.push(BetaContentBlock::ToolUse(
                                crate::claude::create_message::types::BetaToolUseBlock {
                                    id: call.id.unwrap_or_else(|| call.call_id.clone()),
                                    input: parse_json_object_or_empty(&call.arguments),
                                    name: call.name,
                                    type_: BetaToolUseBlockType::ToolUse,
                                    cache_control: None,
                                    caller: None,
                                },
                            ));
                        }
                        ResponseOutputItem::CustomToolCall(call) => {
                            has_tool_use = true;
                            content.push(BetaContentBlock::ToolUse(
                                crate::claude::create_message::types::BetaToolUseBlock {
                                    id: call.id.unwrap_or_else(|| call.call_id.clone()),
                                    input: parse_json_object_or_empty(&call.input),
                                    name: call.name,
                                    type_: BetaToolUseBlockType::ToolUse,
                                    cache_control: None,
                                    caller: None,
                                },
                            ));
                        }
                        ResponseOutputItem::FunctionCallOutput(call) => {
                            let output = match call.output {
                                crate::openai::count_tokens::types::ResponseFunctionCallOutputContent::Text(text) => text,
                                crate::openai::count_tokens::types::ResponseFunctionCallOutputContent::Content(items) => response_input_content_to_text(items),
                            };
                            if !output.is_empty() {
                                content.push(BetaContentBlock::Text(BetaTextBlock {
                                    citations: None,
                                    text: format!("tool_result({}): {}", call.call_id, output),
                                    type_: BetaTextBlockType::Text,
                                }));
                            }
                        }
                        ResponseOutputItem::CustomToolCallOutput(call) => {
                            let output = match call.output {
                                crate::openai::count_tokens::types::ResponseCustomToolCallOutputContent::Text(text) => text,
                                crate::openai::count_tokens::types::ResponseCustomToolCallOutputContent::Content(items) => response_input_content_to_text(items),
                            };
                            if !output.is_empty() {
                                content.push(BetaContentBlock::Text(BetaTextBlock {
                                    citations: None,
                                    text: format!(
                                        "custom_tool_result({}): {}",
                                        call.call_id, output
                                    ),
                                    type_: BetaTextBlockType::Text,
                                }));
                            }
                        }
                        ResponseOutputItem::McpCall(call) => {
                            has_tool_use = true;
                            let tool_use_id = call.id.clone();
                            let is_error = call.error.is_some();
                            let result_text = call.output.or(call.error);
                            content.push(BetaContentBlock::McpToolUse(
                                crate::claude::create_message::types::BetaMcpToolUseBlock {
                                    id: tool_use_id.clone(),
                                    input: parse_json_object_or_empty(&call.arguments),
                                    name: call.name,
                                    server_name: call.server_label,
                                    type_: BetaMcpToolUseBlockType::McpToolUse,
                                    cache_control: None,
                                },
                            ));
                            if let Some(result_text) = result_text {
                                content.push(BetaContentBlock::McpToolResult(
                                    crate::claude::create_message::types::BetaMcpToolResultBlock {
                                        tool_use_id,
                                        type_: BetaRequestMcpToolResultBlockType::McpToolResult,
                                        cache_control: None,
                                        content: Some(BetaMcpToolResultBlockParamContent::Text(
                                            result_text,
                                        )),
                                        is_error: Some(is_error),
                                    },
                                ));
                            }
                        }
                        ResponseOutputItem::CodeInterpreterToolCall(call) => {
                            has_tool_use = true;
                            content.push(BetaContentBlock::ServerToolUse(
                                crate::claude::create_message::types::BetaServerToolUseBlock {
                                    id: call.id,
                                    input: Default::default(),
                                    name: BetaServerToolUseName::CodeExecution,
                                    type_: BetaServerToolUseBlockType::ServerToolUse,
                                    cache_control: None,
                                    caller: None,
                                },
                            ));
                        }
                        ResponseOutputItem::FunctionWebSearch(call) => {
                            has_tool_use = true;
                            let crate::openai::count_tokens::types::ResponseFunctionWebSearch {
                                id,
                                action,
                                ..
                            } = call;
                            content.push(BetaContentBlock::ServerToolUse(
                                crate::claude::create_message::types::BetaServerToolUseBlock {
                                    id: web_search_tool_use_id(id, &action),
                                    input: Default::default(),
                                    name: BetaServerToolUseName::WebSearch,
                                    type_: BetaServerToolUseBlockType::ServerToolUse,
                                    cache_control: None,
                                    caller: None,
                                },
                            ));
                        }
                        ResponseOutputItem::ShellCall(call) => {
                            has_tool_use = true;
                            content.push(BetaContentBlock::ServerToolUse(
                                crate::claude::create_message::types::BetaServerToolUseBlock {
                                    id: call.id.unwrap_or_else(|| call.call_id.clone()),
                                    input: Default::default(),
                                    name: BetaServerToolUseName::BashCodeExecution,
                                    type_: BetaServerToolUseBlockType::ServerToolUse,
                                    cache_control: None,
                                    caller: None,
                                },
                            ));
                        }
                        ResponseOutputItem::ShellCallOutput(call) => {
                            let output = call
                                .output
                                .into_iter()
                                .map(|entry| {
                                    format!("stdout: {}\nstderr: {}", entry.stdout, entry.stderr)
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            if !output.is_empty() {
                                content.push(BetaContentBlock::Text(BetaTextBlock {
                                    citations: None,
                                    text: format!("shell_output({}): {}", call.call_id, output),
                                    type_: BetaTextBlockType::Text,
                                }));
                            }
                        }
                        ResponseOutputItem::LocalShellCall(call) => {
                            has_tool_use = true;
                            content.push(BetaContentBlock::ServerToolUse(
                                crate::claude::create_message::types::BetaServerToolUseBlock {
                                    id: call.id,
                                    input: Default::default(),
                                    name: BetaServerToolUseName::BashCodeExecution,
                                    type_: BetaServerToolUseBlockType::ServerToolUse,
                                    cache_control: None,
                                    caller: None,
                                },
                            ));
                        }
                        ResponseOutputItem::LocalShellCallOutput(call)
                            if !call.output.is_empty() =>
                        {
                            content.push(BetaContentBlock::Text(BetaTextBlock {
                                citations: None,
                                text: format!("local_shell_output({}): {}", call.id, call.output),
                                type_: BetaTextBlockType::Text,
                            }));
                        }
                        ResponseOutputItem::ApplyPatchCall(call) => {
                            has_tool_use = true;
                            content.push(BetaContentBlock::ServerToolUse(
                                crate::claude::create_message::types::BetaServerToolUseBlock {
                                    id: call.id.unwrap_or_else(|| call.call_id.clone()),
                                    input: Default::default(),
                                    name: BetaServerToolUseName::TextEditorCodeExecution,
                                    type_: BetaServerToolUseBlockType::ServerToolUse,
                                    cache_control: None,
                                    caller: None,
                                },
                            ));
                        }
                        ResponseOutputItem::ApplyPatchCallOutput(call) => {
                            let status = match call.status {
                                crate::openai::count_tokens::types::ResponseApplyPatchCallOutputStatus::Completed => "completed",
                                crate::openai::count_tokens::types::ResponseApplyPatchCallOutputStatus::Failed => "failed",
                            };
                            let text = if let Some(output) = call.output {
                                format!(
                                    "apply_patch_output({}): {}\n{}",
                                    call.call_id, status, output
                                )
                            } else {
                                format!("apply_patch_output({}): {}", call.call_id, status)
                            };
                            content.push(BetaContentBlock::Text(BetaTextBlock {
                                citations: None,
                                text,
                                type_: BetaTextBlockType::Text,
                            }));
                        }
                        ResponseOutputItem::ReasoningItem(reasoning) => {
                            let signature = reasoning.id.filter(|id| !id.is_empty());
                            let mut thinking = reasoning
                                .summary
                                .into_iter()
                                .map(|item| item.text)
                                .collect::<Vec<_>>();
                            if thinking.is_empty()
                                && let Some(reasoning_content) = reasoning.content
                            {
                                thinking
                                    .extend(reasoning_content.into_iter().map(|item| item.text));
                            }
                            let thinking = thinking.join("\n");
                            if !thinking.is_empty()
                                && let Some(signature) = signature
                            {
                                content.push(BetaContentBlock::Thinking(
                                    crate::claude::create_message::types::BetaThinkingBlock {
                                        signature,
                                        thinking,
                                        type_: BetaThinkingBlockType::Thinking,
                                    },
                                ));
                            }
                        }
                        ResponseOutputItem::CompactionItem(compaction) => {
                            has_compaction = true;
                            content.push(BetaContentBlock::Compaction(
                                crate::claude::create_message::types::BetaCompactionBlock {
                                    content: Some(compaction.encrypted_content),
                                    type_: BetaCompactionBlockType::Compaction,
                                    cache_control: None,
                                },
                            ));
                        }
                        ResponseOutputItem::ImageGenerationCall(call)
                            if !call.result.is_empty() =>
                        {
                            content.push(BetaContentBlock::Text(BetaTextBlock {
                                citations: None,
                                text: call.result,
                                type_: BetaTextBlockType::Text,
                            }));
                        }
                        _ => {}
                    }
                }

                if content.is_empty() {
                    content.push(BetaContentBlock::Text(BetaTextBlock {
                        citations: None,
                        text: String::new(),
                        type_: BetaTextBlockType::Text,
                    }));
                }

                let stop_reason = if has_compaction {
                    Some(BetaStopReason::Compaction)
                } else if has_tool_use {
                    Some(BetaStopReason::ToolUse)
                } else if matches!(
                    body.incomplete_details
                        .as_ref()
                        .and_then(|details| details.reason.as_ref()),
                    Some(ResponseIncompleteReason::MaxOutputTokens)
                ) {
                    Some(BetaStopReason::MaxTokens)
                } else if has_refusal
                    || matches!(
                        body.incomplete_details
                            .as_ref()
                            .and_then(|details| details.reason.as_ref()),
                        Some(ResponseIncompleteReason::ContentFilter)
                    )
                {
                    Some(BetaStopReason::Refusal)
                } else {
                    Some(BetaStopReason::EndTurn)
                };

                let (input_tokens, cached_tokens, output_tokens) = body
                    .usage
                    .as_ref()
                    .map(|usage| {
                        let cached_tokens = usage.input_tokens_details.cached_tokens;
                        let total_input_tokens = if usage.total_tokens >= usage.output_tokens {
                            usage.total_tokens.saturating_sub(usage.output_tokens)
                        } else {
                            usage.input_tokens
                        };
                        (
                            total_input_tokens.saturating_sub(cached_tokens),
                            cached_tokens,
                            usage.output_tokens,
                        )
                    })
                    .unwrap_or((0, 0, 0));
                let service_tier = match body.service_tier {
                    Some(ResponseServiceTier::Priority) => BetaServiceTier::Priority,
                    _ => BetaServiceTier::Standard,
                };
                let usage: BetaUsage = beta_usage_from_counts(
                    input_tokens,
                    cached_tokens,
                    output_tokens,
                    service_tier,
                );

                ClaudeCreateMessageResponse::Success {
                    stats_code,
                    headers: ClaudeResponseHeaders {
                        extra: headers.extra,
                    },
                    body: BetaMessage {
                        id: body.id,
                        container: None,
                        content,
                        context_management: None,
                        model: Model::Custom(body.model),
                        role: BetaMessageRole::Assistant,
                        stop_reason,
                        stop_sequence: None,
                        type_: BetaMessageType::Message,
                        usage,
                    },
                }
            }
            OpenAiCreateResponseResponse::Error {
                stats_code,
                headers,
                body,
            } => ClaudeCreateMessageResponse::Error {
                stats_code,
                headers: ClaudeResponseHeaders {
                    extra: headers.extra,
                },
                body: beta_error_response_from_status_message(stats_code, body.error.message),
            },
        })
    }
}
