use gproxy_protocol::openai;

use crate::TransformError;
use crate::common::tools;

pub(crate) fn transform(
    body: bytes::Bytes,
    model: &str,
    stream: bool,
) -> Result<bytes::Bytes, TransformError> {
    let input: openai::ChatCompletionRequest = serde_json::from_slice(&body)?;
    if input.audio.is_some()
        || input.frequency_penalty.is_some()
        || input.function_call.is_some()
        || input.functions.is_some()
        || input.logit_bias.is_some()
        || input.logprobs.is_some()
        || input.modalities.is_some()
        || input.n.is_some()
        || input.prediction.is_some()
        || input.presence_penalty.is_some()
        || input.seed.is_some()
        || input.stop.is_some()
        || input.web_search_options.is_some()
    {
        return Err(TransformError::unsupported(
            "OpenAI Chat request",
            "a Chat-only request parameter",
        ));
    }
    let mut items = Vec::new();
    for message in input.messages {
        items.extend(message_items(message)?);
    }
    let output = openai::ResponseCreateRequest {
        background: None,
        context_management: None,
        conversation: None,
        include: None,
        input: Some(openai::ResponseInput::Items(items)),
        instructions: None,
        max_output_tokens: input.max_completion_tokens.or(input.max_tokens),
        max_tool_calls: None,
        metadata: input.metadata,
        model: Some(model.into()),
        moderation: input.moderation,
        multi_agent: None,
        parallel_tool_calls: input.parallel_tool_calls,
        previous_response_id: None,
        prompt_cache_key: input.prompt_cache_key,
        prompt_cache_options: input.prompt_cache_options,
        prompt_cache_retention: input.prompt_cache_retention,
        prompt: None,
        reasoning: input
            .reasoning_effort
            .map(|effort| openai::ReasoningConfig {
                context: None,
                effort: Some(effort),
                mode: None,
                summary: None,
                generate_summary: None,
                rest: Default::default(),
            }),
        safety_identifier: input.safety_identifier,
        service_tier: input.service_tier,
        store: input.store,
        stream: Some(stream),
        stream_options: input
            .stream_options
            .map(|options| openai::ResponseStreamOptions {
                include_obfuscation: options.include_obfuscation,
                rest: options.rest,
            }),
        temperature: input.temperature,
        text: text_config(input.response_format, input.verbosity)?,
        tool_choice: tool_choice(input.tool_choice)?,
        tools: tools::chat_to_responses(input.tools)?,
        top_logprobs: input.top_logprobs,
        top_p: input.top_p,
        truncation: None,
        user: input.user,
        rest: input.rest,
    };
    Ok(bytes::Bytes::from(serde_json::to_vec(&output)?))
}

fn message_items(
    message: openai::ChatCompletionMessageParam,
) -> Result<Vec<openai::ResponseItem>, TransformError> {
    match message {
        openai::ChatCompletionMessageParam::Developer(message) => Ok(vec![easy_message(
            openai::ResponseEasyInputMessageRole::Developer,
            text_content(message.content)?,
            message.rest,
        )]),
        openai::ChatCompletionMessageParam::System(message) => Ok(vec![easy_message(
            openai::ResponseEasyInputMessageRole::System,
            text_content(message.content)?,
            message.rest,
        )]),
        openai::ChatCompletionMessageParam::User(message) => Ok(vec![easy_message(
            openai::ResponseEasyInputMessageRole::User,
            user_content(message.content)?,
            message.rest,
        )]),
        openai::ChatCompletionMessageParam::Assistant(message) => assistant_items(message),
        openai::ChatCompletionMessageParam::Tool(message) => Ok(vec![function_output(
            message.tool_call_id,
            text_output(message.content)?,
            message.rest,
        )]),
        openai::ChatCompletionMessageParam::Function(message) => Ok(vec![function_output(
            message.name,
            openai::ResponseOutput::Text(message.content),
            message.rest,
        )]),
        openai::ChatCompletionMessageParam::Unknown(raw) => {
            Ok(vec![openai::ResponseItem::Unknown(raw)])
        }
    }
}

