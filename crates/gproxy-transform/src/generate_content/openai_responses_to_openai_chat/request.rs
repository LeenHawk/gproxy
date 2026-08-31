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
    let mut messages = Vec::new();
    match input.input {
        Some(openai::ResponseInput::Text(text)) => messages.push(user_text(text)),
        Some(openai::ResponseInput::Items(items)) => {
            messages.extend(items_to_messages(items)?);
        }
        Some(openai::ResponseInput::Unknown(raw)) => {
            messages.push(openai::ChatCompletionMessageParam::Unknown(raw));
        }
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
    let verbosity = input.text.as_ref().and_then(|text| text.verbosity.clone());
    let output = openai::ChatCompletionRequest {
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
            rest: options.rest,
        }),
        temperature: input.temperature,
        tool_choice: tool_choice(input.tool_choice)?,
        tools: tools::responses_to_chat(input.tools)?,
        top_logprobs: input.top_logprobs,
        top_p: input.top_p,
        user: input.user,
        verbosity,
        web_search_options: None,
        rest: input.rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
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
        openai::ResponseItem::Unknown(raw) => {
            let _ = raw;
            Ok(Vec::new())
        }
    }
}

fn message_to_chat(
    message: openai::ResponseMessageItem,
) -> Result<openai::ChatCompletionMessageParam, TransformError> {
    match message {
        openai::ResponseMessageItem::EasyInput(message) => match message.role {
            openai::ResponseEasyInputMessageRole::Assistant => Ok(
                openai::ChatCompletionMessageParam::Assistant(openai::ChatAssistantMessageParam {
                    role: openai::ChatAssistantRole::Assistant,
                    content: Some(easy_assistant(message.content)?),
                    audio: None,
                    function_call: None,
                    name: None,
                    reasoning_content: None,
                    refusal: None,
                    tool_calls: None,
                    rest: message.rest,
                }),
            ),
            openai::ResponseEasyInputMessageRole::System => Ok(text_message(
                message.content,
                openai::ResponseEasyInputMessageRole::System,
                message.rest,
            )?),
            openai::ResponseEasyInputMessageRole::Developer => Ok(text_message(
                message.content,
                openai::ResponseEasyInputMessageRole::Developer,
                message.rest,
            )?),
            openai::ResponseEasyInputMessageRole::User => {
                Ok(user_content(message.content, message.rest)?)
            }
        },
        openai::ResponseMessageItem::Input(message) => {
            let content = openai::ResponseEasyInputContent::Parts(message.content);
            match message.role {
                openai::ResponseInputMessageRole::User => user_content(content, message.rest),
                openai::ResponseInputMessageRole::System => text_message(
                    content,
                    openai::ResponseEasyInputMessageRole::System,
                    message.rest,
                ),
                openai::ResponseInputMessageRole::Developer => text_message(
                    content,
                    openai::ResponseEasyInputMessageRole::Developer,
                    message.rest,
                ),
            }
        }
        openai::ResponseMessageItem::Output(message) => Ok(
            openai::ChatCompletionMessageParam::Assistant(openai::ChatAssistantMessageParam {
                role: openai::ChatAssistantRole::Assistant,
                content: Some(output_content(message.content)),
                audio: None,
                function_call: None,
                name: None,
                reasoning_content: None,
                refusal: None,
                tool_calls: None,
                rest: message.rest,
            }),
        ),
        openai::ResponseMessageItem::Unknown(raw) => {
            let _ = raw;
            Ok(openai::ChatCompletionMessageParam::User(
                openai::ChatUserMessageParam {
                    role: openai::ChatUserRole::User,
                    content: openai::ChatContent::Text(String::new()),
                    name: None,
                    rest: Default::default(),
                },
            ))
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
            id: _,
            rest,
            ..
        } => vec![assistant_call(openai::ChatToolCall::Function(
            openai::ChatFunctionToolCall {
                id: call_id,
                type_: openai::FunctionToolChoiceType::Function,
                function: openai::FunctionCall {
                    arguments,
                    name,
                    rest: Default::default(),
                },
                rest,
            },
        ))],
        openai::TypedResponseItem::CustomToolCall {
            call_id,
            input,
            name,
            id: _,
            rest,
            ..
        } => vec![assistant_call(openai::ChatToolCall::Custom(
            openai::ChatCustomToolCall {
                id: call_id,
                type_: openai::CustomToolChoiceType::Custom,
                custom: openai::CustomToolCall {
                    input,
                    name,
                    rest: Default::default(),
                },
                rest,
            },
        ))],
        openai::TypedResponseItem::FunctionCallOutput {
            call_id,
            output,
            rest,
            ..
        }
        | openai::TypedResponseItem::CustomToolCallOutput {
            call_id,
            output,
            rest,
            ..
        } => vec![openai::ChatCompletionMessageParam::Tool(
            openai::ChatToolMessageParam {
                role: openai::ChatToolRole::Tool,
                content: output_to_chat(output)?,
                tool_call_id: call_id,
                rest,
            },
        )],
        openai::TypedResponseItem::ApplyPatchCall {
            call_id,
            operation,
            rest,
            ..
        } => vec![assistant_call(openai::ChatToolCall::Function(
            openai::ChatFunctionToolCall {
                id: call_id,
                type_: openai::FunctionToolChoiceType::Function,
                function: openai::FunctionCall {
                    arguments: serde_json::to_string(&operation)?,
                    name: "apply_patch".into(),
                    rest: Default::default(),
                },
                rest,
            },
        ))],
        openai::TypedResponseItem::ApplyPatchCallOutput {
            call_id,
            output,
            rest,
            ..
        } => vec![openai::ChatCompletionMessageParam::Tool(
            openai::ChatToolMessageParam {
                role: openai::ChatToolRole::Tool,
                content: openai::ChatTextContent::Text(output.unwrap_or_default()),
                tool_call_id: call_id,
                rest,
            },
        )],
        openai::TypedResponseItem::Reasoning { content, rest, .. } => {
            vec![openai::ChatCompletionMessageParam::Assistant(
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
                    rest,
                },
            )]
        }
        other @ (openai::TypedResponseItem::FileSearchCall { .. }
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
        | openai::TypedResponseItem::CompactionTrigger { .. }
        | openai::TypedResponseItem::ItemReference { .. }) => {
            vec![openai::ChatCompletionMessageParam::Unknown(
                serde_json::to_value(other)?,
            )]
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
    openai::ChatCompletionMessageParam::Assistant(openai::ChatAssistantMessageParam {
        role: openai::ChatAssistantRole::Assistant,
        content: None,
        audio: None,
        function_call: None,
        name: None,
        reasoning_content: Some(reasoning),
        refusal: None,
        tool_calls: None,
        rest: Default::default(),
    })
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
    openai::ChatCompletionMessageParam::Assistant(openai::ChatAssistantMessageParam {
        role: openai::ChatAssistantRole::Assistant,
        content: None,
        audio: None,
        function_call: None,
        name: None,
        reasoning_content: None,
        refusal: None,
        tool_calls: Some(vec![call]),
        rest: Default::default(),
    })
}

fn tool_choice(
    choice: Option<openai::ResponseToolChoice>,
) -> Result<Option<openai::ChatToolChoice>, TransformError> {
    Ok(match choice {
        None => None,
        Some(openai::ResponseToolChoice::Mode(mode)) => Some(openai::ChatToolChoice::Mode(mode)),
        Some(openai::ResponseToolChoice::Function(choice)) => Some(openai::ChatToolChoice::Named(
            openai::ChatNamedToolChoice::Function(openai::ChatNamedFunctionToolChoice {
                type_: openai::FunctionToolChoiceType::Function,
                function: openai::NamedTool {
                    name: choice.name,
                    rest: Default::default(),
                },
                rest: choice.rest,
            }),
        )),
        Some(openai::ResponseToolChoice::Custom(choice)) => Some(openai::ChatToolChoice::Named(
            openai::ChatNamedToolChoice::Custom(openai::ChatNamedCustomToolChoice {
                type_: openai::CustomToolChoiceType::Custom,
                custom: openai::NamedTool {
                    name: choice.name,
                    rest: Default::default(),
                },
                rest: choice.rest,
            }),
        )),
        Some(openai::ResponseToolChoice::Unknown(raw)) => {
            Some(openai::ChatToolChoice::Unknown(raw))
        }
        Some(other) => serde_json::from_slice(&serde_json::to_vec(&other)?).map(Some)?,
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
