use crate::claude::count_tokens::types::{
    BetaContentBlockParam, BetaMcpToolResultBlockParamContent, BetaMessageContent, BetaMessageRole,
    BetaOutputEffort, BetaThinkingConfigParam, BetaToolChoice, BetaToolInputSchema,
    BetaToolInputSchemaType, BetaToolResultBlockParamContent, BetaToolResultContentBlockParam,
    BetaToolUnion,
};
use crate::claude::create_message::request::ClaudeCreateMessageRequest;
use crate::claude::create_message::types::{BetaServiceTierParam, BetaSpeed};
use crate::openai::create_chat_completions::request::{
    OpenAiChatCompletionsRequest, PathParameters, QueryParameters, RequestBody, RequestHeaders,
};
use crate::openai::create_chat_completions::types::{
    ChatCompletionAssistantContent, ChatCompletionAssistantMessageParam,
    ChatCompletionAssistantRole, ChatCompletionContentPart, ChatCompletionContentPartFile,
    ChatCompletionContentPartFileType, ChatCompletionContentPartImage,
    ChatCompletionContentPartImageType, ChatCompletionContentPartText,
    ChatCompletionContentPartTextType, ChatCompletionFileInput, ChatCompletionFunctionCall,
    ChatCompletionFunctionDefinition, ChatCompletionFunctionTool, ChatCompletionFunctionToolType,
    ChatCompletionImageUrl, ChatCompletionMessageCustomToolCall,
    ChatCompletionMessageCustomToolCallPayload, ChatCompletionMessageCustomToolCallType,
    ChatCompletionMessageFunctionToolCall, ChatCompletionMessageFunctionToolCallType,
    ChatCompletionMessageParam, ChatCompletionMessageToolCall, ChatCompletionNamedFunction,
    ChatCompletionNamedToolChoice, ChatCompletionNamedToolChoiceType,
    ChatCompletionReasoningEffort, ChatCompletionResponseFormat,
    ChatCompletionResponseFormatJsonSchema, ChatCompletionResponseFormatJsonSchemaConfig,
    ChatCompletionResponseFormatJsonSchemaType, ChatCompletionServiceTier, ChatCompletionStop,
    ChatCompletionSystemMessageParam, ChatCompletionSystemRole, ChatCompletionTextContent,
    ChatCompletionTool, ChatCompletionToolChoiceMode, ChatCompletionToolChoiceOption,
    ChatCompletionToolMessageParam, ChatCompletionToolRole, ChatCompletionUserContent,
    ChatCompletionUserMessageParam, ChatCompletionUserRole, ChatCompletionVerbosity, HttpMethod,
    Metadata,
};
use crate::transform::claude::generate_content::utils::{
    beta_message_content_to_text, beta_system_prompt_to_text, claude_model_to_string,
};
use crate::transform::utils::TransformError;
use serde_json::{Map, Value};

fn tool_input_schema_to_function_parameters(
    input_schema: BetaToolInputSchema,
) -> std::collections::BTreeMap<String, Value> {
    let mut parameters = std::collections::BTreeMap::new();
    let schema_type = match input_schema.type_ {
        BetaToolInputSchemaType::Object => "object",
    };
    parameters.insert("type".to_string(), Value::String(schema_type.to_string()));
    if let Some(properties) = input_schema.properties {
        let properties_object = properties.into_iter().collect::<Map<String, Value>>();
        parameters.insert("properties".to_string(), Value::Object(properties_object));
    }
    if let Some(required) = input_schema.required {
        parameters.insert(
            "required".to_string(),
            Value::Array(required.into_iter().map(Value::String).collect()),
        );
    }
    parameters
}

impl TryFrom<ClaudeCreateMessageRequest> for OpenAiChatCompletionsRequest {
    type Error = TransformError;

