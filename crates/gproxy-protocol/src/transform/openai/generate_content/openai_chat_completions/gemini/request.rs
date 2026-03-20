use super::utils::parse_tool_use_args;
use crate::gemini::count_tokens::types as gt;
use crate::gemini::generate_content::request::{
    GeminiGenerateContentRequest, PathParameters, QueryParameters, RequestBody, RequestHeaders,
};
use crate::gemini::generate_content::types::HttpMethod as GeminiHttpMethod;
use crate::openai::count_tokens::types as ot;
use crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest;
use crate::openai::create_chat_completions::types as oct;
use crate::transform::gemini::model_get::utils::ensure_models_prefix;
use crate::transform::openai::count_tokens::gemini::utils::{
    GEMINI_SKIP_THOUGHT_SIGNATURE, openai_generation_config,
    openai_message_content_to_gemini_parts, openai_role_to_gemini, openai_tool_choice_to_gemini,
    openai_tools_to_gemini, output_text_to_json_object,
};
use crate::transform::openai::generate_content::openai_chat_completions::openai::utils::{
    chat_reasoning_to_response_reasoning, chat_response_text_config, chat_stop_to_vec,
    chat_text_content_to_plain_text, chat_text_content_to_response_input_message_content,
    chat_tool_choice_to_response_tool_choice, chat_tools_to_response_tools,
    chat_user_content_to_response_input_message_content, pseudo_reasoning_signature,
};
use crate::transform::utils::TransformError;

impl TryFrom<OpenAiChatCompletionsRequest> for GeminiGenerateContentRequest {
    type Error = TransformError;

