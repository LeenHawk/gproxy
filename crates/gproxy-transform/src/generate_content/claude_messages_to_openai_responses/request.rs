use std::collections::BTreeMap;

use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::native::items::{self, NativeKind};
use crate::common::{responses, tools};

#[allow(deprecated)] // Reading the legacy Claude output_format remains necessary on the wire.
pub(crate) fn transform(
    body: bytes::Bytes,
    model: &str,
    stream: bool,
) -> Result<bytes::Bytes, TransformError> {
    let input: claude::CreateMessageRequestBody = serde_json::from_slice(&body)?;
    if input.cache_control.is_some()
        || input.container.is_some()
        || input.context_management.is_some()
        || input.fallback_credit_token.is_some()
        || input.fallbacks.is_some()
        || input.inference_geo.is_some()
        || input.mcp_servers.is_some()
        || input.stop_sequences.is_some()
        || input.top_k.is_some()
        || input.user_profile_id.is_some()
    {
        return Err(TransformError::unsupported(
            "Claude request",
            "an unmodeled Claude-only request parameter",
        ));
    }
    let mut items = Vec::new();
    let mut native_calls = BTreeMap::new();
    for message in input.messages {
        items.extend(message_items(message, &mut native_calls)?);
    }
    let output = openai::ResponseCreateRequest {
        background: None,
        context_management: None,
        conversation: None,
        include: None,
        input: Some(openai::ResponseInput::Items(items)),
        instructions: input.system.map(system_text).transpose()?,
        max_output_tokens: Some(input.max_tokens.min(u64::from(u32::MAX)) as u32),
        max_tool_calls: None,
        metadata: input.metadata.and_then(|metadata| {
            metadata.user_id.map(|user_id| {
                [("user_id".into(), user_id)]
                    .into_iter()
                    .collect::<openai::Metadata>()
            })
        }),
        model: Some(model.into()),
        moderation: None,
        multi_agent: None,
        parallel_tool_calls: parallel(&input.tool_choice),
        previous_response_id: input
            .diagnostics
            .and_then(|diagnostics| diagnostics.previous_message_id.flatten()),
        prompt_cache_key: None,
        prompt_cache_options: None,
        prompt_cache_retention: None,
        prompt: None,
        reasoning: reasoning(input.output_config.as_ref(), input.thinking.as_ref())?,
        safety_identifier: None,
        service_tier: service_tier(input.service_tier, input.speed)?,
        store: None,
        stream: Some(stream),
        stream_options: None,
        temperature: input.temperature,
        text: text_config(input.output_config.as_ref(), input.output_format.as_ref())?,
        tool_choice: tool_choice(input.tool_choice)?,
        tools: tools::claude_to_responses(input.tools)?,
        top_logprobs: None,
        top_p: input.top_p,
        truncation: None,
        user: None,
        rest: input.rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

#[allow(deprecated)]
pub(crate) fn count_tokens(
    input: claude::CountTokensRequestBody,
    model: &str,
) -> Result<openai::ResponseInputTokensRequest, TransformError> {
    if input.cache_control.is_some()
        || input.context_management.is_some()
        || input.mcp_servers.is_some()
    {
        return Err(TransformError::unsupported(
            "Claude count-tokens request",
            "an unmodeled Claude-only request parameter",
        ));
    }
    let mut items = Vec::new();
    let mut native_calls = BTreeMap::new();
    for message in input.messages {
        items.extend(message_items(message, &mut native_calls)?);
    }
    Ok(openai::ResponseInputTokensRequest {
        conversation: None,
        input: Some(openai::ResponseInput::Items(items)),
        instructions: input.system.map(system_text).transpose()?,
        model: Some(model.into()),
        parallel_tool_calls: parallel(&input.tool_choice),
        personality: None,
        previous_response_id: input
            .diagnostics
            .and_then(|diagnostics| diagnostics.previous_message_id.flatten()),
        prompt_cache_options: None,
        reasoning: reasoning(input.output_config.as_ref(), input.thinking.as_ref())?,
        service_tier: service_tier(input.service_tier, input.speed)?,
        text: text_config(input.output_config.as_ref(), input.output_format.as_ref())?,
        tool_choice: tool_choice(input.tool_choice)?,
        tools: tools::claude_to_responses(input.tools)?,
        truncation: None,
        rest: input.rest,
    })
}

fn message_items(
    message: claude::MessageParam,
    native_calls: &mut BTreeMap<String, NativeKind>,
) -> Result<Vec<openai::ResponseItem>, TransformError> {
    let role = match &message.role {
        claude::MessageRole::Known(claude::MessageRoleKnown::User)
        | claude::MessageRole::Unknown(_) => openai::ResponseEasyInputMessageRole::User,
        claude::MessageRole::Known(claude::MessageRoleKnown::Assistant) => {
            openai::ResponseEasyInputMessageRole::Assistant
        }
        claude::MessageRole::Known(claude::MessageRoleKnown::System) => {
            openai::ResponseEasyInputMessageRole::Developer
        }
        _ => {
            return Err(TransformError::unsupported("Claude role", "future role"));
        }
    };
    let blocks = match message.content {
        claude::StringOrArray::String(text) => {
            vec![claude::ContentBlockParam::Text(claude::TextBlock {
                text,
                type_: claude::TextBlockType::Text,
                cache_control: None,
                citations: None,
                rest: Default::default(),
            })]
        }
        claude::StringOrArray::Array(blocks) => blocks,
        claude::StringOrArray::Raw(raw) => return Ok(vec![openai::ResponseItem::Unknown(raw)]),
        _ => {
            return Err(TransformError::unsupported(
                "Claude content",
                "future content shape",
            ));
        }
    };
    let mut output = Vec::new();
    let mut message_blocks = Vec::new();
    for block in blocks {
        match block {
            claude::ContentBlockParam::Text(_)
            | claude::ContentBlockParam::Image(_)
            | claude::ContentBlockParam::Document(_)
            | claude::ContentBlockParam::Raw(_) => message_blocks.push(block),
            claude::ContentBlockParam::ToolUse(block) => {
                let call_id = block.id.clone();
                let (item, kind) = items::claude_call(
                    block.id,
                    block.input,
                    block.name,
                    block.rest,
                    openai::ResponseItemLifecycleStatus::Completed,
                )?;
                if let Some(kind) = kind {
                    native_calls.insert(call_id, kind);
                }
                output.push(openai::ResponseItem::Typed(Box::new(item)));
            }
            claude::ContentBlockParam::ToolResult(block) => {
                let item = if let Some(kind) = native_calls.get(&block.tool_use_id).copied() {
                    openai::ResponseItem::Typed(Box::new(items::claude_result(block, kind)?))
                } else {
                    function_output(block)?
                };
                output.push(item);
            }
            claude::ContentBlockParam::Thinking(block) => output.push(reasoning_item(block)?),
            claude::ContentBlockParam::RedactedThinking(block) => {
                output.push(redacted_reasoning_item(block)?);
            }
            claude::ContentBlockParam::Compaction(mut block) => {
                let encrypted_content = block.encrypted_content.ok_or_else(|| {
                    TransformError::shape("Claude compaction block", "encrypted_content is missing")
                })?;
                output.push(openai::ResponseItem::Typed(Box::new(
                    openai::TypedResponseItem::Compaction {
                        encrypted_content,
                        id: take_item_id(&mut block.rest)?,
                        created_by: None,
                        rest: block.rest,
                    },
                )));
            }
            other => {
                return Err(TransformError::unsupported(
                    "Claude content block",
                    serde_json::to_string(&other)?,
                ));
            }
        }
    }
    if !message_blocks.is_empty() {
        let content = responses::claude_to_input(message_blocks)?;
        output.insert(
            0,
            openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(
                openai::ResponseEasyInputMessageItem {
                    type_: Some(openai::ResponseMessageItemType::Message),
                    role,
                    content: openai::ResponseEasyInputContent::Parts(content),
                    phase: None,
                    rest: message.rest,
                },
            )),
        );
    }
    Ok(output)
}

fn function_output(block: claude::ToolResultBlock) -> Result<openai::ResponseItem, TransformError> {
    Ok(openai::ResponseItem::Typed(Box::new(
        openai::TypedResponseItem::FunctionCallOutput {
            call_id: block.tool_use_id,
            output: tool_output(block.content)?,
            id: None,
            caller: None,
            name: None,
            namespace: None,
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            created_by: None,
            rest: block.rest,
        },
    )))
}

fn reasoning_item(
    mut block: claude::ThinkingBlock,
) -> Result<openai::ResponseItem, TransformError> {
    let id = take_item_id(&mut block.rest)?;
    Ok(openai::ResponseItem::Typed(Box::new(
        openai::TypedResponseItem::Reasoning {
            id,
            summary: Vec::new(),
            content: Some(vec![openai::ResponseReasoningTextPart {
                type_: openai::ResponseReasoningTextType::ReasoningText,
                text: block.thinking,
                rest: Default::default(),
            }]),
            encrypted_content: block.signature,
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            rest: block.rest,
        },
    )))
}

fn redacted_reasoning_item(
    mut block: claude::RedactedThinkingBlock,
) -> Result<openai::ResponseItem, TransformError> {
    let id = take_item_id(&mut block.rest)?;
    Ok(openai::ResponseItem::Typed(Box::new(
        openai::TypedResponseItem::Reasoning {
            id,
            summary: Vec::new(),
            content: None,
            encrypted_content: Some(block.data),
            status: Some(openai::ResponseItemLifecycleStatus::Completed),
            rest: block.rest,
        },
    )))
}

fn tool_output(
    content: Option<claude::ToolResultContent>,
) -> Result<openai::ResponseOutput, TransformError> {
    Ok(match content {
        None => {
            return Err(TransformError::shape(
                "Claude tool result",
                "content is missing",
            ));
        }
        Some(claude::ToolResultContent::Text(text)) => openai::ResponseOutput::Text(text),
        Some(claude::ToolResultContent::Blocks(blocks)) => {
            let mut output = Vec::new();
            for block in blocks {
                let block: claude::ContentBlockParam =
                    serde_json::from_value(serde_json::to_value(block)?)?;
                output.extend(responses::claude_to_input(vec![block])?);
            }
            openai::ResponseOutput::Parts(output)
        }
        Some(claude::ToolResultContent::Raw(raw)) => openai::ResponseOutput::Unknown(raw),
        Some(_) => {
            return Err(TransformError::unsupported(
                "Claude tool output",
                "future output shape",
            ));
        }
    })
}

fn take_item_id(
    rest: &mut serde_json::Map<String, serde_json::Value>,
) -> Result<Option<String>, TransformError> {
    rest.remove("openai_item_id")
        .map(serde_json::from_value)
        .transpose()
        .map_err(Into::into)
}

fn system_text(system: claude::SystemPrompt) -> Result<String, TransformError> {
    match system {
        claude::StringOrArray::String(text) => Ok(text),
        claude::StringOrArray::Array(blocks) => Ok(blocks
            .into_iter()
            .map(|block| block.text)
            .collect::<String>()),
        claude::StringOrArray::Raw(raw) => Err(TransformError::unsupported(
            "Claude system",
            raw.to_string(),
        )),
        _ => Err(TransformError::unsupported(
            "Claude system",
            "future system shape",
        )),
    }
}

fn tool_choice(
    choice: Option<claude::ToolChoice>,
) -> Result<Option<openai::ResponseToolChoice>, TransformError> {
    Ok(match choice {
        None => None,
        Some(claude::ToolChoice::Auto(_)) => Some(openai::ResponseToolChoice::Mode(
            openai::ToolChoiceMode::Auto,
        )),
        Some(claude::ToolChoice::Any(_)) => Some(openai::ResponseToolChoice::Mode(
            openai::ToolChoiceMode::Required,
        )),
        Some(claude::ToolChoice::None(_)) => Some(openai::ResponseToolChoice::Mode(
            openai::ToolChoiceMode::None,
        )),
        Some(claude::ToolChoice::Tool(choice)) => Some(openai::ResponseToolChoice::Function(
            openai::ResponseFunctionToolChoice {
                type_: openai::FunctionToolChoiceType::Function,
                name: choice.name,
                rest: choice.rest,
            },
        )),
        Some(claude::ToolChoice::Unknown(raw)) => Some(openai::ResponseToolChoice::Unknown(raw)),
        Some(_) => {
            return Err(TransformError::unsupported(
                "Claude tool choice",
                "future choice",
            ));
        }
    })
}

fn parallel(choice: &Option<claude::ToolChoice>) -> Option<bool> {
    match choice {
        Some(claude::ToolChoice::Auto(choice)) => choice.disable_parallel_tool_use.map(|v| !v),
        Some(claude::ToolChoice::Any(choice)) => choice.disable_parallel_tool_use.map(|v| !v),
        Some(claude::ToolChoice::Tool(choice)) => choice.disable_parallel_tool_use.map(|v| !v),
        _ => None,
    }
}

fn reasoning(
    output: Option<&claude::OutputConfig>,
    thinking: Option<&claude::ThinkingConfig>,
) -> Result<Option<openai::ReasoningConfig>, TransformError> {
    let effort = output
        .and_then(|output| output.effort.as_ref())
        .map(|effort| serde_json::from_value(serde_json::to_value(effort)?))
        .transpose()?;
    let effort = effort.or(match thinking {
        Some(claude::ThinkingConfig::Disabled(_)) => Some(openai::ReasoningEffort::None),
        Some(claude::ThinkingConfig::Enabled(_) | claude::ThinkingConfig::Adaptive(_)) => {
            Some(openai::ReasoningEffort::Medium)
        }
        Some(claude::ThinkingConfig::Unknown(_)) | Some(_) | None => None,
    });
    Ok(effort.map(|effort| openai::ReasoningConfig {
        context: None,
        effort: Some(effort),
        mode: None,
        summary: None,
        generate_summary: None,
        rest: Default::default(),
    }))
}

fn text_config(
    output: Option<&claude::OutputConfig>,
    legacy: Option<&claude::JsonSchemaFormat>,
) -> Result<Option<openai::TextConfig>, TransformError> {
    let format = output.and_then(|output| output.format.as_ref()).or(legacy);
    Ok(format.map(|format| openai::TextConfig {
        format: Some(openai::ResponseFormat::JsonSchema(
            openai::JsonSchemaResponseFormat {
                type_: openai::JsonSchemaResponseFormatType::JsonSchema,
                name: "response".into(),
                schema: format.schema.clone(),
                description: None,
                strict: None,
                rest: format.rest.clone(),
            },
        )),
        verbosity: None,
        rest: Default::default(),
    }))
}

fn service_tier(
    tier: Option<claude::RequestServiceTier>,
    speed: Option<claude::Speed>,
) -> Result<Option<openai::ServiceTier>, TransformError> {
    if matches!(speed, Some(claude::Speed::Known(claude::SpeedKnown::Fast))) {
        return Ok(Some(openai::ServiceTier::Fast));
    }
    Ok(tier
        .map(|tier| serde_json::from_value(serde_json::to_value(tier)?))
        .transpose()?)
}
