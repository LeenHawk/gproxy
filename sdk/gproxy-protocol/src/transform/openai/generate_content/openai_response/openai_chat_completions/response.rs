use std::collections::BTreeMap;

use crate::openai::count_tokens::types as ot;
use crate::openai::create_chat_completions::response::OpenAiChatCompletionsResponse;
use crate::openai::create_chat_completions::types as ct;
use crate::openai::create_response::response::{OpenAiCreateResponseResponse, ResponseBody};
use crate::openai::create_response::types as rt;
use crate::openai::types::OpenAiResponseHeaders;
use crate::transform::utils::TransformError;

fn reasoning_item_from_chat_message(
    fallback_id: String,
    reasoning_content: Option<String>,
    reasoning_details: Option<Vec<ct::ChatCompletionReasoningDetail>>,
) -> Option<rt::ResponseOutputItem> {
    let mut encrypted_content = None;
    let mut reasoning_id = Some(fallback_id);

    if let Some(details) = reasoning_details {
        for detail in details {
            if matches!(
                detail.type_,
                ct::ChatCompletionReasoningDetailType::ReasoningEncrypted
            ) {
                if detail.id.is_some() {
                    reasoning_id = detail.id;
                }
                if detail.data.is_some() {
                    encrypted_content = detail.data;
                }
                break;
            }
        }
    }

    let text = reasoning_content.unwrap_or_default();
    if text.is_empty() && encrypted_content.is_none() {
        return None;
    }

    let summary = if text.is_empty() {
        Vec::new()
    } else {
        vec![ot::ResponseSummaryTextContent {
            text: text.clone(),
            type_: ot::ResponseSummaryTextContentType::SummaryText,
        }]
    };
    let content = if text.is_empty() {
        None
    } else {
        Some(vec![ot::ResponseReasoningTextContent {
            text,
            type_: ot::ResponseReasoningTextContentType::ReasoningText,
        }])
    };

    Some(rt::ResponseOutputItem::ReasoningItem(
        ot::ResponseReasoningItem {
            id: reasoning_id,
            summary,
            type_: ot::ResponseReasoningItemType::Reasoning,
            content,
            encrypted_content,
            status: Some(ot::ResponseItemStatus::Completed),
        },
    ))
}

impl TryFrom<OpenAiChatCompletionsResponse> for OpenAiCreateResponseResponse {
    type Error = TransformError;