fn assistant_items(
    message: openai::ChatAssistantMessageParam,
) -> Result<Vec<openai::ResponseItem>, TransformError> {
    let mut output = Vec::new();
    if let Some(content) = message.content {
        output.push(easy_message(
            openai::ResponseEasyInputMessageRole::Assistant,
            assistant_content(content)?,
            message.rest,
        ));
    }
    if let Some(reasoning) = message.reasoning_content {
        output.push(openai::ResponseItem::Typed(Box::new(
            openai::TypedResponseItem::Reasoning {
                id: None,
                summary: Vec::new(),
                content: Some(vec![openai::ResponseReasoningTextPart {
                    type_: openai::ResponseReasoningTextType::ReasoningText,
                    text: reasoning,
                    rest: Default::default(),
                }]),
                encrypted_content: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                rest: Default::default(),
            },
        )));
    }
    for call in message.tool_calls.into_iter().flatten() {
        output.push(match call {
            openai::ChatToolCall::Function(call) => {
                openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::FunctionCall {
                    arguments: call.function.arguments,
                    call_id: call.id.clone(),
                    name: call.function.name,
                    id: Some(call.id),
                    caller: None,
                    namespace: None,
                    status: Some(openai::ResponseItemLifecycleStatus::Completed),
                    rest: merge(call.rest, call.function.rest),
                }))
            }
            openai::ChatToolCall::Custom(call) => {
                openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::CustomToolCall {
                    call_id: call.id.clone(),
                    input: call.custom.input,
                    name: call.custom.name,
                    id: Some(call.id),
                    caller: None,
                    namespace: None,
                    rest: merge(call.rest, call.custom.rest),
                }))
            }
            openai::ChatToolCall::Unknown(raw) => openai::ResponseItem::Unknown(raw),
        });
    }
    Ok(output)
}

fn easy_message(
    role: openai::ResponseEasyInputMessageRole,
    content: openai::ResponseEasyInputContent,
    rest: openai::Rest,
) -> openai::ResponseItem {
    openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(
        openai::ResponseEasyInputMessageItem {
            type_: Some(openai::ResponseMessageItemType::Message),
            role,
            content,
            phase: None,
            rest,
        },
    ))
}

fn text_content(
    content: openai::ChatTextContent,
) -> Result<openai::ResponseEasyInputContent, TransformError> {
    Ok(match content {
        openai::ChatTextContent::Text(text) => openai::ResponseEasyInputContent::Text(text),
        openai::ChatTextContent::Parts(parts) => openai::ResponseEasyInputContent::Parts(
            parts
                .into_iter()
                .map(|part| match part {
                    openai::ChatTextContentPart::Text(part) => {
                        openai::ResponseInputContentPart::InputText(openai::ResponseInputText {
                            type_: openai::ResponseInputTextType::InputText,
                            text: part.text,
                            prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                            rest: part.rest,
                        })
                    }
                    openai::ChatTextContentPart::Unknown(raw) => {
                        openai::ResponseInputContentPart::Unknown(raw)
                    }
                })
                .collect(),
        ),
        openai::ChatTextContent::Unknown(raw) => openai::ResponseEasyInputContent::Unknown(raw),
    })
}

fn user_content(
    content: openai::ChatContent,
) -> Result<openai::ResponseEasyInputContent, TransformError> {
    Ok(match content {
        openai::ChatContent::Text(text) => openai::ResponseEasyInputContent::Text(text),
        openai::ChatContent::Parts(parts) => openai::ResponseEasyInputContent::Parts(
            parts
                .into_iter()
                .map(chat_part_to_response)
                .collect::<Result<_, _>>()?,
        ),
        openai::ChatContent::Unknown(raw) => openai::ResponseEasyInputContent::Unknown(raw),
    })
}

fn assistant_content(
    content: openai::ChatAssistantContent,
) -> Result<openai::ResponseEasyInputContent, TransformError> {
    Ok(match content {
        openai::ChatAssistantContent::Text(text) => {
            openai::ResponseEasyInputContent::OutputParts(vec![
                openai::ResponseMessageOutputContentPart::OutputText(openai::ResponseOutputText {
                    type_: openai::ResponseOutputTextType::OutputText,
                    annotations: Vec::new(),
                    logprobs: None,
                    text,
                    rest: Default::default(),
                }),
            ])
        }
        openai::ChatAssistantContent::Parts(parts) => {
            openai::ResponseEasyInputContent::OutputParts(
                parts
                    .into_iter()
                    .map(|part| match part {
                        openai::ChatAssistantContentPart::Text(part) => {
                            openai::ResponseMessageOutputContentPart::OutputText(
                                openai::ResponseOutputText {
                                    type_: openai::ResponseOutputTextType::OutputText,
                                    annotations: Vec::new(),
                                    logprobs: None,
                                    text: part.text,
                                    rest: part.rest,
                                },
                            )
                        }
                        openai::ChatAssistantContentPart::Refusal(part) => {
                            openai::ResponseMessageOutputContentPart::Refusal(
                                openai::ResponseRefusal {
                                    type_: openai::ResponseRefusalType::Refusal,
                                    refusal: part.refusal,
                                    rest: part.rest,
                                },
                            )
                        }
                        openai::ChatAssistantContentPart::Unknown(raw) => {
                            openai::ResponseMessageOutputContentPart::Unknown(raw)
                        }
                    })
                    .collect(),
            )
        }
        openai::ChatAssistantContent::Unknown(raw) => {
            openai::ResponseEasyInputContent::Unknown(raw)
        }
    })
}

