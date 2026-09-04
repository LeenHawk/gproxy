use gproxy_protocol::openai;

use crate::TransformError;

use super::content::*;
use crate::common::tools;

pub(crate) fn transform(
    body: bytes::Bytes,
    model: &str,
    stream: bool,
) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ResponseCreateRequest = serde_json::from_slice(&body)?;
    let output = transform_typed(input, model, stream)?;
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

pub(crate) fn transform_typed(
    input: openai::ResponseCreateRequest,
    model: &str,
    stream: bool,
) -> Result<openai::ChatCompletionRequest, TransformError> {
    let mut messages = Vec::new();
    match input.input {
        Some(openai::ResponseInput::Text(text)) => messages.push(user_text(text)),
        Some(openai::ResponseInput::Items(items)) => {
            messages.extend(items_to_messages(items)?);
        }
        Some(openai::ResponseInput::Unknown(_)) => {}
        None => {}
    }
    if let Some(instructions) = input.instructions {
        messages.insert(
            0,
            openai::ChatCompletionMessageParam::Developer(openai::ChatDeveloperMessageParam {
                role: openai::ChatDeveloperRole::Developer,
                content: openai::ChatTextContent::Text(instructions),
                name: None,
                rest: Default::default(),
            }),
        );
    }
    if messages.is_empty() {
        return Err(TransformError::unsupported(
            "OpenAI Responses input",
            "no representable messages",
        ));
    }
    let verbosity = input.text.as_ref().and_then(|text| text.verbosity.clone());
    let output = crate::wire!(openai::ChatCompletionRequest {
        messages,
        model: model.into(),
        audio: None,
        frequency_penalty: None,
        function_call: None,
        functions: None,
        logit_bias: None,
        logprobs: None,
        max_completion_tokens: input.max_output_tokens,
        max_tokens: None,
        metadata: input.metadata,
        modalities: None,
        moderation: input.moderation,
        n: None,
        parallel_tool_calls: input.parallel_tool_calls,
        prediction: None,
        presence_penalty: None,
        prompt_cache_key: input.prompt_cache_key,
        prompt_cache_options: input.prompt_cache_options,
        prompt_cache_retention: input.prompt_cache_retention,
        reasoning_effort: input.reasoning.and_then(|reasoning| reasoning.effort),
        response_format: response_format(input.text)?,
        safety_identifier: input.safety_identifier,
        seed: None,
        service_tier: input.service_tier,
        stop: None,
        store: input.store,
        stream: Some(stream),
        stream_options: input.stream_options.map(|options| openai::StreamOptions {
            include_obfuscation: options.include_obfuscation,
            include_usage: Some(true),
            rest: Default::default(),
        }),
        temperature: input.temperature,
        tool_choice: tool_choice(input.tool_choice)?,
        tools: tools::responses_to_chat(input.tools)?,
        top_logprobs: input.top_logprobs,
        top_p: input.top_p,
        user: input.user,
        verbosity,
        web_search_options: None,
        rest: Default::default(),
    });
    Ok(output)
}

fn items_to_messages(
    items: Vec<openai::ResponseItem>,
) -> Result<Vec<openai::ChatCompletionMessageParam>, TransformError> {
    let mut messages = Vec::new();
    let mut pending_reasoning = Vec::new();
    for item in items {
        if let openai::ResponseItem::Typed(item) = &item
            && let openai::TypedResponseItem::Reasoning {
                summary, content, ..
            } = item.as_ref()
        {
            pending_reasoning.extend(summary.iter().map(|part| part.text.clone()));
            pending_reasoning.extend(content.iter().flatten().map(|part| part.text.clone()));
            continue;
        }
        let mut converted = item_messages(item)?;
        if let Some(reasoning) = joined(std::mem::take(&mut pending_reasoning))
            && !converted
                .first_mut()
                .is_some_and(|message| attach_reasoning(message, &reasoning))
        {
            messages.push(reasoning_message(reasoning));
        }
        messages.extend(converted);
    }
    if let Some(reasoning) = joined(pending_reasoning) {
        messages.push(reasoning_message(reasoning));
    }
    Ok(messages)
}

