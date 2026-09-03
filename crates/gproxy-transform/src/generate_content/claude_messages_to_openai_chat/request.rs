use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::{content, tools};

pub(crate) fn transform(
    body: bytes::Bytes,
    model: &str,
    stream: bool,
) -> Result<bytes::Bytes, TransformError> {
    let mut input: claude::CreateMessageRequestBody = serde_json::from_slice(&body)?;
    crate::common::claude_message_controls::apply(&mut input.messages, &mut input.output_config);
    let mut messages = Vec::new();
    if let Some(system) = input.system {
        messages.push(openai::ChatCompletionMessageParam::System(
            openai::ChatSystemMessageParam {
                role: openai::ChatSystemRole::System,
                content: content::claude_system_to_chat(system)?,
                name: None,
                rest: Default::default(),
            },
        ));
    }
    for message in input.messages {
        match message.role {
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant) => {
                messages.push(openai::ChatCompletionMessageParam::Assistant(assistant(
                    message.content,
                )?));
            }
            claude::MessageRole::Known(claude::MessageRoleKnown::System) => {
                messages.push(openai::ChatCompletionMessageParam::Developer(
                    openai::ChatDeveloperMessageParam {
                        role: openai::ChatDeveloperRole::Developer,
                        content: chat_text(message.content)?,
                        name: None,
                        rest: Default::default(),
                    },
                ));
            }
            claude::MessageRole::Known(claude::MessageRoleKnown::User) => {
                messages.extend(user(message.content)?);
            }
            claude::MessageRole::Unknown(value) => {
                return Err(TransformError::unsupported("Claude message role", value));
            }
            _ => {
                return Err(TransformError::unsupported(
                    "Claude message role",
                    "future role",
                ));
            }
        }
    }
    let output = openai::ChatCompletionRequest {
        messages,
        model: model.into(),
        audio: None,
        frequency_penalty: None,
        function_call: None,
        functions: None,
        logit_bias: None,
        logprobs: None,
        max_completion_tokens: Some(input.max_tokens.min(u64::from(u32::MAX)) as u32),
        max_tokens: None,
        metadata: None,
        modalities: None,
        moderation: None,
        n: None,
        parallel_tool_calls: parallel(&input.tool_choice),
        prediction: None,
        presence_penalty: None,
        prompt_cache_key: None,
        prompt_cache_options: None,
        prompt_cache_retention: None,
        reasoning_effort: reasoning_effort(input.output_config.as_ref(), input.thinking.as_ref())?,
        response_format: response_format(input.output_config.as_ref())?,
        safety_identifier: None,
        seed: None,
        service_tier: service_tier(input.service_tier, input.speed)?,
        stop: input.stop_sequences.map(openai::StringOrList::List),
        store: None,
        stream: Some(stream),
        stream_options: None,
        temperature: input.temperature,
        tool_choice: tools::claude_choice_to_chat(input.tool_choice)?,
        tools: tools::claude_to_chat(input.tools)?,
        top_logprobs: None,
        top_p: input.top_p,
        user: input.metadata.and_then(|metadata| metadata.user_id),
        verbosity: None,
        web_search_options: None,
        rest: Default::default(),
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

fn assistant(
    content: claude::MessageContent,
) -> Result<openai::ChatAssistantMessageParam, TransformError> {
    let blocks = blocks(content);
    let mut text = Vec::new();
    let mut reasoning = Vec::new();
    let mut calls = Vec::new();
    for block in blocks {
        match block {
            claude::ContentBlockParam::Text(block) => text.push(block.text),
            claude::ContentBlockParam::Thinking(block) => reasoning.push(block.thinking),
            claude::ContentBlockParam::RedactedThinking(_) => {}
            claude::ContentBlockParam::ToolUse(block) => calls.push(
                openai::ChatToolCall::Function(openai::ChatFunctionToolCall {
                    id: block.id,
                    type_: openai::FunctionToolChoiceType::Function,
                    function: openai::FunctionCall {
                        arguments: serde_json::to_string(&block.input)?,
                        name: block.name,
                        rest: Default::default(),
                    },
                    rest: Default::default(),
                }),
            ),
            claude::ContentBlockParam::Raw(_) => {}
            other => {
                return Err(TransformError::unsupported(
                    "Claude assistant block",
                    serde_json::to_string(&other)?,
                ));
            }
        }
    }
    Ok(openai::ChatAssistantMessageParam {
        role: openai::ChatAssistantRole::Assistant,
        content: (!text.is_empty()).then(|| openai::ChatAssistantContent::Text(text.join(""))),
        audio: None,
        function_call: None,
        name: None,
        reasoning_content: (!reasoning.is_empty()).then(|| reasoning.join("")),
        refusal: None,
        tool_calls: (!calls.is_empty()).then_some(calls),
        rest: Default::default(),
    })
}

fn user(
    content_value: claude::MessageContent,
) -> Result<Vec<openai::ChatCompletionMessageParam>, TransformError> {
    if let claude::StringOrArray::String(text) = content_value {
        return Ok(vec![openai::ChatCompletionMessageParam::User(
            openai::ChatUserMessageParam {
                role: openai::ChatUserRole::User,
                content: openai::ChatContent::Text(text),
                name: None,
                rest: Default::default(),
            },
        )]);
    }
    let mut output = Vec::new();
    let mut parts = Vec::new();
    for block in blocks(content_value) {
        match block {
            claude::ContentBlockParam::ToolResult(block) => {
                if !parts.is_empty() {
                    output.push(user_message(std::mem::take(&mut parts)));
                }
                output.push(openai::ChatCompletionMessageParam::Tool(
                    openai::ChatToolMessageParam {
                        role: openai::ChatToolRole::Tool,
                        content: tool_result(block.content)?,
                        tool_call_id: block.tool_use_id,
                        rest: Default::default(),
                    },
                ));
            }
            block => parts.extend(content::claude_user_parts(vec![block])?),
        }
    }
    if !parts.is_empty() {
        output.push(user_message(parts));
    }
    Ok(output)
}

fn user_message(parts: Vec<openai::ChatContentPart>) -> openai::ChatCompletionMessageParam {
    openai::ChatCompletionMessageParam::User(openai::ChatUserMessageParam {
        role: openai::ChatUserRole::User,
        content: openai::ChatContent::Parts(parts),
        name: None,
        rest: Default::default(),
    })
}

fn chat_text(
    content_value: claude::MessageContent,
) -> Result<openai::ChatTextContent, TransformError> {
    match content_value {
        claude::StringOrArray::String(text) => Ok(openai::ChatTextContent::Text(text)),
        claude::StringOrArray::Array(blocks) => {
            let mut parts = Vec::new();
            for block in blocks {
                match block {
                    claude::ContentBlockParam::Text(block) => {
                        parts.push(openai::ChatTextContentPart::Text(openai::ChatTextPart {
                            type_: openai::ChatTextPartType::Text,
                            text: block.text,
                            prompt_cache_breakpoint: None,
                            rest: Default::default(),
                        }));
                    }
                    claude::ContentBlockParam::Raw(_) => {}
                    other => {
                        return Err(TransformError::unsupported(
                            "Claude system block",
                            serde_json::to_string(&other)?,
                        ));
                    }
                }
            }
            Ok(openai::ChatTextContent::Parts(parts))
        }
        claude::StringOrArray::Raw(raw) => Err(TransformError::unsupported(
            "Claude message content",
            raw.to_string(),
        )),
        _ => Err(TransformError::unsupported(
            "Claude message content",
            "future content shape",
        )),
    }
}

fn tool_result(
    content: Option<claude::ToolResultContent>,
) -> Result<openai::ChatTextContent, TransformError> {
    Ok(match content {
        None => {
            return Err(TransformError::shape(
                "Claude tool result",
                "content is missing",
            ));
        }
        Some(claude::ToolResultContent::Text(text)) => openai::ChatTextContent::Text(text),
        Some(claude::ToolResultContent::Blocks(blocks)) => openai::ChatTextContent::Parts(
            blocks
                .into_iter()
                .filter_map(|block| match block {
                    claude::ToolResultContentBlock::Text(block) => Some(Ok(
                        openai::ChatTextContentPart::Text(openai::ChatTextPart {
                            type_: openai::ChatTextPartType::Text,
                            text: block.text,
                            prompt_cache_breakpoint: None,
                            rest: Default::default(),
                        }),
                    )),
                    claude::ToolResultContentBlock::Raw(_) => None,
                    _ => Some(Err(TransformError::unsupported(
                        "Claude tool result",
                        "unsupported block",
                    ))),
                })
                .collect::<Result<_, _>>()?,
        ),
        Some(claude::ToolResultContent::Raw(raw)) => {
            return Err(TransformError::unsupported(
                "Claude tool result",
                raw.to_string(),
            ));
        }
        Some(_) => {
            return Err(TransformError::unsupported(
                "Claude tool result",
                "future result shape",
            ));
        }
    })
}

fn blocks(content: claude::MessageContent) -> Vec<claude::ContentBlockParam> {
    match content {
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
        claude::StringOrArray::Raw(_) => Vec::new(),
        _ => Vec::new(),
    }
}

fn parallel(choice: &Option<claude::ToolChoice>) -> Option<bool> {
    match choice {
        Some(claude::ToolChoice::Auto(choice)) => choice.disable_parallel_tool_use.map(|v| !v),
        Some(claude::ToolChoice::Any(choice)) => choice.disable_parallel_tool_use.map(|v| !v),
        Some(claude::ToolChoice::Tool(choice)) => choice.disable_parallel_tool_use.map(|v| !v),
        _ => None,
    }
}

fn reasoning_effort(
    output: Option<&claude::OutputConfig>,
    thinking: Option<&claude::ThinkingConfig>,
) -> Result<Option<openai::ReasoningEffort>, TransformError> {
    if let Some(effort) = output.and_then(|output| output.effort.as_ref()) {
        return Ok(Some(serde_json::from_value(serde_json::to_value(effort)?)?));
    }
    Ok(thinking.map(|_| openai::ReasoningEffort::Medium))
}

fn response_format(
    output: Option<&claude::OutputConfig>,
) -> Result<Option<openai::ChatResponseFormat>, TransformError> {
    let Some(format) = output.and_then(|output| output.format.as_ref()) else {
        return Ok(None);
    };
    Ok(Some(openai::ChatResponseFormat::JsonSchema(
        openai::ChatJsonSchemaFormat {
            type_: openai::JsonSchemaResponseFormatType::JsonSchema,
            json_schema: openai::JsonSchemaFormat {
                name: "response".into(),
                description: None,
                schema: Some(format.schema.clone()),
                strict: None,
                rest: Default::default(),
            },
            rest: Default::default(),
        },
    )))
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
