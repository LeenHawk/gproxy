use crate::openai::count_tokens::types as ot;
use crate::openai::create_chat_completions::request::{
    OpenAiChatCompletionsRequest, PathParameters, QueryParameters, RequestBody, RequestHeaders,
};
use crate::openai::create_chat_completions::types as ct;
use crate::openai::create_response::request::OpenAiCreateResponseRequest;
use crate::openai::create_response::types::ResponsePromptCacheRetention;
use crate::transform::openai::count_tokens::utils::{
    openai_function_call_output_content_to_text, openai_input_to_items,
    openai_message_content_to_text, openai_reasoning_summary_to_text,
};
use crate::transform::utils::TransformError;

use super::utils::{
    custom_call_output_to_text, message_content_to_user_content,
    response_reasoning_to_chat_reasoning, response_service_tier_to_chat,
    response_text_to_chat_response_format, response_text_to_chat_verbosity,
    response_tool_choice_to_chat_tool_choice, response_tools_to_chat_tools,
};

fn assistant_message_with_text(text: String) -> ct::ChatCompletionAssistantMessageParam {
    ct::ChatCompletionAssistantMessageParam {
        role: ct::ChatCompletionAssistantRole::Assistant,
        audio: None,
        content: if text.is_empty() {
            None
        } else {
            Some(ct::ChatCompletionAssistantContent::Text(text))
        },
        reasoning_content: None,
        function_call: None,
        name: None,
        refusal: None,
        tool_calls: None,
    }
}

fn append_joined_text(target: &mut Option<String>, delta: String) {
    if delta.is_empty() {
        return;
    }

    match target {
        Some(existing) if !existing.is_empty() => {
            existing.push('\n');
            existing.push_str(&delta);
        }
        Some(existing) => existing.push_str(&delta),
        None => *target = Some(delta),
    }
}

fn append_assistant_text(target: &mut ct::ChatCompletionAssistantMessageParam, delta: String) {
    if delta.is_empty() {
        return;
    }

    match target.content.as_mut() {
        Some(ct::ChatCompletionAssistantContent::Text(existing)) if !existing.is_empty() => {
            existing.push('\n');
            existing.push_str(&delta);
        }
        Some(ct::ChatCompletionAssistantContent::Text(existing)) => existing.push_str(&delta),
        Some(ct::ChatCompletionAssistantContent::Parts(parts)) => {
            parts.push(ct::ChatCompletionAssistantContentPart::Text(
                ct::ChatCompletionContentPartText {
                    text: delta,
                    type_: ct::ChatCompletionContentPartTextType::Text,
                },
            ));
        }
        None => {
            target.content = Some(ct::ChatCompletionAssistantContent::Text(delta));
        }
    }
}

fn reasoning_item_to_text(reasoning: &ot::ResponseReasoningItem) -> String {
    if let Some(content) = reasoning.content.as_ref() {
        let text = content
            .iter()
            .map(|part| part.text.as_str())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            return text;
        }
    }

    let text = openai_reasoning_summary_to_text(&reasoning.summary);
    if !text.is_empty() {
        return text;
    }

    reasoning.encrypted_content.clone().unwrap_or_default()
}

fn flush_pending_assistant(
    messages: &mut Vec<ct::ChatCompletionMessageParam>,
    pending_assistant: &mut Option<ct::ChatCompletionAssistantMessageParam>,
) {
    if let Some(message) = pending_assistant.take() {
        messages.push(ct::ChatCompletionMessageParam::Assistant(message));
    }
}

impl TryFrom<OpenAiCreateResponseRequest> for OpenAiChatCompletionsRequest {
    type Error = TransformError;

