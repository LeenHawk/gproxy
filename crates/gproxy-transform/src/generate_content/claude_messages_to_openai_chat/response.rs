use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::{stop, usage};

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ChatCompletionResponse = serde_json::from_slice(&body)?;
    let choice = input.choices.into_iter().next();
    let mut blocks = Vec::new();
    if let Some(reasoning) = choice
        .as_ref()
        .and_then(|choice| choice.message.reasoning_content.clone())
        .filter(|value| !value.is_empty())
    {
        blocks.push(claude::ResponseContentBlock::Thinking(
            claude::ThinkingBlock {
                signature: None,
                thinking: reasoning,
                type_: claude::ThinkingBlockType::Thinking,
                rest: Default::default(),
            },
        ));
    }
    if let Some(text) = choice
        .as_ref()
        .and_then(|choice| choice.message.content.clone())
        .filter(|value| !value.is_empty())
    {
        blocks.push(claude::ResponseContentBlock::Text(
            claude::ResponseTextBlock {
                citations: None,
                text,
                type_: claude::TextBlockType::Text,
                rest: Default::default(),
            },
        ));
    }
    let has_refusal = if let Some(refusal) = choice
        .as_ref()
        .and_then(|choice| choice.message.refusal.clone())
        .filter(|value| !value.is_empty())
    {
        blocks.push(claude::ResponseContentBlock::Text(
            claude::ResponseTextBlock {
                citations: None,
                text: refusal,
                type_: claude::TextBlockType::Text,
                rest: Default::default(),
            },
        ));
        true
    } else {
        false
    };
    if let Some(calls) = choice
        .as_ref()
        .and_then(|choice| choice.message.tool_calls.clone())
    {
        for call in calls {
            match call {
                openai::ChatToolCall::Function(call) => {
                    blocks.push(claude::ResponseContentBlock::ToolUse(
                        claude::ResponseToolUseBlock {
                            id: call.id,
                            input: serde_json::from_str(&call.function.arguments)
                                .unwrap_or_default(),
                            name: call.function.name,
                            type_: claude::ToolUseBlockType::ToolUse,
                            caller: None,
                            rest: Default::default(),
                        },
                    ));
                }
                openai::ChatToolCall::Custom(call) => {
                    blocks.push(claude::ResponseContentBlock::ToolUse(
                        claude::ResponseToolUseBlock {
                            id: call.id,
                            input: serde_json::from_str(&call.custom.input).unwrap_or_default(),
                            name: call.custom.name,
                            type_: claude::ToolUseBlockType::ToolUse,
                            caller: None,
                            rest: Default::default(),
                        },
                    ));
                }
                openai::ChatToolCall::Unknown(_) => {}
            }
        }
    }
    if blocks.is_empty() {
        blocks.push(claude::ResponseContentBlock::Text(
            claude::ResponseTextBlock {
                citations: None,
                text: String::new(),
                type_: claude::TextBlockType::Text,
                rest: Default::default(),
            },
        ));
    }
    let usage = usage::chat_to_claude(input.usage).unwrap_or_else(empty_usage);
    let output = claude::CreateMessageResponseBody {
        id: input.id,
        type_: claude::MessageObjectType::Known(claude::MessageObjectTypeKnown::Message),
        role: claude::AssistantRole::Known(claude::AssistantRoleKnown::Assistant),
        content: blocks,
        model: crate::models::common::wire_string(&input.model)?.into(),
        stop_reason: choice.map_or_else(
            || claude::StopReason::Known(claude::StopReasonKnown::EndTurn),
            |choice| {
                let reason = stop::chat_to_claude(choice.finish_reason);
                if has_refusal
                    && matches!(
                        reason,
                        claude::StopReason::Known(claude::StopReasonKnown::EndTurn)
                    )
                {
                    claude::StopReason::Known(claude::StopReasonKnown::Refusal)
                } else {
                    reason
                }
            },
        ),
        stop_sequence: None,
        usage,
        container: None,
        context_management: None,
        diagnostics: None,
        input_transformations: None,
        stop_details: None,
        rest: Default::default(),
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

fn empty_usage() -> claude::Usage {
    claude::Usage {
        input_tokens: Some(0),
        output_tokens: Some(0),
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
        cache_creation: None,
        output_tokens_details: None,
        server_tool_use: None,
        iterations: None,
        inference_geo: None,
        service_tier: None,
        speed: None,
        rest: Default::default(),
    }
}
