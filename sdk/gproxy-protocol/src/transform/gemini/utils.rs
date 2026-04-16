use crate::claude::count_tokens::types::{
    BetaBase64ImageSource, BetaBase64SourceType, BetaCodeExecutionTool20250825,
    BetaCodeExecutionTool20250825Type, BetaCodeExecutionToolName, BetaComputerToolName,
    BetaContentBlockParam, BetaImageBlockParam, BetaImageBlockType, BetaImageMediaType,
    BetaImageSource, BetaJsonOutputFormat, BetaJsonOutputFormatType, BetaMessageParam,
    BetaMessageRole, BetaOutputConfig, BetaOutputEffort, BetaSystemPrompt, BetaTextBlockParam,
    BetaTextBlockType, BetaThinkingBlockParam, BetaThinkingBlockType, BetaThinkingConfigAdaptive,
    BetaThinkingConfigAdaptiveType, BetaThinkingConfigDisabled, BetaThinkingConfigDisabledType,
    BetaThinkingConfigEnabled, BetaThinkingConfigEnabledType, BetaThinkingConfigParam, BetaTool,
    BetaToolChoice, BetaToolChoiceAny, BetaToolChoiceAnyType, BetaToolChoiceAuto,
    BetaToolChoiceAutoType, BetaToolChoiceNone, BetaToolChoiceNoneType, BetaToolChoiceTool,
    BetaToolChoiceToolType, BetaToolCommonFields, BetaToolComputerUse20251124,
    BetaToolComputerUse20251124Type, BetaToolInputSchema, BetaToolInputSchemaType,
    BetaToolResultBlockParam, BetaToolResultBlockParamContent, BetaToolResultBlockType,
    BetaToolSearchToolBm25_20251119, BetaToolSearchToolBm25Name, BetaToolSearchToolBm25Type,
    BetaToolUnion, BetaToolUseBlockParam, BetaToolUseBlockType, BetaWebFetchTool20250910,
    BetaWebFetchTool20250910Type, BetaWebFetchToolName, BetaWebSearchTool20250305,
    BetaWebSearchTool20250305Type, BetaWebSearchToolName,
};
use crate::gemini::count_tokens::types::{
    GeminiContent, GeminiContentRole, GeminiFunctionCallingMode, GeminiGenerationConfig,
    GeminiThinkingConfig, GeminiThinkingLevel, GeminiTool, GeminiToolConfig,
};
use crate::openai::count_tokens::types::ResponseReasoningEffort;
use crate::openai::create_chat_completions::types::ChatCompletionReasoningEffort;
use crate::transform::claude::utils::claude_model_supports_enabled_thinking;
use crate::transform::utils::push_message_block;

pub fn strip_models_prefix(value: &str) -> String {
    value.strip_prefix("models/").unwrap_or(value).to_string()
}

