use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::{responses, tools};

mod items;

#[allow(deprecated)] // The public Claude wire still carries the deprecated output_format slot.
pub(crate) fn transform(
    body: bytes::Bytes,
    model: &str,
    stream: bool,
) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ResponseCreateRequest = serde_json::from_slice(&body)?;
    let max_tokens = input
        .max_output_tokens
        .map(u64::from)
        .unwrap_or(crate::common::DEFAULT_CLAUDE_MAX_TOKENS);
    let (messages, system) = promote_system(
        input_messages(input.input)?,
        input.instructions.map(claude::StringOrArray::String),
    )?;
    let output = claude::CreateMessageRequestBody {
        model: model.to_owned().into(),
        messages,
        max_tokens,
        cache_control: None,
        container: None,
        context_management: None,
        diagnostics: input
            .previous_response_id
            .map(|id| claude::DiagnosticsParam {
                previous_message_id: Some(Some(id)),
                rest: Default::default(),
            }),
        fallback_credit_token: None,
        fallbacks: None,
        inference_geo: None,
        mcp_servers: None,
        metadata: input.metadata.and_then(|metadata| {
            metadata
                .get("user_id")
                .cloned()
                .map(|user_id| claude::Metadata {
                    user_id: Some(user_id),
                    rest: Default::default(),
                })
        }),
        output_config: output_config(input.reasoning, input.text)?,
        output_format: None,
        service_tier: service_tier(input.service_tier)?,
        speed: None,
        stop_sequences: None,
        stream: Some(stream),
        system,
        temperature: input.temperature,
        thinking: None,
        tool_choice: tool_choice(input.tool_choice, input.parallel_tool_calls)?,
        tools: tools::responses_to_claude(input.tools)?,
        top_k: None,
        top_p: input.top_p,
        user_profile_id: None,
        rest: Default::default(),
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

fn promote_system(
    messages: Vec<claude::MessageParam>,
    initial: Option<claude::SystemPrompt>,
) -> Result<(Vec<claude::MessageParam>, Option<claude::SystemPrompt>), TransformError> {
    let mut system = match initial {
        Some(claude::StringOrArray::String(text)) => text,
        None => String::new(),
        Some(_) => {
            return Err(TransformError::unsupported(
                "Responses instructions",
                "future system shape",
            ));
        }
    };
    let mut retained = Vec::new();
    for message in messages {
        if message.role != claude::MessageRole::Known(claude::MessageRoleKnown::System) {
            retained.push(message);
            continue;
        }
        let text = match message.content {
            claude::StringOrArray::String(text) => text,
            claude::StringOrArray::Array(blocks) => blocks
                .into_iter()
                .filter_map(|block| match block {
                    claude::ContentBlockParam::Text(block) => Some(block.text),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
            claude::StringOrArray::Raw(raw) => {
                return Err(TransformError::unsupported(
                    "Responses system message",
                    raw.to_string(),
                ));
            }
            _ => {
                return Err(TransformError::unsupported(
                    "Responses system message",
                    "future content shape",
                ));
            }
        };
        if !system.is_empty() && !text.is_empty() {
            system.push('\n');
        }
        system.push_str(&text);
    }
    Ok((
        retained,
        (!system.is_empty()).then_some(claude::StringOrArray::String(system)),
    ))
}

#[allow(deprecated)]
pub(crate) fn count_tokens(
    input: openai::ResponseInputTokensRequest,
    model: &str,
) -> Result<claude::CountTokensRequestBody, TransformError> {
    Ok(claude::CountTokensRequestBody {
        model: model.to_owned().into(),
        messages: input_messages(input.input)?,
        cache_control: None,
        context_management: None,
        diagnostics: input
            .previous_response_id
            .map(|id| claude::DiagnosticsParam {
                previous_message_id: Some(Some(id)),
                rest: Default::default(),
            }),
        mcp_servers: None,
        output_config: output_config(input.reasoning, input.text)?,
        output_format: None,
        service_tier: service_tier(input.service_tier)?,
        speed: None,
        system: input.instructions.map(claude::StringOrArray::String),
        thinking: None,
        tool_choice: tool_choice(input.tool_choice, input.parallel_tool_calls)?,
        tools: tools::responses_to_claude(input.tools)?,
        rest: Default::default(),
    })
}

fn input_messages(
    input: Option<openai::ResponseInput>,
) -> Result<Vec<claude::MessageParam>, TransformError> {
    match input {
        None => Ok(Vec::new()),
        Some(openai::ResponseInput::Text(text)) => Ok(vec![message(
            claude::MessageRoleKnown::User,
            vec![text_block(text)],
            Default::default(),
        )]),
        Some(openai::ResponseInput::Items(items)) => items
            .into_iter()
            .filter_map(|item| input_item(item).transpose())
            .collect(),
        Some(openai::ResponseInput::Unknown(_)) => Ok(Vec::new()),
    }
}

fn input_item(item: openai::ResponseItem) -> Result<Option<claude::MessageParam>, TransformError> {
    match item {
        openai::ResponseItem::Message(message_item) => Ok(Some(match message_item {
            openai::ResponseMessageItem::EasyInput(message_item) => {
                let role = match message_item.role {
                    openai::ResponseEasyInputMessageRole::Assistant => {
                        claude::MessageRoleKnown::Assistant
                    }
                    openai::ResponseEasyInputMessageRole::System
                    | openai::ResponseEasyInputMessageRole::Developer => {
                        claude::MessageRoleKnown::System
                    }
                    openai::ResponseEasyInputMessageRole::User => claude::MessageRoleKnown::User,
                };
                let blocks = match message_item.content {
                    openai::ResponseEasyInputContent::Text(text) => vec![text_block(text)],
                    openai::ResponseEasyInputContent::Parts(parts) => {
                        responses::input_to_claude(parts)?
                    }
                    openai::ResponseEasyInputContent::OutputParts(parts) => {
                        responses::output_to_claude(parts)?
                    }
                    openai::ResponseEasyInputContent::Unknown(raw) => {
                        vec![claude::ContentBlockParam::Raw(raw)]
                    }
                };
                message(role, blocks, Default::default())
            }
            openai::ResponseMessageItem::Input(message_item) => {
                let role = match message_item.role {
                    openai::ResponseInputMessageRole::User => claude::MessageRoleKnown::User,
                    openai::ResponseInputMessageRole::System
                    | openai::ResponseInputMessageRole::Developer => {
                        claude::MessageRoleKnown::System
                    }
                };
                message(
                    role,
                    responses::input_to_claude(message_item.content)?,
                    with_item_id(message_item.rest, message_item.id),
                )
            }
            openai::ResponseMessageItem::Output(message_item) => message(
                claude::MessageRoleKnown::Assistant,
                responses::output_to_claude(message_item.content)?,
                with_item_id(message_item.rest, Some(message_item.id)),
            ),
            openai::ResponseMessageItem::Unknown(_) => return Ok(None),
        })),
        openai::ResponseItem::Typed(item) => match items::typed_item(*item) {
            Ok(message) => Ok(Some(message)),
            Err(TransformError::Unsupported { .. }) => Ok(None),
            Err(error) => Err(error),
        },
        openai::ResponseItem::Unknown(_) => Ok(None),
    }
}

fn with_item_id(
    _: serde_json::Map<String, serde_json::Value>,
    _: Option<String>,
) -> serde_json::Map<String, serde_json::Value> {
    Default::default()
}

fn preserve_item_id(rest: &mut serde_json::Map<String, serde_json::Value>, id: Option<String>) {
    let _ = id;
    rest.remove("openai_item_id");
}

fn message(
    role: claude::MessageRoleKnown,
    content: Vec<claude::ContentBlockParam>,
    rest: serde_json::Map<String, serde_json::Value>,
) -> claude::MessageParam {
    claude::MessageParam {
        role: claude::MessageRole::Known(role),
        content: claude::StringOrArray::Array(content),
        clear_at: None,
        output_config: None,
        rest,
    }
}

fn text_block(text: String) -> claude::ContentBlockParam {
    claude::ContentBlockParam::Text(claude::TextBlock {
        text,
        type_: claude::TextBlockType::Text,
        cache_control: None,
        citations: None,
        rest: Default::default(),
    })
}

fn tool_choice(
    choice: Option<openai::ResponseToolChoice>,
    parallel: Option<bool>,
) -> Result<Option<claude::ToolChoice>, TransformError> {
    let disable_parallel_tool_use = parallel.map(|parallel| !parallel);
    Ok(match choice {
        None => None,
        Some(openai::ResponseToolChoice::Mode(openai::ToolChoiceMode::Auto)) => {
            Some(claude::ToolChoice::Auto(claude::ToolChoiceAuto {
                type_: claude::ToolChoiceAutoType::Auto,
                disable_parallel_tool_use,
                rest: Default::default(),
            }))
        }
        Some(openai::ResponseToolChoice::Mode(openai::ToolChoiceMode::Required)) => {
            Some(claude::ToolChoice::Any(claude::ToolChoiceAny {
                type_: claude::ToolChoiceAnyType::Any,
                disable_parallel_tool_use,
                rest: Default::default(),
            }))
        }
        Some(openai::ResponseToolChoice::Mode(openai::ToolChoiceMode::None)) => {
            Some(claude::ToolChoice::None(claude::ToolChoiceNone {
                type_: claude::ToolChoiceNoneType::None,
                rest: Default::default(),
            }))
        }
        Some(openai::ResponseToolChoice::Function(choice)) => {
            Some(claude::ToolChoice::Tool(claude::ToolChoiceTool {
                name: choice.name,
                type_: claude::ToolChoiceToolType::Tool,
                disable_parallel_tool_use,
                rest: choice.rest,
            }))
        }
        Some(openai::ResponseToolChoice::Custom(choice)) => {
            Some(claude::ToolChoice::Tool(claude::ToolChoiceTool {
                name: choice.name,
                type_: claude::ToolChoiceToolType::Tool,
                disable_parallel_tool_use,
                rest: choice.rest,
            }))
        }
        Some(openai::ResponseToolChoice::Unknown(raw)) => Some(claude::ToolChoice::Unknown(raw)),
        Some(other) => {
            return Err(TransformError::unsupported(
                "OpenAI Responses tool choice",
                serde_json::to_string(&other)?,
            ));
        }
    })
}

fn output_config(
    reasoning: Option<openai::ReasoningConfig>,
    text: Option<openai::TextConfig>,
) -> Result<Option<claude::OutputConfig>, TransformError> {
    let effort = reasoning
        .and_then(|reasoning| reasoning.effort)
        .map(|effort| serde_json::from_value(serde_json::to_value(effort)?))
        .transpose()?;
    let format = match text.and_then(|text| text.format) {
        Some(openai::ResponseFormat::JsonSchema(format)) => Some(claude::JsonSchemaFormat {
            type_: claude::JsonSchemaFormatType::Known(
                claude::JsonSchemaFormatTypeKnown::JsonSchema,
            ),
            schema: format.schema,
            rest: format.rest,
        }),
        Some(openai::ResponseFormat::Text(_)) | None => None,
        Some(other) => {
            return Err(TransformError::unsupported(
                "OpenAI Responses format",
                serde_json::to_string(&other)?,
            ));
        }
    };
    Ok(
        (effort.is_some() || format.is_some()).then_some(claude::OutputConfig {
            effort,
            format,
            task_budget: None,
            rest: Default::default(),
        }),
    )
}

fn service_tier(
    tier: Option<openai::ServiceTier>,
) -> Result<Option<claude::RequestServiceTier>, TransformError> {
    Ok(match tier {
        Some(openai::ServiceTier::Auto | openai::ServiceTier::Default) => Some(
            claude::RequestServiceTier::Known(claude::RequestServiceTierKnown::Auto),
        ),
        Some(openai::ServiceTier::Unknown(value)) => {
            Some(claude::RequestServiceTier::Unknown(value))
        }
        _ => None,
    })
}