    fn try_from(value: OpenAiCreateResponseRequest) -> Result<Self, TransformError> {
        let body = value.body;
        let mut messages = Vec::new();
        let mut pending_assistant: Option<ct::ChatCompletionAssistantMessageParam> = None;

        if let Some(instructions) = body.instructions.as_ref().filter(|text| !text.is_empty()) {
            messages.push(ct::ChatCompletionMessageParam::System(
                ct::ChatCompletionSystemMessageParam {
                    content: ct::ChatCompletionTextContent::Text(instructions.clone()),
                    role: ct::ChatCompletionSystemRole::System,
                    name: None,
                },
            ));
        }

        for item in openai_input_to_items(body.input.clone()) {
            match item {
                ot::ResponseInputItem::Message(message) => match message.role {
                    ot::ResponseInputMessageRole::User => {
                        flush_pending_assistant(&mut messages, &mut pending_assistant);
                        messages.push(ct::ChatCompletionMessageParam::User(
                            ct::ChatCompletionUserMessageParam {
                                content: message_content_to_user_content(message.content),
                                role: ct::ChatCompletionUserRole::User,
                                name: None,
                            },
                        ));
                    }
                    ot::ResponseInputMessageRole::Assistant => {
                        let text = openai_message_content_to_text(&message.content);
                        let assistant = pending_assistant
                            .get_or_insert_with(|| assistant_message_with_text(String::new()));
                        append_assistant_text(assistant, text);
                    }
                    ot::ResponseInputMessageRole::System => {
                        flush_pending_assistant(&mut messages, &mut pending_assistant);
                        let text = openai_message_content_to_text(&message.content);
                        messages.push(ct::ChatCompletionMessageParam::System(
                            ct::ChatCompletionSystemMessageParam {
                                content: ct::ChatCompletionTextContent::Text(text),
                                role: ct::ChatCompletionSystemRole::System,
                                name: None,
                            },
                        ));
                    }
                    ot::ResponseInputMessageRole::Developer => {
                        flush_pending_assistant(&mut messages, &mut pending_assistant);
                        let text = openai_message_content_to_text(&message.content);
                        messages.push(ct::ChatCompletionMessageParam::Developer(
                            ct::ChatCompletionDeveloperMessageParam {
                                content: ct::ChatCompletionTextContent::Text(text),
                                role: ct::ChatCompletionDeveloperRole::Developer,
                                name: None,
                            },
                        ));
                    }
                },
                ot::ResponseInputItem::OutputMessage(message) => {
                    let assistant = pending_assistant
                        .get_or_insert_with(|| assistant_message_with_text(String::new()));
                    let mut text_parts = Vec::new();
                    let mut refusal_parts = Vec::new();
                    for part in message.content {
                        match part {
                            ot::ResponseOutputContent::Text(text) => {
                                if !text.text.is_empty() {
                                    text_parts.push(text.text);
                                }
                            }
                            ot::ResponseOutputContent::Refusal(refusal) => {
                                if !refusal.refusal.is_empty() {
                                    refusal_parts.push(refusal.refusal);
                                }
                            }
                        }
                    }

                    append_assistant_text(assistant, text_parts.join("\n"));
                    append_joined_text(&mut assistant.refusal, refusal_parts.join("\n"));
                }
                ot::ResponseInputItem::FunctionToolCall(tool_call) => {
                    let assistant = pending_assistant
                        .get_or_insert_with(|| assistant_message_with_text(String::new()));
                    assistant.tool_calls.get_or_insert_with(Vec::new).push(
                        ct::ChatCompletionMessageToolCall::Function(
                            ct::ChatCompletionMessageFunctionToolCall {
                                id: tool_call.call_id,
                                function: ct::ChatCompletionFunctionCall {
                                    arguments: tool_call.arguments,
                                    name: tool_call.name,
                                },
                                type_: ct::ChatCompletionMessageFunctionToolCallType::Function,
                            },
                        ),
                    );
                }
                ot::ResponseInputItem::CustomToolCall(tool_call) => {
                    let id = tool_call
                        .id
                        .clone()
                        .unwrap_or_else(|| tool_call.call_id.clone());
                    let assistant = pending_assistant
                        .get_or_insert_with(|| assistant_message_with_text(String::new()));
                    assistant.tool_calls.get_or_insert_with(Vec::new).push(
                        ct::ChatCompletionMessageToolCall::Custom(
                            ct::ChatCompletionMessageCustomToolCall {
                                id,
                                custom: ct::ChatCompletionMessageCustomToolCallPayload {
                                    input: tool_call.input,
                                    name: tool_call.name,
                                },
                                type_: ct::ChatCompletionMessageCustomToolCallType::Custom,
                            },
                        ),
                    );
                }
                ot::ResponseInputItem::FunctionCallOutput(output) => {
                    flush_pending_assistant(&mut messages, &mut pending_assistant);
                    let tool_call_id = output.id.unwrap_or(output.call_id);
                    messages.push(ct::ChatCompletionMessageParam::Tool(
                        ct::ChatCompletionToolMessageParam {
                            content: ct::ChatCompletionTextContent::Text(
                                openai_function_call_output_content_to_text(&output.output),
                            ),
                            role: ct::ChatCompletionToolRole::Tool,
                            tool_call_id,
                        },
                    ));
                }
                ot::ResponseInputItem::CustomToolCallOutput(output) => {
                    flush_pending_assistant(&mut messages, &mut pending_assistant);
                    let tool_call_id = output.id.unwrap_or(output.call_id);
                    messages.push(ct::ChatCompletionMessageParam::Tool(
                        ct::ChatCompletionToolMessageParam {
                            content: ct::ChatCompletionTextContent::Text(
                                custom_call_output_to_text(&output.output),
                            ),
                            role: ct::ChatCompletionToolRole::Tool,
                            tool_call_id,
                        },
                    ));
                }
                ot::ResponseInputItem::ReasoningItem(reasoning) => {
                    let assistant = pending_assistant
                        .get_or_insert_with(|| assistant_message_with_text(String::new()));
                    let reasoning_text = reasoning_item_to_text(&reasoning);
                    append_joined_text(&mut assistant.reasoning_content, reasoning_text);
                }
                _ => {}
            }
        }

        flush_pending_assistant(&mut messages, &mut pending_assistant);

        let service_tier = body.service_tier.map(response_service_tier_to_chat);
        let response_format = response_text_to_chat_response_format(body.text.as_ref());
        let verbosity = response_text_to_chat_verbosity(body.text.as_ref());
        let prompt_cache_retention = body.prompt_cache_retention.map(|value| match value {
            ResponsePromptCacheRetention::InMemory => {
                ct::ChatCompletionPromptCacheRetention::InMemory
            }
            ResponsePromptCacheRetention::H24 => ct::ChatCompletionPromptCacheRetention::H24,
        });

        Ok(OpenAiChatCompletionsRequest {
            method: ct::HttpMethod::Post,
            path: PathParameters::default(),
            query: QueryParameters::default(),
            headers: RequestHeaders::default(),
            body: RequestBody {
                messages,
                model: body.model.unwrap_or_default(),
                audio: None,
                frequency_penalty: None,
                function_call: None,
                functions: None,
                logit_bias: None,
                logprobs: None,
                max_completion_tokens: body.max_output_tokens,
                max_tokens: None,
                metadata: body.metadata,
                modalities: None,
                n: None,
                parallel_tool_calls: body.parallel_tool_calls,
                prediction: None,
                presence_penalty: None,
                prompt_cache_key: body.prompt_cache_key,
                prompt_cache_retention,
                reasoning_effort: response_reasoning_to_chat_reasoning(body.reasoning),
                response_format,
                safety_identifier: body.safety_identifier,
                seed: None,
                service_tier,
                stop: None,
                store: body.store,
                stream: None,
                stream_options: None,
                temperature: body.temperature,
                tool_choice: response_tool_choice_to_chat_tool_choice(body.tool_choice),
                tools: response_tools_to_chat_tools(body.tools),
                top_logprobs: body.top_logprobs,
                top_p: body.top_p,
                user: body.user,
                verbosity,
                thinking: None,
                thinking_config: None,
                cached_content: None,
                web_search_options: None,
            },
        })
    }
}