fn item_messages(
    item: openai::ResponseItem,
) -> Result<Vec<openai::ChatCompletionMessageParam>, TransformError> {
    match item {
        openai::ResponseItem::Message(openai::ResponseMessageItem::Unknown(_)) => Ok(Vec::new()),
        openai::ResponseItem::Message(message) => Ok(vec![message_to_chat(message)?]),
        openai::ResponseItem::Typed(item) => typed_messages(*item),
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

fn message_to_chat(
    message: openai::ResponseMessageItem,
) -> Result<openai::ChatCompletionMessageParam, TransformError> {
    match message {
        openai::ResponseMessageItem::EasyInput(message) => match message.role {
            openai::ResponseEasyInputMessageRole::Assistant => {
                Ok(openai::ChatCompletionMessageParam::Assistant(crate::wire!(
                    openai::ChatAssistantMessageParam {
                        role: openai::ChatAssistantRole::Assistant,
                        content: Some(easy_assistant(message.content)?),
                        audio: None,
                        function_call: None,
                        name: None,
                        reasoning_content: None,
                        refusal: None,
                        tool_calls: None,
                        rest: Default::default(),
                    }
                )))
            }
            openai::ResponseEasyInputMessageRole::System => Ok(text_message(
                message.content,
                openai::ResponseEasyInputMessageRole::System,
            )?),
            openai::ResponseEasyInputMessageRole::Developer => Ok(text_message(
                message.content,
                openai::ResponseEasyInputMessageRole::Developer,
            )?),
            openai::ResponseEasyInputMessageRole::User => Ok(user_content(message.content)?),
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
                ));
            }
        },
        openai::ResponseMessageItem::Input(message) => {
            let content = openai::ResponseEasyInputContent::Parts(message.content);
            match message.role {
                openai::ResponseInputMessageRole::User => user_content(content),
                openai::ResponseInputMessageRole::System => {
                    text_message(content, openai::ResponseEasyInputMessageRole::System)
                }
                openai::ResponseInputMessageRole::Developer => {
                    text_message(content, openai::ResponseEasyInputMessageRole::Developer)
                }
                #[cfg(not(feature = "exhaustive"))]
                _ => {
                    return Err(crate::TransformError::unsupported(
                        "protocol enum",
                        "unrecognized external variant",
                    ));
                }
            }
        }
        openai::ResponseMessageItem::Output(message) => {
            Ok(openai::ChatCompletionMessageParam::Assistant(crate::wire!(
                openai::ChatAssistantMessageParam {
                    role: openai::ChatAssistantRole::Assistant,
                    content: Some(output_content(message.content)),
                    audio: None,
                    function_call: None,
                    name: None,
                    reasoning_content: None,
                    refusal: None,
                    tool_calls: None,
                    rest: Default::default(),
                }
            )))
        }
        openai::ResponseMessageItem::Unknown(raw) => Err(TransformError::unsupported(
            "OpenAI Responses message",
            raw.to_string(),
        )),
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    }
}

fn typed_messages(
    item: openai::TypedResponseItem,
) -> Result<Vec<openai::ChatCompletionMessageParam>, TransformError> {
    Ok(match item {
        openai::TypedResponseItem::FunctionCall {
            arguments,
            call_id,
            name,
            ..
        } => vec![assistant_call(openai::ChatToolCall::Function(
            crate::wire!(openai::ChatFunctionToolCall {
                id: call_id,
                type_: openai::FunctionToolChoiceType::Function,
                function: openai::FunctionCall {
                    arguments,
                    name,
                    rest: Default::default(),
                },
                rest: Default::default(),
            }),
        ))],
        openai::TypedResponseItem::CustomToolCall {
            call_id,
            input,
            name,
            ..
        } => vec![assistant_call(openai::ChatToolCall::Custom(crate::wire!(
            openai::ChatCustomToolCall {
                id: call_id,
                type_: openai::CustomToolChoiceType::Custom,
                custom: openai::CustomToolCall {
                    input,
                    name,
                    rest: Default::default(),
                },
                rest: Default::default(),
            }
        )))],
        openai::TypedResponseItem::FunctionCallOutput {
            call_id, output, ..
        }
        | openai::TypedResponseItem::CustomToolCallOutput {
            call_id, output, ..
        } => vec![openai::ChatCompletionMessageParam::Tool(crate::wire!(
            openai::ChatToolMessageParam {
                role: openai::ChatToolRole::Tool,
                content: output_to_chat(output)?,
                tool_call_id: call_id,
                rest: Default::default(),
            }
        ))],
        openai::TypedResponseItem::ApplyPatchCall {
            call_id, operation, ..
        } => vec![assistant_call(openai::ChatToolCall::Function(
            crate::wire!(openai::ChatFunctionToolCall {
                id: call_id,
                type_: openai::FunctionToolChoiceType::Function,
                function: openai::FunctionCall {
                    arguments: serde_json::to_string(&operation)?,
                    name: "apply_patch".into(),
                    rest: Default::default(),
                },
                rest: Default::default(),
            }),
        ))],
        openai::TypedResponseItem::ApplyPatchCallOutput {
            call_id, output, ..
        } => vec![openai::ChatCompletionMessageParam::Tool(crate::wire!(
            openai::ChatToolMessageParam {
                role: openai::ChatToolRole::Tool,
                content: openai::ChatTextContent::Text(output.unwrap_or_default()),
                tool_call_id: call_id,
                rest: Default::default(),
            }
        ))],
        openai::TypedResponseItem::Reasoning { content, .. } => {
            vec![openai::ChatCompletionMessageParam::Assistant(crate::wire!(
                openai::ChatAssistantMessageParam {
                    role: openai::ChatAssistantRole::Assistant,
                    content: None,
                    audio: None,
                    function_call: None,
                    name: None,
                    reasoning_content: Some(
                        content
                            .into_iter()
                            .flatten()
                            .map(|part| part.text)
                            .collect(),
                    ),
                    refusal: None,
                    tool_calls: None,
                    rest: Default::default(),
                }
            ))]
        }
        _other @ (openai::TypedResponseItem::FileSearchCall { .. }
        | openai::TypedResponseItem::ComputerCall { .. }
        | openai::TypedResponseItem::ComputerCallOutput { .. }
        | openai::TypedResponseItem::WebSearchCall { .. }
        | openai::TypedResponseItem::ToolSearchCall { .. }
        | openai::TypedResponseItem::ToolSearchOutput { .. }
        | openai::TypedResponseItem::AdditionalTools { .. }
        | openai::TypedResponseItem::Compaction { .. }
        | openai::TypedResponseItem::ImageGenerationCall { .. }
        | openai::TypedResponseItem::CodeInterpreterCall { .. }
        | openai::TypedResponseItem::LocalShellCall { .. }
        | openai::TypedResponseItem::LocalShellCallOutput { .. }
        | openai::TypedResponseItem::ShellCall { .. }
        | openai::TypedResponseItem::ShellCallOutput { .. }
        | openai::TypedResponseItem::McpListTools { .. }
        | openai::TypedResponseItem::McpApprovalRequest { .. }
        | openai::TypedResponseItem::McpApprovalResponse { .. }
        | openai::TypedResponseItem::McpCall { .. }
        | openai::TypedResponseItem::Program { .. }
        | openai::TypedResponseItem::ProgramOutput { .. }
        | openai::TypedResponseItem::MultiAgentCall { .. }
        | openai::TypedResponseItem::MultiAgentCallOutput { .. }
        | openai::TypedResponseItem::AgentMessage { .. }
        | openai::TypedResponseItem::ConfigurationUpdate { .. }
        | openai::TypedResponseItem::CompactionTrigger { .. }
        | openai::TypedResponseItem::ItemReference { .. }) => Vec::new(),
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    })
}

