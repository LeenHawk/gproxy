use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::items;
use crate::common::usage;

pub(crate) fn transform(body: bytes::Bytes) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ResponseObject = serde_json::from_slice(&body)?;
    let output = transform_typed(input)?;
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

pub(crate) fn transform_typed(
    input: openai::ResponseObject,
) -> Result<claude::CreateMessageResponseBody, TransformError> {
    let model = input
        .model
        .as_ref()
        .map(crate::models::common::wire_string)
        .transpose()?
        .unwrap_or_else(|| "unknown".into())
        .into();
    let usage = usage::responses_to_claude(input.usage).unwrap_or_else(empty_usage);
    let mut blocks = Vec::new();
    for item in input.output {
        blocks.extend(item_blocks(item)?);
    }
    let has_tool = blocks
        .iter()
        .any(|block| matches!(block, claude::ResponseContentBlock::ToolUse(_)));
    let stop_reason = match input.status {
        Some(openai::ResponseStatus::Incomplete) => {
            claude::StopReason::Known(claude::StopReasonKnown::MaxTokens)
        }
        Some(openai::ResponseStatus::Failed) => {
            claude::StopReason::Known(claude::StopReasonKnown::Refusal)
        }
        _ if has_tool => claude::StopReason::Known(claude::StopReasonKnown::ToolUse),
        _ => claude::StopReason::Known(claude::StopReasonKnown::EndTurn),
    };
    let output = crate::wire!(claude::CreateMessageResponseBody {
        id: input.id,
        type_: claude::MessageObjectType::Known(claude::MessageObjectTypeKnown::Message),
        role: claude::AssistantRole::Known(claude::AssistantRoleKnown::Assistant),
        content: blocks,
        model,
        stop_reason,
        stop_sequence: None,
        usage,
        container: None,
        context_management: None,
        diagnostics: None,
        input_transformations: None,
        stop_details: None,
        rest: Default::default(),
    });
    Ok(output)
}

fn item_blocks(
    item: openai::ResponseItem,
) -> Result<Vec<claude::ResponseContentBlock>, TransformError> {
    match item {
        openai::ResponseItem::Message(message) => match message {
            openai::ResponseMessageItem::Output(message) => {
                let blocks = message
                    .content
                    .into_iter()
                    .map(|part| match part {
                        openai::ResponseMessageOutputContentPart::OutputText(part) => {
                            claude::ResponseContentBlock::Text(crate::wire!(claude::ResponseTextBlock {
                                citations: None,
                                text: part.text,
                                type_: claude::TextBlockType::Text,
                                rest: Default::default(),
                            }))
                        }
                        openai::ResponseMessageOutputContentPart::Refusal(part) => {
                            claude::ResponseContentBlock::Text(crate::wire!(claude::ResponseTextBlock {
                                citations: None,
                                text: part.refusal,
                                type_: claude::TextBlockType::Text,
                                rest: Default::default(),
                            }))
                        }
                        openai::ResponseMessageOutputContentPart::Unknown(_) => {
                            claude::ResponseContentBlock::Text(crate::wire!(claude::ResponseTextBlock {
                                citations: None,
                                text: String::new(),
                                type_: claude::TextBlockType::Text,
                                rest: Default::default(),
                            }))
                        },
                        #[cfg(not(feature = "exhaustive"))]
                        _ => claude::ResponseContentBlock::Text(crate::wire!(
                            claude::ResponseTextBlock {
                                citations: None,
                                text: String::new(),
                                type_: claude::TextBlockType::Text,
                                rest: Default::default(),
                            }
                        )),
                    })
                    .filter(|block| !matches!(block, claude::ResponseContentBlock::Text(text) if text.text.is_empty()))
                    .collect::<Vec<_>>();
                Ok(blocks)
            }
            openai::ResponseMessageItem::Unknown(_)
            | openai::ResponseMessageItem::Input(_)
            | openai::ResponseMessageItem::EasyInput(_) => Ok(Vec::new()),
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
                ));
            }
        },
        openai::ResponseItem::Typed(item) => typed_blocks(*item),
        openai::ResponseItem::Unknown(_) => Ok(Vec::new()),
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    }
}