    fn try_from(value: ClaudeCreateMessageRequest) -> Result<Self, TransformError> {
        let body = value.body;
        let model = claude_model_to_string(&body.model);

        let mut messages = Vec::new();
        if let Some(system) = beta_system_prompt_to_text(body.system) {
            messages.push(ChatCompletionMessageParam::System(
                ChatCompletionSystemMessageParam {
                    content: ChatCompletionTextContent::Text(system),
                    role: ChatCompletionSystemRole::System,
                    name: None,
                },
            ));
        }

        for message in body.messages {
            let fallback_text = beta_message_content_to_text(&message.content);
            match (message.role, message.content) {
                (BetaMessageRole::User, BetaMessageContent::Text(text)) => {
                    messages.push(ChatCompletionMessageParam::User(
                        ChatCompletionUserMessageParam {
                            content: ChatCompletionUserContent::Text(text),
                            role: ChatCompletionUserRole::User,
                            name: None,
                        },
                    ));
                }
                (BetaMessageRole::User, BetaMessageContent::Blocks(blocks)) => {
                    let mut user_parts = Vec::new();
                    let mut tool_messages = Vec::new();

                    for block in blocks {
                        match block {
                            BetaContentBlockParam::Text(block) => {
                                user_parts.push(ChatCompletionContentPart::Text(
                                    ChatCompletionContentPartText {
                                        text: block.text,
                                        type_: ChatCompletionContentPartTextType::Text,
                                    },
                                ));
                            }
                            BetaContentBlockParam::Image(block) => match block.source {
                                crate::claude::count_tokens::types::BetaImageSource::Base64(
                                    source,
                                ) => {
                                    let mime_type = match source.media_type {
                                        crate::claude::count_tokens::types::BetaImageMediaType::ImageJpeg => "image/jpeg",
                                        crate::claude::count_tokens::types::BetaImageMediaType::ImagePng => "image/png",
                                        crate::claude::count_tokens::types::BetaImageMediaType::ImageGif => "image/gif",
                                        crate::claude::count_tokens::types::BetaImageMediaType::ImageWebp => "image/webp",
                                    };
                                    user_parts.push(ChatCompletionContentPart::Image(
                                        ChatCompletionContentPartImage {
                                            image_url: ChatCompletionImageUrl {
                                                url: format!(
                                                    "data:{mime_type};base64,{}",
                                                    source.data
                                                ),
                                                detail: None,
                                            },
                                            type_: ChatCompletionContentPartImageType::ImageUrl,
                                        },
                                    ));
                                }
                                crate::claude::count_tokens::types::BetaImageSource::Url(source) => {
                                    user_parts.push(ChatCompletionContentPart::Image(
                                        ChatCompletionContentPartImage {
                                            image_url: ChatCompletionImageUrl {
                                                url: source.url,
                                                detail: None,
                                            },
                                            type_: ChatCompletionContentPartImageType::ImageUrl,
                                        },
                                    ));
                                }
                                crate::claude::count_tokens::types::BetaImageSource::File(source) => {
                                    user_parts.push(ChatCompletionContentPart::File(
                                        ChatCompletionContentPartFile {
                                            file: ChatCompletionFileInput {
                                                file_data: None,
                                                file_id: Some(source.file_id),
                                                file_url: None,
                                                filename: None,
                                            },
                                            type_: ChatCompletionContentPartFileType::File,
                                        },
                                    ));
                                }
                            },
                            BetaContentBlockParam::RequestDocument(block) => match block.source {
                                crate::claude::count_tokens::types::BetaDocumentSource::Base64Pdf(
                                    source,
                                ) => {
                                    user_parts.push(ChatCompletionContentPart::File(
                                        ChatCompletionContentPartFile {
                                            file: ChatCompletionFileInput {
                                                file_data: Some(source.data),
                                                file_id: None,
                                                file_url: None,
                                                filename: block.title.clone(),
                                            },
                                            type_: ChatCompletionContentPartFileType::File,
                                        },
                                    ));
                                }
                                crate::claude::count_tokens::types::BetaDocumentSource::PlainText(
                                    source,
                                ) => {
                                    user_parts.push(ChatCompletionContentPart::File(
                                        ChatCompletionContentPartFile {
                                            file: ChatCompletionFileInput {
                                                file_data: Some(source.data),
                                                file_id: None,
                                                file_url: None,
                                                filename: block.title.clone(),
                                            },
                                            type_: ChatCompletionContentPartFileType::File,
                                        },
                                    ));
                                }
                                crate::claude::count_tokens::types::BetaDocumentSource::File(
                                    source,
                                ) => {
                                    user_parts.push(ChatCompletionContentPart::File(
                                        ChatCompletionContentPartFile {
                                            file: ChatCompletionFileInput {
                                                file_data: None,
                                                file_id: Some(source.file_id),
                                                file_url: None,
                                                filename: block.title.clone(),
                                            },
                                            type_: ChatCompletionContentPartFileType::File,
                                        },
                                    ));
                                }
                                _ => {}
                            },
                            BetaContentBlockParam::ToolResult(block) => {
                                let output = match block.content {
                                    Some(BetaToolResultBlockParamContent::Text(text)) => text,
                                    Some(BetaToolResultBlockParamContent::Blocks(parts)) => parts
                                        .into_iter()
                                        .filter_map(|part| match part {
                                            BetaToolResultContentBlockParam::Text(part) => {
                                                Some(part.text)
                                            }
                                            _ => None,
                                        })
                                        .collect::<Vec<_>>()
                                        .join("\n"),
                                    None => String::new(),
                                };
                                tool_messages.push(ChatCompletionMessageParam::Tool(
                                    ChatCompletionToolMessageParam {
                                        content: ChatCompletionTextContent::Text(output),
                                        role: ChatCompletionToolRole::Tool,
                                        tool_call_id: block.tool_use_id,
                                    },
                                ));
                            }
                            BetaContentBlockParam::McpToolResult(block) => {
                                let output = match block.content {
                                    Some(BetaMcpToolResultBlockParamContent::Text(text)) => text,
                                    Some(BetaMcpToolResultBlockParamContent::Blocks(parts)) => parts
                                        .into_iter()
                                        .map(|part| part.text)
                                        .collect::<Vec<_>>()
                                        .join("\n"),
                                    None => String::new(),
                                };
                                tool_messages.push(ChatCompletionMessageParam::Tool(
                                    ChatCompletionToolMessageParam {
                                        content: ChatCompletionTextContent::Text(output),
                                        role: ChatCompletionToolRole::Tool,
                                        tool_call_id: block.tool_use_id,
                                    },
                                ));
                            }
                            _ => {}
                        }
                    }

                    if user_parts.is_empty() {
                        messages.push(ChatCompletionMessageParam::User(
                            ChatCompletionUserMessageParam {
                                content: ChatCompletionUserContent::Text(fallback_text),
                                role: ChatCompletionUserRole::User,
                                name: None,
                            },
                        ));
                    } else {
                        messages.push(ChatCompletionMessageParam::User(
                            ChatCompletionUserMessageParam {
                                content: ChatCompletionUserContent::Parts(user_parts),
                                role: ChatCompletionUserRole::User,
                                name: None,
                            },
                        ));
                    }

                    messages.extend(tool_messages);
                }
                (BetaMessageRole::Assistant, BetaMessageContent::Text(text)) => {
                    messages.push(ChatCompletionMessageParam::Assistant(
                        ChatCompletionAssistantMessageParam {
                            role: ChatCompletionAssistantRole::Assistant,
                            audio: None,
                            content: Some(ChatCompletionAssistantContent::Text(text)),
                            reasoning_content: None,
                            function_call: None,
                            name: None,
                            refusal: None,
                            tool_calls: None,
                        },
                    ));
                }
                (BetaMessageRole::Assistant, BetaMessageContent::Blocks(blocks)) => {
                    let mut assistant_text_parts = Vec::new();
                    let mut tool_calls = Vec::new();

                    for block in blocks {
                        match block {
                            BetaContentBlockParam::Text(block) => {
                                assistant_text_parts.push(block.text);
                            }
                            BetaContentBlockParam::ToolUse(block) => {
                                tool_calls.push(ChatCompletionMessageToolCall::Function(
                                    ChatCompletionMessageFunctionToolCall {
                                        id: block.id,
                                        function: ChatCompletionFunctionCall {
                                            arguments: serde_json::to_string(&block.input)
                                                .unwrap_or_else(|_| "{}".to_string()),
                                            name: block.name,
                                        },
                                        type_: ChatCompletionMessageFunctionToolCallType::Function,
                                    },
                                ));
                            }
                            BetaContentBlockParam::ServerToolUse(block) => {
                                tool_calls.push(ChatCompletionMessageToolCall::Custom(
                                    ChatCompletionMessageCustomToolCall {
                                        id: block.id,
                                        custom: ChatCompletionMessageCustomToolCallPayload {
                                            input: serde_json::to_string(&block.input)
                                                .unwrap_or_else(|_| "{}".to_string()),
                                            name: match block.name {
                                                crate::claude::count_tokens::types::BetaServerToolUseName::WebSearch => "web_search".to_string(),
                                                crate::claude::count_tokens::types::BetaServerToolUseName::WebFetch => "web_fetch".to_string(),
                                                crate::claude::count_tokens::types::BetaServerToolUseName::CodeExecution => "code_execution".to_string(),
                                                crate::claude::count_tokens::types::BetaServerToolUseName::BashCodeExecution => "bash_code_execution".to_string(),
                                                crate::claude::count_tokens::types::BetaServerToolUseName::TextEditorCodeExecution => "text_editor_code_execution".to_string(),
                                                crate::claude::count_tokens::types::BetaServerToolUseName::ToolSearchToolRegex => "tool_search_tool_regex".to_string(),
                                                crate::claude::count_tokens::types::BetaServerToolUseName::ToolSearchToolBm25 => "tool_search_tool_bm25".to_string(),
                                            },
                                        },
                                        type_: ChatCompletionMessageCustomToolCallType::Custom,
                                    },
                                ));
                            }
                            BetaContentBlockParam::McpToolUse(block) => {
                                tool_calls.push(ChatCompletionMessageToolCall::Custom(
                                    ChatCompletionMessageCustomToolCall {
                                        id: block.id,
                                        custom: ChatCompletionMessageCustomToolCallPayload {
                                            input: serde_json::to_string(&block.input)
                                                .unwrap_or_else(|_| "{}".to_string()),
                                            name: format!(
                                                "mcp:{}:{}",
                                                block.server_name, block.name
                                            ),
                                        },
                                        type_: ChatCompletionMessageCustomToolCallType::Custom,
                                    },
                                ));
                            }
                            _ => {}
                        }
                    }

                    let content_text = if assistant_text_parts.is_empty() {
                        if fallback_text.is_empty() {
                            None
                        } else {
                            Some(fallback_text)
                        }
                    } else {
                        Some(assistant_text_parts.join("\n"))
                    };

                    messages.push(ChatCompletionMessageParam::Assistant(
                        ChatCompletionAssistantMessageParam {
                            role: ChatCompletionAssistantRole::Assistant,
                            audio: None,
                            content: content_text.map(ChatCompletionAssistantContent::Text),
                            reasoning_content: None,
                            function_call: None,
                            name: None,
                            refusal: None,
                            tool_calls: if tool_calls.is_empty() {
                                None
                            } else {
                                Some(tool_calls)
                            },
                        },
                    ));
                }
            }
        }

        let parallel_tool_calls = match body.tool_choice.as_ref() {
            Some(BetaToolChoice::Auto(choice)) => choice.disable_parallel_tool_use.map(|v| !v),
            Some(BetaToolChoice::Any(choice)) => choice.disable_parallel_tool_use.map(|v| !v),
            Some(BetaToolChoice::Tool(choice)) => choice.disable_parallel_tool_use.map(|v| !v),
            Some(BetaToolChoice::None(_)) | None => None,
        };
        let tool_choice = match body.tool_choice {
            Some(BetaToolChoice::Auto(_)) => Some(ChatCompletionToolChoiceOption::Mode(
                ChatCompletionToolChoiceMode::Auto,
            )),
            Some(BetaToolChoice::Any(_)) => Some(ChatCompletionToolChoiceOption::Mode(
                ChatCompletionToolChoiceMode::Required,
            )),
            Some(BetaToolChoice::Tool(choice)) => Some(
                ChatCompletionToolChoiceOption::NamedFunction(ChatCompletionNamedToolChoice {
                    function: ChatCompletionNamedFunction { name: choice.name },
                    type_: ChatCompletionNamedToolChoiceType::Function,
                }),
            ),
            Some(BetaToolChoice::None(_)) => Some(ChatCompletionToolChoiceOption::Mode(
                ChatCompletionToolChoiceMode::None,
            )),
            None => None,
        };
        let reasoning_effort_from_thinking = match body.thinking {
            Some(BetaThinkingConfigParam::Enabled(config)) => Some(if config.budget_tokens == 0 {
                ChatCompletionReasoningEffort::None
            } else if config.budget_tokens <= 4096 {
                ChatCompletionReasoningEffort::Minimal
            } else if config.budget_tokens <= 8192 {
                ChatCompletionReasoningEffort::Low
            } else if config.budget_tokens <= 16384 {
                ChatCompletionReasoningEffort::Medium
            } else if config.budget_tokens <= 32768 {
                ChatCompletionReasoningEffort::High
            } else {
                ChatCompletionReasoningEffort::XHigh
            }),
            Some(BetaThinkingConfigParam::Disabled(_)) => Some(ChatCompletionReasoningEffort::None),
            Some(BetaThinkingConfigParam::Adaptive(_)) => {
                Some(ChatCompletionReasoningEffort::Medium)
            }
            None => None,
        };
        let reasoning_effort = reasoning_effort_from_thinking;
        let verbosity = body
            .output_config
            .as_ref()
            .and_then(|config| config.effort.as_ref())
            .map(|effort| match effort {
                BetaOutputEffort::Low => ChatCompletionVerbosity::Low,
                BetaOutputEffort::Medium => ChatCompletionVerbosity::Medium,
                BetaOutputEffort::High | BetaOutputEffort::XHigh | BetaOutputEffort::Max => {
                    ChatCompletionVerbosity::High
                }
            });

        let response_format = body
            .output_config
            .as_ref()
            .and_then(|config| config.format.as_ref())
            .map(|schema| {
                ChatCompletionResponseFormat::JsonSchema(ChatCompletionResponseFormatJsonSchema {
                    json_schema: ChatCompletionResponseFormatJsonSchemaConfig {
                        name: "output".to_string(),
                        description: None,
                        schema: Some(schema.schema.clone()),
                        strict: None,
                    },
                    type_: ChatCompletionResponseFormatJsonSchemaType::JsonSchema,
                })
            });

        let tools = body.tools.map(|items| {
            items
                .into_iter()
                .filter_map(|tool| match tool {
                    BetaToolUnion::Custom(tool) => {
                        Some(ChatCompletionTool::Function(ChatCompletionFunctionTool {
                            function: ChatCompletionFunctionDefinition {
                                name: tool.name,
                                description: tool.description,
                                parameters: Some(tool_input_schema_to_function_parameters(
                                    tool.input_schema,
                                )),
                                strict: tool.common.strict,
                            },
                            type_: ChatCompletionFunctionToolType::Function,
                        }))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        });

        let stop = body
            .stop_sequences
            .and_then(|sequences| match sequences.len() {
                0 => None,
                1 => Some(ChatCompletionStop::Single(
                    sequences.into_iter().next().unwrap_or_default(),
                )),
                _ => Some(ChatCompletionStop::Multiple(sequences)),
            });
        let service_tier = match body.service_tier {
            Some(BetaServiceTierParam::Auto) => Some(ChatCompletionServiceTier::Auto),
            Some(BetaServiceTierParam::StandardOnly) => Some(ChatCompletionServiceTier::Default),
            None => match body.speed {
                Some(BetaSpeed::Fast) => Some(ChatCompletionServiceTier::Priority),
                Some(BetaSpeed::Standard) | None => None,
            },
        };
        let metadata = if let Some(user_id) = body
            .metadata
            .as_ref()
            .and_then(|value| value.user_id.clone())
        {
            let mut map = Metadata::new();
            map.insert("user_id".to_string(), user_id);
            Some(map)
        } else {
            None
        };

        Ok(Self {
            method: HttpMethod::Post,
            path: PathParameters::default(),
            query: QueryParameters::default(),
            headers: RequestHeaders::default(),
            body: RequestBody {
                messages,
                model,
                max_completion_tokens: Some(body.max_tokens),
                metadata,
                parallel_tool_calls,
                reasoning_effort,
                response_format,
                service_tier,
                stop,
                stream: body.stream,
                temperature: body.temperature,
                tool_choice,
                tools,
                top_p: body.top_p,
                verbosity,
                ..RequestBody::default()
            },
        })
    }
}