fn attach_reasoning(message: &mut openai::ChatCompletionMessageParam, reasoning: &str) -> bool {
    let openai::ChatCompletionMessageParam::Assistant(message) = message else {
        return false;
    };
    match &mut message.reasoning_content {
        Some(existing) if !existing.is_empty() => {
            existing.push('\n');
            existing.push_str(reasoning);
        }
        value => *value = Some(reasoning.to_owned()),
    }
    true
}

fn reasoning_message(reasoning: String) -> openai::ChatCompletionMessageParam {
    openai::ChatCompletionMessageParam::Assistant(crate::wire!(openai::ChatAssistantMessageParam {
        role: openai::ChatAssistantRole::Assistant,
        content: None,
        audio: None,
        function_call: None,
        name: None,
        reasoning_content: Some(reasoning),
        refusal: None,
        tool_calls: None,
        rest: Default::default(),
    }))
}

fn joined(parts: Vec<String>) -> Option<String> {
    let value = parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    (!value.is_empty()).then_some(value)
}

fn assistant_call(call: openai::ChatToolCall) -> openai::ChatCompletionMessageParam {
    openai::ChatCompletionMessageParam::Assistant(crate::wire!(openai::ChatAssistantMessageParam {
        role: openai::ChatAssistantRole::Assistant,
        content: None,
        audio: None,
        function_call: None,
        name: None,
        reasoning_content: None,
        refusal: None,
        tool_calls: Some(vec![call]),
        rest: Default::default(),
    }))
}

fn tool_choice(
    choice: Option<openai::ResponseToolChoice>,
) -> Result<Option<openai::ChatToolChoice>, TransformError> {
    Ok(match choice {
        None => None,
        Some(openai::ResponseToolChoice::Mode(mode)) => Some(openai::ChatToolChoice::Mode(mode)),
        Some(openai::ResponseToolChoice::Function(choice)) => Some(openai::ChatToolChoice::Named(
            openai::ChatNamedToolChoice::Function(crate::wire!(
                openai::ChatNamedFunctionToolChoice {
                    type_: openai::FunctionToolChoiceType::Function,
                    function: openai::NamedTool {
                        name: choice.name,
                        rest: Default::default(),
                    },
                    rest: Default::default(),
                }
            )),
        )),
        Some(openai::ResponseToolChoice::Custom(choice)) => Some(openai::ChatToolChoice::Named(
            openai::ChatNamedToolChoice::Custom(crate::wire!(openai::ChatNamedCustomToolChoice {
                type_: openai::CustomToolChoiceType::Custom,
                custom: openai::NamedTool {
                    name: choice.name,
                    rest: Default::default(),
                },
                rest: Default::default(),
            })),
        )),
        Some(openai::ResponseToolChoice::Unknown(_)) => None,
        Some(_) => None,
    })
}

fn response_format(
    text: Option<openai::TextConfig>,
) -> Result<Option<openai::ChatResponseFormat>, TransformError> {
    text.and_then(|text| text.format)
        .map(|format| serde_json::from_slice(&serde_json::to_vec(&format)?))
        .transpose()
        .map_err(TransformError::from)
}
