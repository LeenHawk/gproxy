use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::usage::response_usage_to_claude;
use crate::transform::compact::openai_to_claude::{
    apply_patch_to_text_editor_input, local_shell_to_bash_input, shell_to_bash_input,
    web_action_to_claude,
};

pub fn response(
    input: openai::ResponseObject,
    _: &TransformContext,
) -> Result<claude::CreateMessageResponseBody, TransformError> {
    let stop_reason = response_stop_reason(&input);
    Ok(crate::protocol::wire!(claude::CreateMessageResponseBody {
        id: input.id,
        type_: claude::MessageObjectType::Known(claude::MessageObjectTypeKnown::Message),
        role: claude::AssistantRole::Known(claude::AssistantRoleKnown::Assistant),
        content: input
            .output
            .into_iter()
            .flat_map(response_item_to_claude_content)
            .collect(),
        model: claude::ClaudeModel::Unknown(
            input
                .model
                .map(common::openai_model_string)
                .unwrap_or_else(|| common::DEFAULT_OPENAI_MODEL.to_owned()),
        ),
        stop_reason,
        stop_sequence: None,
        usage: response_usage_to_claude(input.usage),
        container: None,
        context_management: None,
        diagnostics: None,
        stop_details: None,
        extra: Default::default(),
    }))
}

fn response_item_to_claude_content(item: openai::ResponseOutputItem) -> Vec<claude::ContentBlock> {
    match item.0 {
        openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => message
            .content
            .into_iter()
            .map(|part| match part {
                openai::ResponseMessageOutputContentPart::OutputText { text, .. } => {
                    response_text_block(text)
                }
                openai::ResponseMessageOutputContentPart::Refusal { refusal, .. } => {
                    response_text_block(refusal)
                }
                _ => unreachable!(
                    "new non-exhaustive protocol variant requires a lockstep transform update"
                ),
            })
            .collect(),
        openai::ResponseItem::Typed(openai::TypedResponseItem::Reasoning {
            summary,
            content,
            encrypted_content,
            ..
        }) => {
            let thinking = content
                .into_iter()
                .flatten()
                .map(|part| part.text)
                .collect::<Vec<_>>()
                .join("");
            let mut blocks = Vec::new();
            if !thinking.is_empty() {
                if let Some(signature) = encrypted_content {
                    blocks.push(claude::ContentBlock::Thinking(crate::protocol::wire!(
                        claude::ThinkingBlock {
                            signature,
                            thinking,
                            type_: claude::ThinkingBlockType::Thinking,
                        }
                    )));
                } else {
                    blocks.push(response_text_block(thinking));
                }
            } else if let Some(data) = encrypted_content {
                blocks.push(claude::ContentBlock::RedactedThinking(
                    crate::protocol::wire!(claude::RedactedThinkingBlock {
                        data,
                        type_: claude::RedactedThinkingBlockType::RedactedThinking,
                    }),
                ));
            }
            blocks.extend(
                summary
                    .into_iter()
                    .map(|part| response_text_block(part.text)),
            );
            blocks
        }
        openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall {
            arguments,
            call_id,
            name,
            id,
            ..
        }) => vec![claude::ContentBlock::ToolUse(crate::protocol::wire!(
            claude::ResponseToolUseBlock {
                id: id.unwrap_or(call_id),
                input: serde_json::from_str(&arguments).unwrap_or_default(),
                name,
                type_: claude::ToolUseBlockType::ToolUse,
                caller: None,
                extra: Default::default(),
            }
        ))],
        openai::ResponseItem::Typed(openai::TypedResponseItem::CustomToolCall {
            call_id,
            input,
            name,
            id,
            ..
        }) => vec![tool_use_block(
            id.unwrap_or(call_id),
            string_input_to_object(input),
            name,
        )],
        openai::ResponseItem::Typed(openai::TypedResponseItem::WebSearchCall {
            id,
            action,
            ..
        }) => {
            let (name, input) = web_action_to_claude(action);
            vec![server_tool_use_block(id, input, name)]
        }
        openai::ResponseItem::Typed(openai::TypedResponseItem::LocalShellCall {
            call_id,
            action,
            ..
        }) => vec![tool_use_block(
            call_id,
            local_shell_to_bash_input(action),
            "bash".to_owned(),
        )],
        openai::ResponseItem::Typed(openai::TypedResponseItem::ShellCall {
            call_id,
            action,
            environment,
            ..
        }) => vec![tool_use_block(
            call_id,
            shell_to_bash_input(action, environment),
            "bash".to_owned(),
        )],
        openai::ResponseItem::Typed(openai::TypedResponseItem::ApplyPatchCall {
            call_id,
            operation,
            ..
        }) => vec![tool_use_block(
            call_id,
            apply_patch_to_text_editor_input(operation),
            "str_replace_based_edit_tool".to_owned(),
        )],
        openai::ResponseItem::Typed(openai::TypedResponseItem::ToolSearchCall {
            arguments,
            id,
            call_id,
            execution,
            ..
        }) => vec![server_tool_use_block(
            id.or(call_id).unwrap_or_else(|| "tool_search".to_owned()),
            json_value_to_object(arguments),
            if matches!(execution, Some(openai::ToolSearchExecution::Client)) {
                claude::ServerToolUseNameKnown::ToolSearchToolRegex
            } else {
                claude::ServerToolUseNameKnown::ToolSearchToolBm25
            },
        )],
        _ => Vec::new(),
    }
}