pub fn gemini_content_to_text(content: &GeminiContent) -> String {
    content
        .parts
        .iter()
        .filter_map(|part| {
            if let Some(text) = part.text.as_ref() {
                return Some(text.clone());
            }
            part.file_data.as_ref().map(|file| file.file_uri.clone())
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn gemini_contents_to_claude_messages(contents: Vec<GeminiContent>) -> Vec<BetaMessageParam> {
    let mut messages = Vec::new();
    for content in contents {
        let role = match content.role.unwrap_or(GeminiContentRole::User) {
            GeminiContentRole::User => BetaMessageRole::User,
            GeminiContentRole::Model => BetaMessageRole::Assistant,
        };

        for (index, part) in content.parts.into_iter().enumerate() {
            if let Some(text) = part.text
                && !text.is_empty()
            {
                if part.thought.unwrap_or(false) {
                    push_message_block(
                        &mut messages,
                        role.clone(),
                        BetaContentBlockParam::Thinking(BetaThinkingBlockParam {
                            signature: part
                                .thought_signature
                                .unwrap_or_else(|| format!("thought_{index}")),
                            thinking: text,
                            type_: BetaThinkingBlockType::Thinking,
                        }),
                    );
                } else {
                    push_message_block(
                        &mut messages,
                        role.clone(),
                        BetaContentBlockParam::Text(BetaTextBlockParam {
                            text,
                            type_: BetaTextBlockType::Text,
                            cache_control: None,
                            citations: None,
                        }),
                    );
                }
            }

            if let Some(function_call) = part.function_call {
                push_message_block(
                    &mut messages,
                    role.clone(),
                    BetaContentBlockParam::ToolUse(BetaToolUseBlockParam {
                        id: function_call
                            .id
                            .unwrap_or_else(|| format!("tool_use_{index}")),
                        input: function_call.args.unwrap_or_default(),
                        name: function_call.name,
                        type_: BetaToolUseBlockType::ToolUse,
                        cache_control: None,
                        caller: None,
                    }),
                );
            }

            if let Some(function_response) = part.function_response {
                // Gemini's function_response carries the result for an
                // earlier function_call. Emit it as a Claude tool_result so
                // the assistant can see what the tool returned.
                // `push_message_block` handles the pairing rule by
                // synthesising a placeholder tool_use when the matching
                // function_call is not present in the same request — this
                // happens when a client only sends the new tool outputs
                // and relies on server-side history reconstruction we
                // don't have access to.
                let tool_use_id = function_response
                    .id
                    .unwrap_or_else(|| format!("tool_use_{index}"));
                let response_text = if function_response.response.is_empty() {
                    String::new()
                } else {
                    serde_json::to_string(&function_response.response).unwrap_or_default()
                };
                push_message_block(
                    &mut messages,
                    BetaMessageRole::User,
                    BetaContentBlockParam::ToolResult(BetaToolResultBlockParam {
                        tool_use_id,
                        type_: BetaToolResultBlockType::ToolResult,
                        cache_control: None,
                        content: if response_text.is_empty() {
                            None
                        } else {
                            Some(BetaToolResultBlockParamContent::Text(response_text))
                        },
                        is_error: None,
                    }),
                );
            }

            if let Some(inline_data) = part.inline_data {
                let image_media_type = match inline_data.mime_type.as_str() {
                    "image/jpeg" => Some(BetaImageMediaType::ImageJpeg),
                    "image/png" => Some(BetaImageMediaType::ImagePng),
                    "image/gif" => Some(BetaImageMediaType::ImageGif),
                    "image/webp" => Some(BetaImageMediaType::ImageWebp),
                    _ => None,
                };

                if let Some(media_type) = image_media_type {
                    push_message_block(
                        &mut messages,
                        role.clone(),
                        BetaContentBlockParam::Image(BetaImageBlockParam {
                            source: BetaImageSource::Base64(BetaBase64ImageSource {
                                data: inline_data.data,
                                media_type,
                                type_: BetaBase64SourceType::Base64,
                            }),
                            type_: BetaImageBlockType::Image,
                            cache_control: None,
                        }),
                    );
                } else {
                    push_message_block(
                        &mut messages,
                        role.clone(),
                        BetaContentBlockParam::Text(BetaTextBlockParam {
                            text: format!(
                                "inline_data({}): {}",
                                inline_data.mime_type, inline_data.data
                            ),
                            type_: BetaTextBlockType::Text,
                            cache_control: None,
                            citations: None,
                        }),
                    );
                }
            }

            if let Some(file_data) = part.file_data {
                let text = if let Some(mime_type) = file_data.mime_type {
                    format!("file_data({mime_type}): {}", file_data.file_uri)
                } else {
                    file_data.file_uri
                };
                if !text.is_empty() {
                    push_message_block(
                        &mut messages,
                        role.clone(),
                        BetaContentBlockParam::Text(BetaTextBlockParam {
                            text,
                            type_: BetaTextBlockType::Text,
                            cache_control: None,
                            citations: None,
                        }),
                    );
                }
            }
        }
    }
    messages
}

pub fn gemini_system_instruction_to_claude(
    system_instruction: Option<GeminiContent>,
) -> Option<BetaSystemPrompt> {
    system_instruction.and_then(|instruction| {
        let text = instruction
            .parts
            .into_iter()
            .filter_map(|part| part.text)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        if text.is_empty() {
            None
        } else {
            Some(BetaSystemPrompt::Text(text))
        }
    })
}

pub fn gemini_tool_choice_to_claude(
    tool_config: Option<GeminiToolConfig>,
) -> Option<BetaToolChoice> {
    tool_config
        .and_then(|config| config.function_calling_config)
        .map(|config| {
            if let Some(name) = config
                .allowed_function_names
                .as_ref()
                .and_then(|names| names.first())
                .cloned()
            {
                return BetaToolChoice::Tool(BetaToolChoiceTool {
                    name,
                    type_: BetaToolChoiceToolType::Tool,
                    disable_parallel_tool_use: None,
                });
            }

            match config
                .mode
                .unwrap_or(GeminiFunctionCallingMode::ModeUnspecified)
            {
                GeminiFunctionCallingMode::Auto | GeminiFunctionCallingMode::ModeUnspecified => {
                    BetaToolChoice::Auto(BetaToolChoiceAuto {
                        type_: BetaToolChoiceAutoType::Auto,
                        disable_parallel_tool_use: None,
                    })
                }
                GeminiFunctionCallingMode::Any | GeminiFunctionCallingMode::Validated => {
                    BetaToolChoice::Any(BetaToolChoiceAny {
                        type_: BetaToolChoiceAnyType::Any,
                        disable_parallel_tool_use: None,
                    })
                }
                GeminiFunctionCallingMode::None => BetaToolChoice::None(BetaToolChoiceNone {
                    type_: BetaToolChoiceNoneType::None,
                }),
            }
        })
}

pub fn gemini_tools_to_claude(tools: Option<Vec<GeminiTool>>) -> Option<Vec<BetaToolUnion>> {
    tools.and_then(|tool_defs| {
        let mut mapped = Vec::new();
        for tool in tool_defs {
            if let Some(function_declarations) = tool.function_declarations {
                for declaration in function_declarations {
                    let input_schema = declaration
                        .parameters_json_schema
                        .or_else(|| {
                            declaration
                                .parameters
                                .and_then(|schema| serde_json::to_value(schema).ok())
                        })
                        .map(gemini_parameters_schema_to_claude_input_schema)
                        .unwrap_or_else(default_claude_tool_input_schema);

                    mapped.push(BetaToolUnion::Custom(BetaTool {
                        input_schema,
                        name: declaration.name,
                        common: BetaToolCommonFields::default(),
                        description: if declaration.description.is_empty() {
                            None
                        } else {
                            Some(declaration.description)
                        },
                        eager_input_streaming: None,
                        type_: None,
                    }));
                }
            }

            if tool.code_execution.is_some() {
                mapped.push(BetaToolUnion::CodeExecution20250825(
                    BetaCodeExecutionTool20250825 {
                        name: BetaCodeExecutionToolName::CodeExecution,
                        type_: BetaCodeExecutionTool20250825Type::CodeExecution20250825,
                        common: BetaToolCommonFields::default(),
                    },
                ));
            }

            if tool.computer_use.is_some() {
                mapped.push(BetaToolUnion::ComputerUse20251124(
                    BetaToolComputerUse20251124 {
                        display_height_px: 1024,
                        display_width_px: 1024,
                        name: BetaComputerToolName::Computer,
                        type_: BetaToolComputerUse20251124Type::Computer20251124,
                        common: BetaToolCommonFields::default(),
                        display_number: None,
                        enable_zoom: None,
                    },
                ));
            }

            if tool.google_search.is_some() {
                mapped.push(BetaToolUnion::WebSearch20250305(
                    BetaWebSearchTool20250305 {
                        name: BetaWebSearchToolName::WebSearch,
                        type_: BetaWebSearchTool20250305Type::WebSearch20250305,
                        common: BetaToolCommonFields::default(),
                        allowed_domains: None,
                        blocked_domains: None,
                        max_uses: None,
                        user_location: None,
                    },
                ));
            }

            if tool.url_context.is_some() {
                mapped.push(BetaToolUnion::WebFetch20250910(BetaWebFetchTool20250910 {
                    name: BetaWebFetchToolName::WebFetch,
                    type_: BetaWebFetchTool20250910Type::WebFetch20250910,
                    common: BetaToolCommonFields::default(),
                    allowed_domains: None,
                    blocked_domains: None,
                    citations: None,
                    max_content_tokens: None,
                    max_uses: None,
                }));
            }

            if tool.file_search.is_some() {
                mapped.push(BetaToolUnion::ToolSearchBm25_20251119(
                    BetaToolSearchToolBm25_20251119 {
                        name: BetaToolSearchToolBm25Name::ToolSearchToolBm25,
                        type_: BetaToolSearchToolBm25Type::ToolSearchToolBm2520251119,
                        common: BetaToolCommonFields::default(),
                    },
                ));
            }
        }

        if mapped.is_empty() {
            None
        } else {
            Some(mapped)
        }
    })
}

fn default_claude_tool_input_schema() -> BetaToolInputSchema {
    BetaToolInputSchema {
        type_: BetaToolInputSchemaType::Object,
        properties: None,
        required: None,
        extra_fields: Default::default(),
    }
}

fn gemini_parameters_schema_to_claude_input_schema(
    value: serde_json::Value,
) -> BetaToolInputSchema {
    let serde_json::Value::Object(mut schema) = value else {
        return default_claude_tool_input_schema();
    };

    let required = schema.remove("required").and_then(|value| match value {
        serde_json::Value::Array(values) => {
            let required = values
                .into_iter()
                .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                .collect::<Vec<_>>();
            if required.is_empty() {
                None
            } else {
                Some(required)
            }
        }
        _ => None,
    });

    let properties = schema.remove("properties").and_then(|value| match value {
        serde_json::Value::Object(map) => Some(map.into_iter().collect()),
        _ => None,
    });

    // Claude custom tool input_schema expects an object schema.
    let _ = schema.remove("type");

    BetaToolInputSchema {
        type_: BetaToolInputSchemaType::Object,
        properties,
        required,
        extra_fields: schema.into_iter().collect(),
    }
}

fn gemini_thinking_effort_bucket(thinking: &GeminiThinkingConfig) -> Option<u8> {
    if thinking.include_thoughts == Some(false) {
        return Some(0);
    }

    if let Some(level) = thinking.thinking_level.as_ref() {
        return Some(match level {
            GeminiThinkingLevel::ThinkingLevelUnspecified => 3,
            GeminiThinkingLevel::Minimal => 1,
            GeminiThinkingLevel::Low => 2,
            GeminiThinkingLevel::Medium => 3,
            GeminiThinkingLevel::High => 4,
        });
    }

    thinking.thinking_budget.map(|budget| {
        if budget <= 0 {
            0
        } else if budget <= 4096 {
            1
        } else if budget <= 8192 {
            2
        } else if budget <= 16384 {
            3
        } else if budget <= 32768 {
            4
        } else {
            5
        }
    })
}

pub fn openai_reasoning_effort_from_gemini_thinking(
    thinking: &GeminiThinkingConfig,
) -> Option<ResponseReasoningEffort> {
    gemini_thinking_effort_bucket(thinking).map(|bucket| match bucket {
        0 => ResponseReasoningEffort::None,
        1 => ResponseReasoningEffort::Minimal,
        2 => ResponseReasoningEffort::Low,
        3 => ResponseReasoningEffort::Medium,
        4 => ResponseReasoningEffort::High,
        _ => ResponseReasoningEffort::XHigh,
    })
}

pub fn openai_chat_reasoning_effort_from_gemini_thinking(
    thinking: &GeminiThinkingConfig,
) -> Option<ChatCompletionReasoningEffort> {
    gemini_thinking_effort_bucket(thinking).map(|bucket| match bucket {
        0 => ChatCompletionReasoningEffort::None,
        1 => ChatCompletionReasoningEffort::Minimal,
        2 => ChatCompletionReasoningEffort::Low,
        3 => ChatCompletionReasoningEffort::Medium,
        4 => ChatCompletionReasoningEffort::High,
        _ => ChatCompletionReasoningEffort::XHigh,
    })
}

pub fn claude_thinking_from_gemini(
    thinking: &GeminiThinkingConfig,
    model: Option<&crate::claude::count_tokens::types::Model>,
) -> Option<BetaThinkingConfigParam> {
    if matches!(thinking.include_thoughts, Some(false)) {
        return Some(BetaThinkingConfigParam::Disabled(
            BetaThinkingConfigDisabled {
                type_: BetaThinkingConfigDisabledType::Disabled,
            },
        ));
    }

    if let Some(budget) = thinking.thinking_budget {
        if !claude_model_supports_enabled_thinking(model) {
            return Some(BetaThinkingConfigParam::Adaptive(
                BetaThinkingConfigAdaptive {
                    type_: BetaThinkingConfigAdaptiveType::Adaptive,
                    display: None,
                },
            ));
        }
        return Some(BetaThinkingConfigParam::Enabled(
            BetaThinkingConfigEnabled {
                budget_tokens: u64::try_from(budget).unwrap_or(0),
                type_: BetaThinkingConfigEnabledType::Enabled,
                display: None,
            },
        ));
    }

    if thinking.thinking_level.is_some() {
        return Some(BetaThinkingConfigParam::Adaptive(
            BetaThinkingConfigAdaptive {
                type_: BetaThinkingConfigAdaptiveType::Adaptive,
                display: None,
            },
        ));
    }

    None
}

pub fn claude_output_effort_from_gemini_level(
    level: &GeminiThinkingLevel,
) -> Option<BetaOutputEffort> {
    match level {
        GeminiThinkingLevel::Minimal | GeminiThinkingLevel::Low => Some(BetaOutputEffort::Low),
        GeminiThinkingLevel::Medium => Some(BetaOutputEffort::Medium),
        GeminiThinkingLevel::High => Some(BetaOutputEffort::High),
        GeminiThinkingLevel::ThinkingLevelUnspecified => None,
    }
}

pub fn claude_output_format_from_gemini_generation_config(
    generation_config: &GeminiGenerationConfig,
) -> Option<BetaJsonOutputFormat> {
    generation_config
        .response_json_schema
        .clone()
        .or(generation_config.response_json_schema_legacy.clone())
        .and_then(|value| {
            serde_json::from_value::<crate::claude::count_tokens::types::JsonObject>(value).ok()
        })
        .map(|schema| BetaJsonOutputFormat {
            schema,
            type_: BetaJsonOutputFormatType::JsonSchema,
        })
}

pub fn claude_thinking_effort_format_from_gemini_generation_config(
    generation_config: Option<&GeminiGenerationConfig>,
    model: Option<&crate::claude::count_tokens::types::Model>,
) -> (
    Option<BetaThinkingConfigParam>,
    Option<BetaOutputEffort>,
    Option<BetaJsonOutputFormat>,
) {
    if let Some(generation_config) = generation_config {
        let thinking = generation_config
            .thinking_config
            .as_ref()
            .and_then(|thinking_config| claude_thinking_from_gemini(thinking_config, model));

        let output_effort = generation_config
            .thinking_config
            .as_ref()
            .and_then(|thinking_config| thinking_config.thinking_level.as_ref())
            .and_then(claude_output_effort_from_gemini_level);

        let output_format = claude_output_format_from_gemini_generation_config(generation_config);

        (thinking, output_effort, output_format)
    } else {
        (None, None, None)
    }
}

pub fn claude_output_config_from_effort_and_format(
    output_effort: Option<BetaOutputEffort>,
    output_format: Option<BetaJsonOutputFormat>,
) -> Option<BetaOutputConfig> {
    if output_effort.is_some() || output_format.is_some() {
        Some(BetaOutputConfig {
            effort: output_effort,
            format: output_format,
            task_budget: None,
        })
    } else {
        None
    }
}
