use crate::openai::count_tokens::types as ot;
use crate::openai::create_chat_completions::response::OpenAiChatCompletionsResponse;
use crate::openai::create_chat_completions::types as ct;
use crate::openai::create_response::response::OpenAiCreateResponseResponse;
use crate::openai::create_response::types as rt;
use crate::openai::types::OpenAiResponseHeaders;
use crate::transform::utils::TransformError;

impl TryFrom<OpenAiCreateResponseResponse> for OpenAiChatCompletionsResponse {
    type Error = TransformError;

    fn try_from(value: OpenAiCreateResponseResponse) -> Result<Self, TransformError> {
        Ok(match value {
            OpenAiCreateResponseResponse::Success {
                stats_code,
                headers,
                body,
            } => {
                let mut content_parts = Vec::new();
                let mut refusal_parts = Vec::new();
                let mut function_calls = Vec::new();
                let mut custom_calls = Vec::new();

                for item in &body.output {
                    match item {
                        rt::ResponseOutputItem::Message(message) => {
                            for content in &message.content {
                                match content {
                                    ot::ResponseOutputContent::Text(text) => {
                                        if !text.text.is_empty() {
                                            content_parts.push(text.text.clone());
                                        }
                                    }
                                    ot::ResponseOutputContent::Refusal(refusal) => {
                                        if !refusal.refusal.is_empty() {
                                            refusal_parts.push(refusal.refusal.clone());
                                        }
                                    }
                                }
                            }
                        }
                        rt::ResponseOutputItem::FunctionToolCall(call) => {
                            function_calls.push(ct::ChatCompletionMessageFunctionToolCall {
                                id: call.call_id.clone(),
                                function: ct::ChatCompletionFunctionCall {
                                    arguments: call.arguments.clone(),
                                    name: call.name.clone(),
                                },
                                type_: ct::ChatCompletionMessageFunctionToolCallType::Function,
                            });
                        }
                        rt::ResponseOutputItem::CustomToolCall(call) => {
                            custom_calls.push(ct::ChatCompletionMessageCustomToolCall {
                                id: call.call_id.clone(),
                                custom: ct::ChatCompletionMessageCustomToolCallPayload {
                                    input: call.input.clone(),
                                    name: call.name.clone(),
                                },
                                type_: ct::ChatCompletionMessageCustomToolCallType::Custom,
                            });
                        }
                        _ => {}
                    }
                }

                let function_call = function_calls.first().map(|call| call.function.clone());

                let mut tool_calls = function_calls
                    .into_iter()
                    .map(ct::ChatCompletionMessageToolCall::Function)
                    .collect::<Vec<_>>();
                tool_calls.extend(
                    custom_calls
                        .into_iter()
                        .map(ct::ChatCompletionMessageToolCall::Custom),
                );

                let finish_reason = if matches!(
                    body.incomplete_details
                        .as_ref()
                        .and_then(|d| d.reason.as_ref()),
                    Some(rt::ResponseIncompleteReason::MaxOutputTokens)
                ) {
                    ct::ChatCompletionFinishReason::Length
                } else if matches!(
                    body.incomplete_details
                        .as_ref()
                        .and_then(|d| d.reason.as_ref()),
                    Some(rt::ResponseIncompleteReason::ContentFilter)
                ) {
                    ct::ChatCompletionFinishReason::ContentFilter
                } else if !tool_calls.is_empty() {
                    ct::ChatCompletionFinishReason::ToolCalls
                } else {
                    ct::ChatCompletionFinishReason::Stop
                };

                OpenAiChatCompletionsResponse::Success {
                    stats_code,
                    headers: OpenAiResponseHeaders {
                        extra: headers.extra,
                    },
                    body: ct::ChatCompletion {
                        id: body.id,
                        choices: vec![ct::ChatCompletionChoice {
                            finish_reason,
                            index: 0,
                            logprobs: None,
                            message: ct::ChatCompletionMessage {
                                content: if content_parts.is_empty() {
                                    None
                                } else {
                                    Some(content_parts.join("\n"))
                                },
                                reasoning_content: None,
                                reasoning_details: None,
                                refusal: if refusal_parts.is_empty() {
                                    None
                                } else {
                                    Some(refusal_parts.join("\n"))
                                },
                                role: ct::ChatCompletionAssistantRole::Assistant,
                                annotations: None,
                                audio: None,
                                function_call,
                                tool_calls: if tool_calls.is_empty() {
                                    None
                                } else {
                                    Some(tool_calls)
                                },
                            },
                        }],
                        created: body.created_at,
                        model: body.model,
                        object: ct::ChatCompletionObject::ChatCompletion,
                        service_tier: body.service_tier.map(|tier| match tier {
                            rt::ResponseServiceTier::Auto => ct::ChatCompletionServiceTier::Auto,
                            rt::ResponseServiceTier::Default => {
                                ct::ChatCompletionServiceTier::Default
                            }
                            rt::ResponseServiceTier::Flex => ct::ChatCompletionServiceTier::Flex,
                            rt::ResponseServiceTier::Scale => ct::ChatCompletionServiceTier::Scale,
                            rt::ResponseServiceTier::Priority => {
                                ct::ChatCompletionServiceTier::Priority
                            }
                        }),
                        system_fingerprint: None,
                        usage: body.usage.map(|usage| ct::CompletionUsage {
                            completion_tokens: usage.output_tokens,
                            prompt_tokens: usage.input_tokens,
                            total_tokens: usage.total_tokens,
                            completion_tokens_details: Some(ct::CompletionTokensDetails {
                                accepted_prediction_tokens: None,
                                audio_tokens: None,
                                reasoning_tokens: Some(
                                    usage.output_tokens_details.reasoning_tokens,
                                ),
                                rejected_prediction_tokens: None,
                            }),
                            prompt_tokens_details: Some(ct::PromptTokensDetails {
                                audio_tokens: None,
                                cached_tokens: Some(usage.input_tokens_details.cached_tokens),
                            }),
                        }),
                    },
                }
            }
            OpenAiCreateResponseResponse::Error {
                stats_code,
                headers,
                body,
            } => OpenAiChatCompletionsResponse::Error {
                stats_code,
                headers: OpenAiResponseHeaders {
                    extra: headers.extra,
                },
                body,
            },
        })
    }
}
