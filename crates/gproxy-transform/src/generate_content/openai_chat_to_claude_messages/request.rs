use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::{content, tools};
use crate::models::common::wire_string;

#[allow(deprecated)] // The public Claude wire still carries the deprecated output_format slot.
pub(crate) fn transform(
    body: bytes::Bytes,
    model: &str,
    stream: bool,
) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ChatCompletionRequest = serde_json::from_slice(&body)?;
    let _ = wire_string(&input.model)?;
    let mid_conv_supported = supports_mid_conv_system(model);
    let last_non_system = input.messages.iter().rposition(|message| {
        !matches!(
            message,
            openai::ChatCompletionMessageParam::Developer(_)
                | openai::ChatCompletionMessageParam::System(_)
        )
    });
    let mut messages = Vec::new();
    let mut system = Vec::new();
    let mut seen_turn = false;
    for (index, message) in input.messages.into_iter().enumerate() {
        match message {
            openai::ChatCompletionMessageParam::Developer(message) => {
                push_system(
                    content::chat_text_blocks(message.content)?,
                    message.rest,
                    seen_turn,
                    mid_conv_supported,
                    last_non_system.is_some_and(|last| index > last),
                    &mut system,
                    &mut messages,
                );
            }
            openai::ChatCompletionMessageParam::System(message) => {
                push_system(
                    content::chat_text_blocks(message.content)?,
                    message.rest,
                    seen_turn,
                    mid_conv_supported,
                    last_non_system.is_some_and(|last| index > last),
                    &mut system,
                    &mut messages,
                );
            }
            openai::ChatCompletionMessageParam::User(message) => {
                seen_turn = true;
                push_message(
                    &mut messages,
                    claude::MessageRoleKnown::User,
                    content::chat_user_blocks(message.content)?,
                    message.rest,
                );
            }
            openai::ChatCompletionMessageParam::Assistant(message) => {
                seen_turn = true;
                let mut blocks = message
                    .content
                    .map(content::chat_assistant_blocks)
                    .transpose()?
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>();
                if let Some(reasoning) = message.reasoning_content.filter(|value| !value.is_empty())
                {
                    blocks.insert(
                        0,
                        claude::ContentBlockParam::Thinking(claude::ThinkingBlock {
                            signature: None,
                            thinking: reasoning,
                            type_: claude::ThinkingBlockType::Thinking,
                            rest: Default::default(),
                        }),
                    );
                }
                if message.function_call.is_some() {
                    return Err(TransformError::shape(
                        "OpenAI Chat function_call",
                        "tool call id is missing",
                    ));
                }
                if let Some(calls) = message.tool_calls {
                    for call in calls {
                        blocks.push(tool_call(call)?);
                    }
                }
                push_message(
                    &mut messages,
                    claude::MessageRoleKnown::Assistant,
                    blocks,
                    message.rest,
                );
            }
            openai::ChatCompletionMessageParam::Tool(message) => {
                seen_turn = true;
                push_message(
                    &mut messages,
                    claude::MessageRoleKnown::User,
                    vec![claude::ContentBlockParam::ToolResult(
                        claude::ToolResultBlock {
                            tool_use_id: message.tool_call_id,
                            type_: claude::ToolResultBlockType::ToolResult,
                            cache_control: None,
                            content: Some(tool_result_content(message.content)?),
                            is_error: None,
                            rest: message.rest,
                        },
                    )],
                    Default::default(),
                );
            }
            openai::ChatCompletionMessageParam::Function(message) => {
                seen_turn = true;
                let content = message.content.ok_or_else(|| {
                    TransformError::unsupported("OpenAI Chat function message", "null content")
                })?;
                push_message(
                    &mut messages,
                    claude::MessageRoleKnown::User,
                    content::chat_text_blocks(openai::ChatTextContent::Text(format!(
                        "function:{}\n{}",
                        message.name, content
                    )))?,
                    message.rest,
                );
            }
            openai::ChatCompletionMessageParam::Unknown(raw) => {
                return Err(TransformError::unsupported(
                    "OpenAI Chat message",
                    raw.to_string(),
                ));
            }
        }
    }
    let service_tier_value = input.service_tier.clone();
    let max_tokens = input
        .max_completion_tokens
        .or(input.max_tokens)
        .map(u64::from)
        .unwrap_or(crate::common::DEFAULT_CLAUDE_MAX_TOKENS);
    let output = claude::CreateMessageRequestBody {
        model: model.to_owned().into(),
        messages,
        max_tokens,
        cache_control: None,
        container: None,
        context_management: None,
        diagnostics: None,
        fallback_credit_token: None,
        fallbacks: None,
        inference_geo: None,
        mcp_servers: None,
        metadata: input.user.map(|user_id| claude::Metadata {
            user_id: Some(user_id),
            rest: Default::default(),
        }),
        output_config: output_config(input.response_format, input.reasoning_effort)?,
        output_format: None,
        service_tier: service_tier(service_tier_value)?,
        speed: speed(input.service_tier)?,
        stop_sequences: input.stop.map(stop_sequences),
        stream: Some(stream),
        system: (!system.is_empty()).then_some(claude::StringOrArray::Array(system)),
        temperature: input.temperature,
        thinking: None,
        tool_choice: tools::chat_choice_to_claude(input.tool_choice, input.parallel_tool_calls)?,
        tools: tools::chat_to_claude(input.tools)?,
        top_k: None,
        top_p: input.top_p,
        user_profile_id: None,
        rest: input.rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

fn push_system(
    blocks: Vec<claude::ContentBlockParam>,
    rest: openai::Rest,
    seen_turn: bool,
    mid_conv_supported: bool,
    trailing: bool,
    system: &mut Vec<claude::TextBlock>,
    messages: &mut Vec<claude::MessageParam>,
) {
    if seen_turn {
        let role = if mid_conv_supported {
            claude::MessageRoleKnown::System
        } else if trailing {
            claude::MessageRoleKnown::User
        } else {
            claude::MessageRoleKnown::Assistant
        };
        push_message(messages, role, blocks, rest);
    } else {
        system.extend(blocks.into_iter().filter_map(|block| match block {
            claude::ContentBlockParam::Text(block) => Some(block),
            _ => None,
        }));
    }
}

fn supports_mid_conv_system(model: &str) -> bool {
    const PRE_OPUS_48: &[&str] = &[
        "claude-instant",
        "claude-1",
        "claude-2",
        "claude-3",
        "claude-sonnet-4",
        "claude-haiku-4",
        "claude-4-",
        "claude-opus-4-0",
        "claude-opus-4-1",
        "claude-opus-4-2",
        "claude-opus-4-3",
        "claude-opus-4-4",
        "claude-opus-4-5",
        "claude-opus-4-6",
        "claude-opus-4-7",
        "claude-opus-4@",
        "claude-sonnet-5",
    ];
    let model = model.to_ascii_lowercase();
    !PRE_OPUS_48.iter().any(|pattern| model.contains(pattern))
}

fn push_message(
    messages: &mut Vec<claude::MessageParam>,
    role: claude::MessageRoleKnown,
    blocks: Vec<claude::ContentBlockParam>,
    rest: serde_json::Map<String, serde_json::Value>,
) {
    if !blocks.is_empty() {
        messages.push(claude::MessageParam {
            role: claude::MessageRole::Known(role),
            content: claude::StringOrArray::Array(blocks),
            rest,
        });
    }
}

fn function_call(
    id: String,
    call: openai::FunctionCall,
) -> Result<claude::ContentBlockParam, TransformError> {
    let input = serde_json::from_str(&call.arguments).unwrap_or_default();
    Ok(claude::ContentBlockParam::ToolUse(claude::ToolUseBlock {
        id,
        input,
        name: call.name,
        type_: claude::ToolUseBlockType::ToolUse,
        cache_control: None,
        caller: None,
        rest: call.rest,
    }))
}

fn tool_call(call: openai::ChatToolCall) -> Result<claude::ContentBlockParam, TransformError> {
    match call {
        openai::ChatToolCall::Function(call) => function_call(call.id, call.function),
        openai::ChatToolCall::Custom(call) => {
            let input = serde_json::from_str(&call.custom.input).unwrap_or_default();
            Ok(claude::ContentBlockParam::ToolUse(claude::ToolUseBlock {
                id: call.id,
                input,
                name: call.custom.name,
                type_: claude::ToolUseBlockType::ToolUse,
                cache_control: None,
                caller: None,
                rest: call.rest,
            }))
        }
        openai::ChatToolCall::Unknown(raw) => Ok(claude::ContentBlockParam::Raw(raw)),
    }
}

fn tool_result_content(
    content: openai::ChatTextContent,
) -> Result<claude::ToolResultContent, TransformError> {
    match content {
        openai::ChatTextContent::Text(text) => Ok(claude::ToolResultContent::Text(text)),
        openai::ChatTextContent::Parts(parts) => Ok(claude::ToolResultContent::Blocks(
            parts
                .into_iter()
                .map(|part| match part {
                    openai::ChatTextContentPart::Text(part) => {
                        Ok(claude::ToolResultContentBlock::Text(claude::TextBlock {
                            text: part.text,
                            type_: claude::TextBlockType::Text,
                            cache_control: None,
                            citations: None,
                            rest: part.rest,
                        }))
                    }
                    openai::ChatTextContentPart::Unknown(raw) => {
                        Ok(claude::ToolResultContentBlock::Raw(raw))
                    }
                })
                .collect::<Result<_, TransformError>>()?,
        )),
        openai::ChatTextContent::Unknown(raw) => Ok(claude::ToolResultContent::Raw(raw)),
    }
}

fn output_config(
    format: Option<openai::ChatResponseFormat>,
    effort: Option<openai::ReasoningEffort>,
) -> Result<Option<claude::OutputConfig>, TransformError> {
    let format = match format {
        Some(openai::ChatResponseFormat::JsonSchema(format)) => Some(claude::JsonSchemaFormat {
            type_: claude::JsonSchemaFormatType::Known(
                claude::JsonSchemaFormatTypeKnown::JsonSchema,
            ),
            schema: format.json_schema.schema.ok_or_else(|| {
                TransformError::shape("OpenAI JSON schema response format", "schema is missing")
            })?,
            rest: format.rest,
        }),
        Some(openai::ChatResponseFormat::Text(_)) | None => None,
        Some(other) => {
            return Err(TransformError::unsupported(
                "OpenAI response format",
                serde_json::to_string(&other)?,
            ));
        }
    };
    let effort = effort
        .map(|effort| serde_json::from_value(serde_json::to_value(effort)?))
        .transpose()?;
    Ok(
        (format.is_some() || effort.is_some()).then_some(claude::OutputConfig {
            effort,
            format,
            task_budget: None,
            rest: Default::default(),
        }),
    )
}

fn stop_sequences(stop: openai::StringOrList) -> Vec<String> {
    match stop {
        openai::StringOrList::String(stop) => vec![stop],
        openai::StringOrList::List(stops) => stops,
    }
}

fn service_tier(
    tier: Option<openai::ServiceTier>,
) -> Result<Option<claude::RequestServiceTier>, TransformError> {
    match tier {
        None => Ok(None),
        Some(openai::ServiceTier::Auto | openai::ServiceTier::Default) => Ok(Some(
            claude::RequestServiceTier::Known(claude::RequestServiceTierKnown::Auto),
        )),
        Some(openai::ServiceTier::Unknown(value)) => {
            Ok(Some(claude::RequestServiceTier::Unknown(value)))
        }
        Some(_) => Ok(None),
    }
}

fn speed(tier: Option<openai::ServiceTier>) -> Result<Option<claude::Speed>, TransformError> {
    Ok(match tier {
        Some(
            openai::ServiceTier::Fast
            | openai::ServiceTier::Priority
            | openai::ServiceTier::Ultrafast,
        ) => Some(claude::Speed::Known(claude::SpeedKnown::Fast)),
        _ => None,
    })
}