fn chat_part_to_response(
    part: openai::ChatContentPart,
) -> Result<openai::ResponseInputContentPart, TransformError> {
    Ok(match part {
        openai::ChatContentPart::Text(part) => {
            openai::ResponseInputContentPart::InputText(openai::ResponseInputText {
                type_: openai::ResponseInputTextType::InputText,
                text: part.text,
                prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                rest: part.rest,
            })
        }
        openai::ChatContentPart::ImageUrl(part) => {
            openai::ResponseInputContentPart::InputImage(openai::ResponseInputImage {
                type_: openai::ResponseInputImageType::InputImage,
                detail: None,
                file_id: None,
                image_url: Some(part.image_url.url),
                prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                rest: part.rest,
            })
        }
        openai::ChatContentPart::File(part) => {
            openai::ResponseInputContentPart::InputFile(openai::ResponseInputFile {
                type_: openai::ResponseInputFileType::InputFile,
                detail: None,
                file_data: part.file.file_data,
                file_id: part.file.file_id,
                file_url: None,
                filename: part.file.filename,
                prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                rest: part.rest,
            })
        }
        openai::ChatContentPart::InputAudio(part) => {
            openai::ResponseInputContentPart::InputAudio(openai::ResponseInputAudio {
                type_: openai::ResponseInputAudioType::InputAudio,
                input_audio: openai::InputAudioContent {
                    data: part.input_audio.data,
                    format: part.input_audio.format,
                    rest: part.input_audio.rest,
                },
                rest: part.rest,
            })
        }
        openai::ChatContentPart::Unknown(raw) => openai::ResponseInputContentPart::Unknown(raw),
    })
}

fn text_output(content: openai::ChatTextContent) -> Result<openai::ResponseOutput, TransformError> {
    Ok(match text_content(content)? {
        openai::ResponseEasyInputContent::Text(text) => openai::ResponseOutput::Text(text),
        openai::ResponseEasyInputContent::Parts(parts) => openai::ResponseOutput::Parts(parts),
        openai::ResponseEasyInputContent::Unknown(raw) => openai::ResponseOutput::Unknown(raw),
        openai::ResponseEasyInputContent::OutputParts(_) => {
            return Err(TransformError::shape(
                "Chat tool output",
                "unexpected assistant output parts",
            ));
        }
    })
}

fn function_output(
    call_id: String,
    output: openai::ResponseOutput,
    rest: openai::Rest,
) -> openai::ResponseItem {
    openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::FunctionCallOutput {
        call_id,
        output,
        id: None,
        caller: None,
        name: None,
        namespace: None,
        status: Some(openai::ResponseItemLifecycleStatus::Completed),
        created_by: None,
        rest,
    }))
}

fn tool_choice(
    choice: Option<openai::ChatToolChoice>,
) -> Result<Option<openai::ResponseToolChoice>, TransformError> {
    Ok(match choice {
        None => None,
        Some(openai::ChatToolChoice::Mode(mode)) => Some(openai::ResponseToolChoice::Mode(mode)),
        Some(openai::ChatToolChoice::Named(openai::ChatNamedToolChoice::Function(choice))) => Some(
            openai::ResponseToolChoice::Function(openai::ResponseFunctionToolChoice {
                type_: openai::FunctionToolChoiceType::Function,
                name: choice.function.name,
                rest: choice.rest,
            }),
        ),
        Some(openai::ChatToolChoice::Named(openai::ChatNamedToolChoice::Custom(choice))) => Some(
            openai::ResponseToolChoice::Custom(openai::ResponseCustomToolChoice {
                type_: openai::CustomToolChoiceType::Custom,
                name: choice.custom.name,
                rest: choice.rest,
            }),
        ),
        Some(openai::ChatToolChoice::Unknown(raw)) => {
            Some(openai::ResponseToolChoice::Unknown(raw))
        }
        Some(other) => serde_json::from_slice(&serde_json::to_vec(&other)?).map(Some)?,
    })
}

fn text_config(
    format: Option<openai::ChatResponseFormat>,
    verbosity: Option<openai::Verbosity>,
) -> Result<Option<openai::TextConfig>, TransformError> {
    let format = format
        .map(|format| serde_json::from_slice(&serde_json::to_vec(&format)?))
        .transpose()?;
    Ok(
        (format.is_some() || verbosity.is_some()).then_some(openai::TextConfig {
            format,
            verbosity,
            rest: Default::default(),
        }),
    )
}

fn merge(mut left: openai::Rest, right: openai::Rest) -> openai::Rest {
    left.extend(right);
    left
}