fn typed_blocks(
    item: openai::TypedResponseItem,
) -> Result<Vec<claude::ResponseContentBlock>, TransformError> {
    Ok(match item {
        openai::TypedResponseItem::FunctionCall {
            arguments,
            call_id,
            name,
            ..
        } => vec![claude::ResponseContentBlock::ToolUse(crate::wire!(
            claude::ResponseToolUseBlock {
                id: call_id,
                input: serde_json::from_str(&arguments).unwrap_or_default(),
                name,
                type_: claude::ToolUseBlockType::ToolUse,
                caller: None,
                rest: Default::default(),
            }
        ))],
        openai::TypedResponseItem::CustomToolCall {
            call_id,
            input,
            name,
            ..
        } => vec![claude::ResponseContentBlock::ToolUse(crate::wire!(
            claude::ResponseToolUseBlock {
                id: call_id,
                input: string_input(input),
                name,
                type_: claude::ToolUseBlockType::ToolUse,
                caller: None,
                rest: Default::default(),
            }
        ))],
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
            match (thinking.is_empty(), encrypted_content) {
                (false, Some(signature)) => vec![claude::ResponseContentBlock::Thinking(
                    crate::wire!(claude::ThinkingBlock {
                        signature: Some(signature),
                        thinking,
                        type_: claude::ThinkingBlockType::Thinking,
                        rest: Default::default(),
                    }),
                )],
                (false, None) => vec![claude::ResponseContentBlock::Text(crate::wire!(
                    claude::ResponseTextBlock {
                        citations: None,
                        text: thinking,
                        type_: claude::TextBlockType::Text,
                        rest: Default::default(),
                    }
                ))],
                (true, Some(data)) => vec![claude::ResponseContentBlock::RedactedThinking(
                    crate::wire!(claude::RedactedThinkingBlock {
                        data,
                        type_: claude::RedactedThinkingBlockType::RedactedThinking,
                        rest: Default::default(),
                    }),
                )],
                (true, None) => Vec::new(),
            }
        }
        openai::TypedResponseItem::Compaction {
            encrypted_content, ..
        } => vec![claude::ResponseContentBlock::Compaction(crate::wire!(
            claude::ResponseCompactionBlock {
                content: None,
                encrypted_content,
                type_: claude::CompactionBlockType::Compaction,
                rest: Default::default(),
            }
        ))],
        other @ (openai::TypedResponseItem::FileSearchCall { .. }
        | openai::TypedResponseItem::ComputerCall { .. }
        | openai::TypedResponseItem::ComputerCallOutput { .. }
        | openai::TypedResponseItem::WebSearchCall { .. }
        | openai::TypedResponseItem::FunctionCallOutput { .. }
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
        | openai::TypedResponseItem::CustomToolCallOutput { .. }
        | openai::TypedResponseItem::Program { .. }
        | openai::TypedResponseItem::ProgramOutput { .. }
        | openai::TypedResponseItem::MultiAgentCall { .. }
        | openai::TypedResponseItem::MultiAgentCallOutput { .. }
        | openai::TypedResponseItem::AgentMessage { .. }
        | openai::TypedResponseItem::CompactionTrigger { .. }
        | openai::TypedResponseItem::ItemReference { .. }) => {
            if let Some(call) = items::openai_call(other.clone())? {
                vec![items::response_block(call)]
            } else {
                Vec::new()
            }
        }
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    })
}

fn string_input(input: String) -> claude::JsonObject {
    serde_json::from_str(&input).unwrap_or_else(|_| {
        let mut object = claude::JsonObject::new();
        object.insert("input".into(), input.into());
        object
    })
}

fn empty_usage() -> claude::Usage {
    crate::wire!(claude::Usage {
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
    })
}
