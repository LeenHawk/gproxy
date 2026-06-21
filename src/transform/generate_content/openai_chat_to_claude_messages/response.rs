use crate::protocol::{claude, openai};
use crate::transform::{TransformContext, TransformError};

use super::super::common;
use super::tools::{chat_tool_call_to_claude_response, parse_json_object, response_tool_use_block};

pub fn response(
    input: openai::ChatCompletionResponse,
    _: &TransformContext,
) -> Result<claude::CreateMessageResponseBody, TransformError> {
    let mut content = Vec::new();
    let mut stop_reason = claude::StopReason::Known(claude::StopReasonKnown::EndTurn);
    let mut has_tool_use = false;
    let mut has_refusal = false;

    if let Some(choice) = input.choices.into_iter().next() {
        stop_reason = chat_finish_reason_to_claude(choice.finish_reason);
        if let Some(text) = choice.message.content.filter(|value| !value.is_empty()) {
            content.push(claude::ContentBlock::Text(claude::ResponseTextBlock {
                citations: None,
                text,
                type_: claude::TextBlockType::Text,
                extra: Default::default(),
            }));
        }
        if let Some(refusal) = choice.message.refusal.filter(|value| !value.is_empty()) {
            has_refusal = true;
            content.push(claude::ContentBlock::Text(claude::ResponseTextBlock {
                citations: None,
                text: refusal,
                type_: claude::TextBlockType::Text,
                extra: Default::default(),
            }));
        }
        if let Some(function_call) = choice.message.function_call {
            has_tool_use = true;
            content.push(response_tool_use_block(
                "function_call".to_owned(),
                function_call.name,
                parse_json_object(function_call.arguments),
            ));
        }
        if let Some(tool_calls) = choice.message.tool_calls {
            for call in tool_calls {
                has_tool_use = true;
                content.push(chat_tool_call_to_claude_response(call));
            }
        }
    }

    if content.is_empty() {
        content.push(claude::ContentBlock::Text(claude::ResponseTextBlock {
            citations: None,
            text: String::new(),
            type_: claude::TextBlockType::Text,
            extra: Default::default(),
        }));
    }
    if matches!(
        stop_reason,
        claude::StopReason::Known(claude::StopReasonKnown::EndTurn)
    ) {
        if has_tool_use {
            stop_reason = claude::StopReason::Known(claude::StopReasonKnown::ToolUse);
        } else if has_refusal {
            stop_reason = claude::StopReason::Known(claude::StopReasonKnown::Refusal);
        }
    }

    Ok(claude::CreateMessageResponseBody {
        id: input.id,
        type_: claude::MessageObjectType::Known(claude::MessageObjectTypeKnown::Message),
        role: claude::AssistantRole::Known(claude::AssistantRoleKnown::Assistant),
        content,
        model: common::openai_model_string(input.model).into(),
        stop_reason,
        stop_sequence: None,
        usage: common::completion_usage_to_claude(input.usage),
        container: None,
        context_management: None,
        diagnostics: None,
        stop_details: None,
        extra: Default::default(),
    })
}

fn chat_finish_reason_to_claude(reason: openai::ChatFinishReason) -> claude::StopReason {
    match reason {
        openai::ChatFinishReason::Stop => {
            claude::StopReason::Known(claude::StopReasonKnown::EndTurn)
        }
        openai::ChatFinishReason::Length => {
            claude::StopReason::Known(claude::StopReasonKnown::MaxTokens)
        }
        openai::ChatFinishReason::ToolCalls | openai::ChatFinishReason::FunctionCall => {
            claude::StopReason::Known(claude::StopReasonKnown::ToolUse)
        }
        openai::ChatFinishReason::ContentFilter => {
            claude::StopReason::Known(claude::StopReasonKnown::Refusal)
        }
    }
}