    fn try_from(value: OpenAiChatCompletionsRequest) -> Result<Self, TransformError> {
        let crate::openai::create_chat_completions::request::RequestBody {
            messages: chat_messages,
            model,
            function_call,
            functions,
            frequency_penalty,
            max_completion_tokens,
            max_tokens,
            n,
            presence_penalty,
            reasoning_effort,
            response_format,
            seed,
            stop,
            temperature,
            tool_choice,
            tools,
            top_logprobs,
            top_p,
            verbosity,
            thinking_config: chat_thinking_config,
            cached_content: chat_cached_content,
            web_search_options,
            ..
        } = value.body;

        let response_reasoning = chat_reasoning_to_response_reasoning(reasoning_effort);
        let response_text = chat_response_text_config(response_format, verbosity);
        let response_tool_choice =
            chat_tool_choice_to_response_tool_choice(tool_choice, function_call);
        let response_tools = chat_tools_to_response_tools(tools, functions, web_search_options);
        let model = ensure_models_prefix(&model);

        let mut contents = Vec::new();
        let mut system_parts = Vec::new();
        let mut seen_non_system = false;

        for (message_index, message) in chat_messages.into_iter().enumerate() {
            match message {
                oct::ChatCompletionMessageParam::Developer(message) => {
                    let content =
                        chat_text_content_to_response_input_message_content(message.content);
                    let parts = openai_message_content_to_gemini_parts(content);
                    if parts.is_empty() {
                        continue;
                    }
                    if seen_non_system {
                        contents.push(gt::GeminiContent {
                            parts,
                            role: Some(openai_role_to_gemini(
                                ot::ResponseInputMessageRole::Developer,
                            )),
                        });
                    } else {
                        system_parts.extend(parts);
                    }
                }
                oct::ChatCompletionMessageParam::System(message) => {
                    let content =
                        chat_text_content_to_response_input_message_content(message.content);
                    let parts = openai_message_content_to_gemini_parts(content);
                    if parts.is_empty() {
                        continue;
                    }
                    if seen_non_system {
                        contents.push(gt::GeminiContent {
                            parts,
                            role: Some(openai_role_to_gemini(ot::ResponseInputMessageRole::System)),
                        });
                    } else {
                        system_parts.extend(parts);
                    }
                }
                oct::ChatCompletionMessageParam::User(message) => {
                    seen_non_system = true;
                    let content =
                        chat_user_content_to_response_input_message_content(message.content);
                    let parts = openai_message_content_to_gemini_parts(content);
                    if !parts.is_empty() {
                        contents.push(gt::GeminiContent {
                            parts,
                            role: Some(openai_role_to_gemini(ot::ResponseInputMessageRole::User)),
                        });
                    }
                }
                oct::ChatCompletionMessageParam::Assistant(message) => {
                    seen_non_system = true;
                    let oct::ChatCompletionAssistantMessageParam {
                        content,
                        reasoning_content,
                        refusal,
                        function_call,
                        tool_calls,
                        ..
                    } = message;
                    let mut parts = Vec::new();
                    let reasoning_signature = reasoning_content
                        .as_ref()
                        .filter(|text| !text.is_empty())
                        .map(|_| pseudo_reasoning_signature(message_index, 0));
                    let mut first_function_call = true;

                    if let Some(reasoning_content) = reasoning_content
                        && !reasoning_content.is_empty()
                    {
                        parts.push(gt::GeminiPart {
                            thought: Some(true),
                            thought_signature: reasoning_signature.clone(),
                            text: Some(reasoning_content),
                            ..gt::GeminiPart::default()
                        });
                    }

                    if let Some(content) = content {
                        match content {
                            oct::ChatCompletionAssistantContent::Text(text) => {
                                if !text.is_empty() {
                                    parts.push(gt::GeminiPart {
                                        text: Some(text),
                                        ..gt::GeminiPart::default()
                                    });
                                }
                            }
                            oct::ChatCompletionAssistantContent::Parts(content_parts) => {
                                for part in content_parts {
                                    match part {
                                        oct::ChatCompletionAssistantContentPart::Text(part) => {
                                            if !part.text.is_empty() {
                                                parts.push(gt::GeminiPart {
                                                    text: Some(part.text),
                                                    ..gt::GeminiPart::default()
                                                });
                                            }
                                        }
                                        oct::ChatCompletionAssistantContentPart::Refusal(part) => {
                                            if !part.refusal.is_empty() {
                                                parts.push(gt::GeminiPart {
                                                    text: Some(part.refusal),
                                                    ..gt::GeminiPart::default()
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let Some(refusal) = refusal
                        && !refusal.is_empty()
                    {
                        parts.push(gt::GeminiPart {
                            text: Some(refusal),
                            ..gt::GeminiPart::default()
                        });
                    }

                    if let Some(function_call) = function_call {
                        let thought_signature = if first_function_call {
                            first_function_call = false;
                            Some(
                                reasoning_signature
                                    .clone()
                                    .unwrap_or_else(|| GEMINI_SKIP_THOUGHT_SIGNATURE.to_string()),
                            )
                        } else {
                            None
                        };
                        parts.push(gt::GeminiPart {
                            thought_signature,
                            function_call: Some(gt::GeminiFunctionCall {
                                id: Some(format!("function_call_{}", function_call.name)),
                                name: function_call.name,
                                args: Some(parse_tool_use_args(function_call.arguments)),
                            }),
                            ..gt::GeminiPart::default()
                        });
                    }

                    if let Some(tool_calls) = tool_calls {
                        for call in tool_calls {
                            let thought_signature =
                                if first_function_call {
                                    first_function_call = false;
                                    Some(reasoning_signature.clone().unwrap_or_else(|| {
                                        GEMINI_SKIP_THOUGHT_SIGNATURE.to_string()
                                    }))
                                } else {
                                    None
                                };
                            match call {
                                oct::ChatCompletionMessageToolCall::Function(call) => {
                                    parts.push(gt::GeminiPart {
                                        thought_signature,
                                        function_call: Some(gt::GeminiFunctionCall {
                                            id: Some(call.id),
                                            name: call.function.name,
                                            args: Some(parse_tool_use_args(
                                                call.function.arguments,
                                            )),
                                        }),
                                        ..gt::GeminiPart::default()
                                    });
                                }
                                oct::ChatCompletionMessageToolCall::Custom(call) => {
                                    parts.push(gt::GeminiPart {
                                        thought_signature,
                                        function_call: Some(gt::GeminiFunctionCall {
                                            id: Some(call.id),
                                            name: call.custom.name,
                                            args: Some(parse_tool_use_args(call.custom.input)),
                                        }),
                                        ..gt::GeminiPart::default()
                                    });
                                }
                            }
                        }
                    }

                    if !parts.is_empty() {
                        contents.push(gt::GeminiContent {
                            parts,
                            role: Some(gt::GeminiContentRole::Model),
                        });
                    }
                }
                oct::ChatCompletionMessageParam::Tool(message) => {
                    seen_non_system = true;
                    let output_text = chat_text_content_to_plain_text(&message.content);
                    contents.push(gt::GeminiContent {
                        parts: vec![gt::GeminiPart {
                            function_response: Some(gt::GeminiFunctionResponse {
                                id: Some(message.tool_call_id.clone()),
                                name: message.tool_call_id,
                                response: output_text_to_json_object(&output_text),
                                parts: None,
                                will_continue: None,
                                scheduling: None,
                            }),
                            ..gt::GeminiPart::default()
                        }],
                        role: Some(gt::GeminiContentRole::User),
                    });
                }
                oct::ChatCompletionMessageParam::Function(message) => {
                    seen_non_system = true;
                    contents.push(gt::GeminiContent {
                        parts: vec![gt::GeminiPart {
                            function_response: Some(gt::GeminiFunctionResponse {
                                id: Some(message.name.clone()),
                                name: message.name,
                                response: output_text_to_json_object(&message.content),
                                parts: None,
                                will_continue: None,
                                scheduling: None,
                            }),
                            ..gt::GeminiPart::default()
                        }],
                        role: Some(gt::GeminiContentRole::User),
                    });
                }
            }
        }

        let (tools, has_function_calling_tools) = response_tools
            .map(openai_tools_to_gemini)
            .unwrap_or((None, false));

        let tool_config =
            openai_tool_choice_to_gemini(response_tool_choice, has_function_calling_tools);

        let mut generation_config = openai_generation_config(
            response_reasoning,
            response_text,
            max_completion_tokens.or(max_tokens),
            temperature,
            top_p,
            top_logprobs,
        );
        let mut has_generation_config = generation_config.is_some();
        let cached_content = chat_cached_content;
        if let Some(thinking_config) = chat_thinking_config {
            let converted_thinking = gt::GeminiThinkingConfig {
                include_thoughts: thinking_config.include_thoughts,
                thinking_budget: thinking_config.thinking_budget,
                thinking_level: thinking_config.thinking_level.map(|level| match level {
                    oct::ChatCompletionGeminiExtraThinkingLevel::Minimal => {
                        gt::GeminiThinkingLevel::Minimal
                    }
                    oct::ChatCompletionGeminiExtraThinkingLevel::Low => {
                        gt::GeminiThinkingLevel::Low
                    }
                    oct::ChatCompletionGeminiExtraThinkingLevel::Medium => {
                        gt::GeminiThinkingLevel::Medium
                    }
                    oct::ChatCompletionGeminiExtraThinkingLevel::High => {
                        gt::GeminiThinkingLevel::High
                    }
                }),
            };
            generation_config
                .get_or_insert_with(gt::GeminiGenerationConfig::default)
                .thinking_config = Some(converted_thinking);
            has_generation_config = true;
        }

        if let Some(stop_sequences) = chat_stop_to_vec(stop) {
            generation_config
                .get_or_insert_with(gt::GeminiGenerationConfig::default)
                .stop_sequences = Some(stop_sequences);
            has_generation_config = true;
        }

        if let Some(seed) = seed.and_then(|value| u32::try_from(value).ok()) {
            generation_config
                .get_or_insert_with(gt::GeminiGenerationConfig::default)
                .seed = Some(seed);
            has_generation_config = true;
        }

        if let Some(value) = frequency_penalty {
            generation_config
                .get_or_insert_with(gt::GeminiGenerationConfig::default)
                .frequency_penalty = Some(value);
            has_generation_config = true;
        }

        if let Some(value) = presence_penalty {
            generation_config
                .get_or_insert_with(gt::GeminiGenerationConfig::default)
                .presence_penalty = Some(value);
            has_generation_config = true;
        }

        if let Some(value) = n
            && value > 0
        {
            generation_config
                .get_or_insert_with(gt::GeminiGenerationConfig::default)
                .candidate_count = Some(value);
            has_generation_config = true;
        }

        let generation_config = if has_generation_config {
            generation_config
        } else {
            None
        };

        let system_instruction = if system_parts.is_empty() {
            None
        } else {
            Some(gt::GeminiContent {
                parts: system_parts,
                role: None,
            })
        };

        Ok(GeminiGenerateContentRequest {
            method: GeminiHttpMethod::Post,
            path: PathParameters { model },
            query: QueryParameters::default(),
            headers: RequestHeaders::default(),
            body: RequestBody {
                contents,
                tools,
                tool_config,
                safety_settings: None,
                system_instruction,
                generation_config,
                cached_content,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai::create_chat_completions::request as oreq;

    #[test]
    fn chat_reasoning_content_maps_to_gemini_thought_part_with_pseudo_signature() {
        let request = OpenAiChatCompletionsRequest {
            method: oct::HttpMethod::Post,
            path: oreq::PathParameters::default(),
            query: oreq::QueryParameters::default(),
            headers: oreq::RequestHeaders::default(),
            body: oreq::RequestBody {
                model: "gpt-5".to_string(),
                messages: vec![oct::ChatCompletionMessageParam::Assistant(
                    oct::ChatCompletionAssistantMessageParam {
                        role: oct::ChatCompletionAssistantRole::Assistant,
                        audio: None,
                        content: None,
                        reasoning_content: Some("reasoning text".to_string()),
                        function_call: None,
                        name: None,
                        refusal: None,
                        tool_calls: None,
                    },
                )],
                ..oreq::RequestBody::default()
            },
        };

        let converted = GeminiGenerateContentRequest::try_from(request).unwrap();
        let content = converted.body.contents.first().expect("assistant content");
        assert_eq!(content.role, Some(gt::GeminiContentRole::Model));
        let part = content.parts.first().expect("first part");
        assert_eq!(part.thought, Some(true));
        assert_eq!(part.thought_signature.as_deref(), Some("rs_0_0"));
        assert_eq!(part.text.as_deref(), Some("reasoning text"));
    }

    #[test]
    fn assistant_tool_call_without_reasoning_uses_dummy_signature() {
        let request = OpenAiChatCompletionsRequest {
            method: oct::HttpMethod::Post,
            path: oreq::PathParameters::default(),
            query: oreq::QueryParameters::default(),
            headers: oreq::RequestHeaders::default(),
            body: oreq::RequestBody {
                model: "gpt-5".to_string(),
                messages: vec![oct::ChatCompletionMessageParam::Assistant(
                    oct::ChatCompletionAssistantMessageParam {
                        role: oct::ChatCompletionAssistantRole::Assistant,
                        audio: None,
                        content: None,
                        reasoning_content: None,
                        function_call: None,
                        name: None,
                        refusal: None,
                        tool_calls: Some(vec![oct::ChatCompletionMessageToolCall::Function(
                            oct::ChatCompletionMessageFunctionToolCall {
                                id: "call_1".to_string(),
                                function: oct::ChatCompletionFunctionCall {
                                    name: "exec_command".to_string(),
                                    arguments: "{\"cmd\":\"ls\"}".to_string(),
                                },
                                type_: oct::ChatCompletionMessageFunctionToolCallType::Function,
                            },
                        )]),
                    },
                )],
                ..oreq::RequestBody::default()
            },
        };

        let converted = GeminiGenerateContentRequest::try_from(request).unwrap();
        let content = converted.body.contents.first().expect("assistant content");
        let part = content.parts.first().expect("first part");
        assert_eq!(
            part.thought_signature.as_deref(),
            Some(GEMINI_SKIP_THOUGHT_SIGNATURE)
        );
        assert!(part.function_call.is_some());
    }
}