fn tool_use_block(id: String, input: claude::JsonObject, name: String) -> claude::ContentBlock {
    claude::ContentBlock::ToolUse(crate::protocol::wire!(claude::ResponseToolUseBlock {
        id,
        input,
        name,
        type_: claude::ToolUseBlockType::ToolUse,
        caller: None,
        extra: Default::default(),
    }))
}

fn server_tool_use_block(
    id: String,
    input: claude::JsonObject,
    name: claude::ServerToolUseNameKnown,
) -> claude::ContentBlock {
    claude::ContentBlock::ServerToolUse(crate::protocol::wire!(
        claude::ResponseServerToolUseBlock {
            id,
            input,
            name: claude::ServerToolUseName::Known(name),
            type_: claude::ServerToolUseBlockType::ServerToolUse,
            caller: None,
            extra: Default::default(),
        }
    ))
}

fn string_input_to_object(input: String) -> claude::JsonObject {
    serde_json::from_str(&input).unwrap_or_else(|_| {
        let mut object = claude::JsonObject::new();
        object.insert("input".to_owned(), serde_json::Value::String(input));
        object
    })
}

fn json_value_to_object(value: serde_json::Value) -> claude::JsonObject {
    match value {
        serde_json::Value::Object(object) => object.into_iter().collect(),
        value => {
            let mut object = claude::JsonObject::new();
            object.insert("value".to_owned(), value);
            object
        }
    }
}

fn response_text_block(text: String) -> claude::ContentBlock {
    claude::ContentBlock::Text(crate::protocol::wire!(claude::ResponseTextBlock {
        citations: None,
        text,
        type_: claude::TextBlockType::Text,
        extra: Default::default(),
    }))
}

fn response_stop_reason(response: &openai::ResponseObject) -> claude::StopReason {
    if response.output.iter().any(|item| {
        matches!(
            item.0,
            openai::ResponseItem::Typed(
                openai::TypedResponseItem::FunctionCall { .. }
                    | openai::TypedResponseItem::CustomToolCall { .. }
                    | openai::TypedResponseItem::WebSearchCall { .. }
                    | openai::TypedResponseItem::LocalShellCall { .. }
                    | openai::TypedResponseItem::ShellCall { .. }
                    | openai::TypedResponseItem::ApplyPatchCall { .. }
                    | openai::TypedResponseItem::ToolSearchCall { .. }
            )
        )
    }) {
        return claude::StopReason::Known(claude::StopReasonKnown::ToolUse);
    }
    match response.status {
        Some(openai::ResponseStatus::Incomplete) => {
            claude::StopReason::Known(claude::StopReasonKnown::MaxTokens)
        }
        _ => claude::StopReason::Known(claude::StopReasonKnown::EndTurn),
    }
}