    fn try_from(value: OpenAiChatCompletionsResponse) -> Result<Self, TransformError> {
        Ok(match value {
            OpenAiChatCompletionsResponse::Success {
                stats_code,
                headers,
                body,
            } => {
                let choice = body.choices.into_iter().next();
                let mut output = Vec::new();
                let mut output_text_parts = Vec::new();
                let mut tool_call_count = 0usize;

                let mut status = Some(rt::ResponseStatus::Completed);
                let mut incomplete_details = None;

                if let Some(choice) = choice {
                    match choice.finish_reason {
                        ct::ChatCompletionFinishReason::Length => {
                            status = Some(rt::ResponseStatus::Incomplete);
                            incomplete_details = Some(rt::ResponseIncompleteDetails {
                                reason: Some(rt::ResponseIncompleteReason::MaxOutputTokens),
                            });
                        }
                        ct::ChatCompletionFinishReason::ContentFilter => {
                            status = Some(rt::ResponseStatus::Incomplete);
                            incomplete_details = Some(rt::ResponseIncompleteDetails {
                                reason: Some(rt::ResponseIncompleteReason::ContentFilter),
                            });
                        }
                        ct::ChatCompletionFinishReason::Stop
                        | ct::ChatCompletionFinishReason::ToolCalls
                        | ct::ChatCompletionFinishReason::FunctionCall => {}
                    }

                    if let Some(reasoning_item) = reasoning_item_from_chat_message(
                        format!("{}_reasoning_0", body.id),
                        choice.message.reasoning_content.clone(),
                        choice.message.reasoning_details.clone(),
                    ) {
                        output.push(reasoning_item);
                    }

                    let mut message_content = Vec::new();
                    if let Some(content) = choice.message.content
                        && !content.is_empty()
                    {
                        output_text_parts.push(content.clone());
                        message_content.push(ot::ResponseOutputContent::Text(
                            ot::ResponseOutputText {
                                annotations: Vec::new(),
                                logprobs: None,
                                text: content,
                                type_: ot::ResponseOutputTextType::OutputText,
                            },
                        ));
                    }
                    if let Some(refusal) = choice.message.refusal
                        && !refusal.is_empty()
                    {
                        message_content.push(ot::ResponseOutputContent::Refusal(
                            ot::ResponseOutputRefusal {
                                refusal,
                                type_: ot::ResponseOutputRefusalType::Refusal,
                            },
                        ));
                    }

                    if !message_content.is_empty() {
                        output.push(rt::ResponseOutputItem::Message(ot::ResponseOutputMessage {
                            id: format!("{}_message_0", body.id),
                            content: message_content,
                            role: ot::ResponseOutputMessageRole::Assistant,
                            phase: Some(ot::ResponseMessagePhase::FinalAnswer),
                            status: Some(ot::ResponseItemStatus::Completed),
                            type_: Some(ot::ResponseOutputMessageType::Message),
                        }));
                    }

                    if let Some(function_call) = choice.message.function_call {
                        tool_call_count += 1;
                        output.push(rt::ResponseOutputItem::FunctionToolCall(
                            ot::ResponseFunctionToolCall {
                                arguments: function_call.arguments,
                                call_id: "function_call".to_string(),
                                name: function_call.name,
                                type_: ot::ResponseFunctionToolCallType::FunctionCall,
                                id: Some("function_call".to_string()),
                                status: None,
                            },
                        ));
                    }

                    if let Some(tool_calls) = choice.message.tool_calls {
                        for tool_call in tool_calls {
                            match tool_call {
                                ct::ChatCompletionMessageToolCall::Function(call) => {
                                    tool_call_count += 1;
                                    output.push(rt::ResponseOutputItem::FunctionToolCall(
                                        ot::ResponseFunctionToolCall {
                                            arguments: call.function.arguments,
                                            call_id: call.id.clone(),
                                            name: call.function.name,
                                            type_: ot::ResponseFunctionToolCallType::FunctionCall,
                                            id: Some(call.id),
                                            status: None,
                                        },
                                    ));
                                }
                                ct::ChatCompletionMessageToolCall::Custom(call) => {
                                    tool_call_count += 1;
                                    output.push(rt::ResponseOutputItem::CustomToolCall(
                                        ot::ResponseCustomToolCall {
                                            call_id: call.id.clone(),
                                            input: call.custom.input,
                                            name: call.custom.name,
                                            type_: ot::ResponseCustomToolCallType::CustomToolCall,
                                            id: Some(call.id),
                                        },
                                    ));
                                }
                            }
                        }
                    }
                }

                OpenAiCreateResponseResponse::Success {
                    stats_code,
                    headers: OpenAiResponseHeaders {
                        extra: headers.extra,
                    },
                    body: ResponseBody {
                        id: body.id,
                        created_at: body.created,
                        error: None,
                        incomplete_details,
                        instructions: Some(ot::ResponseInput::Text(String::new())),
                        metadata: BTreeMap::new(),
                        model: body.model,
                        object: rt::ResponseObject::Response,
                        output,
                        parallel_tool_calls: tool_call_count > 1,
                        temperature: 1.0,
                        tool_choice: if tool_call_count > 0 {
                            ot::ResponseToolChoice::Options(ot::ResponseToolChoiceOptions::Required)
                        } else {
                            ot::ResponseToolChoice::Options(ot::ResponseToolChoiceOptions::Auto)
                        },
                        tools: Vec::new(),
                        top_p: 1.0,
                        background: None,
                        completed_at: None,
                        conversation: None,
                        max_output_tokens: None,
                        max_tool_calls: None,
                        output_text: if output_text_parts.is_empty() {
                            None
                        } else {
                            Some(output_text_parts.join("\n"))
                        },
                        previous_response_id: None,
                        prompt: None,
                        prompt_cache_key: None,
                        prompt_cache_retention: None,
                        reasoning: None,
                        safety_identifier: None,
                        service_tier: body.service_tier.map(|tier| match tier {
                            ct::ChatCompletionServiceTier::Auto => rt::ResponseServiceTier::Auto,
                            ct::ChatCompletionServiceTier::Default => {
                                rt::ResponseServiceTier::Default
                            }
                            ct::ChatCompletionServiceTier::Flex => rt::ResponseServiceTier::Flex,
                            ct::ChatCompletionServiceTier::Scale => rt::ResponseServiceTier::Scale,
                            ct::ChatCompletionServiceTier::Priority => {
                                rt::ResponseServiceTier::Priority
                            }
                        }),
                        status,
                        text: None,
                        top_logprobs: None,
                        truncation: None,
                        usage: body.usage.map(|usage| {
                            let cached_tokens = usage
                                .prompt_tokens_details
                                .as_ref()
                                .and_then(|details| details.cached_tokens)
                                .unwrap_or(0);
                            let reasoning_tokens = usage
                                .completion_tokens_details
                                .as_ref()
                                .and_then(|details| details.reasoning_tokens)
                                .unwrap_or(0);
                            rt::ResponseUsage {
                                input_tokens: usage.prompt_tokens,
                                input_tokens_details: rt::ResponseInputTokensDetails {
                                    cached_tokens,
                                },
                                output_tokens: usage.completion_tokens,
                                output_tokens_details: rt::ResponseOutputTokensDetails {
                                    reasoning_tokens,
                                },
                                total_tokens: usage.total_tokens,
                            }
                        }),
                        user: None,
                    },
                }
            }
            OpenAiChatCompletionsResponse::Error {
                stats_code,
                headers,
                body,
            } => OpenAiCreateResponseResponse::Error {
                stats_code,
                headers: OpenAiResponseHeaders {
                    extra: headers.extra,
                },
                body,
            },
        })
    }
}
