use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;

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
        usage: common::completion_usage_to_claude(common::response_usage_to_completion(
            input.usage,
        )),
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
        _ => Vec::new(),
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
            openai::ResponseItem::Typed(openai::TypedResponseItem::FunctionCall { .. })
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
