use super::utils::{parse_tool_use_input, system_text_block, text_block};
use crate::claude::count_tokens::types as ct;
use crate::claude::create_message::request::{
    ClaudeCreateMessageRequest, PathParameters, QueryParameters, RequestBody, RequestHeaders,
};
use crate::claude::create_message::types::{
    BetaMetadata, BetaServiceTierParam, BetaSpeed, HttpMethod as ClaudeHttpMethod, Model,
};
use crate::openai::count_tokens::types as ot;
use crate::openai::create_chat_completions::request::OpenAiChatCompletionsRequest;
use crate::openai::create_chat_completions::types as oct;
use crate::transform::claude::utils::claude_model_supports_enabled_thinking;
use crate::transform::openai::count_tokens::claude::utils::{
    ClaudeToolUseIdMapper, mcp_allowed_tools_to_configs, openai_mcp_tool_to_server,
    openai_message_content_to_claude, openai_reasoning_to_claude, openai_role_to_claude,
    openai_tool_choice_to_claude, parallel_disable, push_message_block, tool_from_function,
};
use crate::transform::openai::count_tokens::utils::openai_message_content_to_text;
use crate::transform::openai::generate_content::openai_chat_completions::utils::{
    chat_reasoning_to_response_reasoning, chat_response_text_config, chat_stop_to_vec,
    chat_text_content_to_plain_text, chat_text_content_to_response_input_message_content,
    chat_tool_choice_to_response_tool_choice, chat_tools_to_response_tools,
    chat_user_content_to_response_input_message_content,
};
use crate::transform::utils::TransformError;

impl TryFrom<OpenAiChatCompletionsRequest> for ClaudeCreateMessageRequest {
    type Error = TransformError;

    fn try_from(value: OpenAiChatCompletionsRequest) -> Result<Self, TransformError> {
        let crate::openai::create_chat_completions::request::RequestBody {
            messages: chat_messages,
            model,
            function_call,
            functions,
            max_completion_tokens,
            max_tokens,
            metadata,
            parallel_tool_calls,
            reasoning_effort,
            response_format,
            service_tier,
            stop,
            stream,
            temperature,
            tool_choice,
            tools,
            top_p,
            user,
            verbosity,
            thinking: chat_thinking,
            web_search_options,
            ..
        } = value.body;

        let response_reasoning = chat_reasoning_to_response_reasoning(reasoning_effort);
        let claude_model = Model::Custom(model.clone());
        let response_text = chat_response_text_config(response_format, verbosity);
        let response_tool_choice =
            chat_tool_choice_to_response_tool_choice(tool_choice, function_call);
        let response_tools = chat_tools_to_response_tools(tools, functions, web_search_options);
        let reasoning_max_tokens = max_completion_tokens.or(max_tokens);
        let claude_max_tokens = reasoning_max_tokens.unwrap_or(8_192);

        let mut messages = Vec::new();
        let mut system_blocks = Vec::new();
        let mut seen_non_system = false;
        let mut tool_use_ids = ClaudeToolUseIdMapper::default();

        for (message_index, message) in chat_messages.into_iter().enumerate() {
            match message {
                oct::ChatCompletionMessageParam::Developer(message) => {
                    let content =
                        chat_text_content_to_response_input_message_content(message.content);
                    if !seen_non_system {
                        let text = openai_message_content_to_text(&content);
                        if !text.is_empty() {
                            system_blocks.push(system_text_block(text));
                        }
                    } else {
                        messages.push(ct::BetaMessageParam {
                            content: openai_message_content_to_claude(content),
                            role: openai_role_to_claude(ot::ResponseInputMessageRole::Developer),
                        });
                    }
                }
                oct::ChatCompletionMessageParam::System(message) => {
                    let content =
                        chat_text_content_to_response_input_message_content(message.content);
                    if !seen_non_system {
                        let text = openai_message_content_to_text(&content);
                        if !text.is_empty() {
                            system_blocks.push(system_text_block(text));
                        }
                    } else {
                        messages.push(ct::BetaMessageParam {
                            content: openai_message_content_to_claude(content),
                            role: openai_role_to_claude(ot::ResponseInputMessageRole::System),
                        });
                    }
                }
                oct::ChatCompletionMessageParam::User(message) => {
                    seen_non_system = true;
                    let content =
                        chat_user_content_to_response_input_message_content(message.content);
                    messages.push(ct::BetaMessageParam {
                        content: openai_message_content_to_claude(content),
                        role: openai_role_to_claude(ot::ResponseInputMessageRole::User),
                    });
                }
                oct::ChatCompletionMessageParam::Assistant(message) => {
                    seen_non_system = true;
                    let oct::ChatCompletionAssistantMessageParam {
                        content,
                        refusal,
                        function_call,
                        tool_calls,
                        ..
                    } = message;
                    let mut blocks = Vec::new();

                    if let Some(content) = content {
                        match content {
                            oct::ChatCompletionAssistantContent::Text(text) => {
                                if !text.is_empty() {
                                    blocks.push(text_block(text));
                                }
                            }
                            oct::ChatCompletionAssistantContent::Parts(parts) => {
                                for part in parts {
                                    match part {
                                        oct::ChatCompletionAssistantContentPart::Text(part) => {
                                            if !part.text.is_empty() {
                                                blocks.push(text_block(part.text));
                                            }
                                        }
                                        oct::ChatCompletionAssistantContentPart::Refusal(part) => {
                                            if !part.refusal.is_empty() {
                                                blocks.push(text_block(part.refusal));
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
                        blocks.push(text_block(refusal));
                    }

                    if let Some(function_call) = function_call {
                        let tool_use_id =
                            tool_use_ids.tool_use_id(format!("function_call_{message_index}"));
                        blocks.push(ct::BetaContentBlockParam::ToolUse(
                            ct::BetaToolUseBlockParam {
                                id: tool_use_id,
                                input: parse_tool_use_input(function_call.arguments),
                                name: function_call.name,
                                type_: ct::BetaToolUseBlockType::ToolUse,
                                cache_control: None,
                                caller: None,
                            },
                        ));
                    }

                    if let Some(tool_calls) = tool_calls {
                        for call in tool_calls {
                            match call {
                                oct::ChatCompletionMessageToolCall::Function(call) => {
                                    let tool_use_id = tool_use_ids.tool_use_id(call.id);
                                    blocks.push(ct::BetaContentBlockParam::ToolUse(
                                        ct::BetaToolUseBlockParam {
                                            id: tool_use_id,
                                            input: parse_tool_use_input(call.function.arguments),
                                            name: call.function.name,
                                            type_: ct::BetaToolUseBlockType::ToolUse,
                                            cache_control: None,
                                            caller: None,
                                        },
                                    ));
                                }
                                oct::ChatCompletionMessageToolCall::Custom(call) => {
                                    let tool_use_id = tool_use_ids.tool_use_id(call.id);
                                    blocks.push(ct::BetaContentBlockParam::ToolUse(
                                        ct::BetaToolUseBlockParam {
                                            id: tool_use_id,
                                            input: parse_tool_use_input(call.custom.input),
                                            name: call.custom.name,
                                            type_: ct::BetaToolUseBlockType::ToolUse,
                                            cache_control: None,
                                            caller: None,
                                        },
                                    ));
                                }
                            }
                        }
                    }

                    if !blocks.is_empty() {
                        messages.push(ct::BetaMessageParam {
                            content: ct::BetaMessageContent::Blocks(blocks),
                            role: ct::BetaMessageRole::Assistant,
                        });
                    }
                }
                oct::ChatCompletionMessageParam::Tool(message) => {
                    seen_non_system = true;
                    let text = chat_text_content_to_plain_text(&message.content);
                    let tool_use_id = tool_use_ids.tool_use_id(message.tool_call_id);
                    push_message_block(
                        &mut messages,
                        ct::BetaMessageRole::User,
                        ct::BetaContentBlockParam::ToolResult(ct::BetaToolResultBlockParam {
                            tool_use_id,
                            type_: ct::BetaToolResultBlockType::ToolResult,
                            cache_control: None,
                            content: if text.is_empty() {
                                None
                            } else {
                                Some(ct::BetaToolResultBlockParamContent::Text(text))
                            },
                            is_error: None,
                        }),
                    );
                }
                oct::ChatCompletionMessageParam::Function(message) => {
                    seen_non_system = true;
                    let text = if message.content.is_empty() {
                        format!("function:{}", message.name)
                    } else {
                        format!("function:{}\n{}", message.name, message.content)
                    };
                    messages.push(ct::BetaMessageParam {
                        content: ct::BetaMessageContent::Text(text),
                        role: ct::BetaMessageRole::User,
                    });
                }
            }
        }

        let disable_parallel_tool_use = parallel_disable(parallel_tool_calls);
        let tool_choice =
            openai_tool_choice_to_claude(response_tool_choice, disable_parallel_tool_use);
        let extra_thinking = chat_thinking.map(|thinking| match thinking {
            oct::ChatCompletionClaudeThinkingConfig::Enabled(config) => {
                if claude_model_supports_enabled_thinking(Some(&claude_model)) {
                    ct::BetaThinkingConfigParam::Enabled(ct::BetaThinkingConfigEnabled {
                        budget_tokens: config.budget_tokens,
                        type_: ct::BetaThinkingConfigEnabledType::Enabled,
                        display: None,
                    })
                } else {
                    ct::BetaThinkingConfigParam::Adaptive(ct::BetaThinkingConfigAdaptive {
                        type_: ct::BetaThinkingConfigAdaptiveType::Adaptive,
                        display: None,
                    })
                }
            }
            oct::ChatCompletionClaudeThinkingConfig::Disabled(_) => {
                ct::BetaThinkingConfigParam::Disabled(ct::BetaThinkingConfigDisabled {
                    type_: ct::BetaThinkingConfigDisabledType::Disabled,
                })
            }
            oct::ChatCompletionClaudeThinkingConfig::Adaptive(_) => {
                ct::BetaThinkingConfigParam::Adaptive(ct::BetaThinkingConfigAdaptive {
                    type_: ct::BetaThinkingConfigAdaptiveType::Adaptive,
                    display: None,
                })
            }
        });
        let thinking = extra_thinking
            .or_else(|| {
                openai_reasoning_to_claude(
                    response_reasoning,
                    reasoning_max_tokens,
                    Some(&claude_model),
                )
            })
            .or_else(|| Some(default_chat_thinking()));

        let output_effort = response_text
            .as_ref()
            .and_then(|text| text.verbosity.as_ref())
            .map(|verbosity| match verbosity {
                ot::ResponseTextVerbosity::Low => ct::BetaOutputEffort::Low,
                ot::ResponseTextVerbosity::Medium => ct::BetaOutputEffort::Medium,
                ot::ResponseTextVerbosity::High => ct::BetaOutputEffort::High,
            });

        let output_format = response_text
            .as_ref()
            .and_then(|text| text.format.as_ref())
            .and_then(|format| match format {
                ot::ResponseTextFormatConfig::JsonSchema(schema) => {
                    Some(ct::BetaJsonOutputFormat {
                        schema: schema.schema.clone(),
                        type_: ct::BetaJsonOutputFormatType::JsonSchema,
                    })
                }
                ot::ResponseTextFormatConfig::JsonObject(_) => None,
                ot::ResponseTextFormatConfig::Text(_) => None,
            });

        let output_config = if output_effort.is_some() || output_format.is_some() {
            Some(ct::BetaOutputConfig {
                effort: output_effort,
                format: output_format.clone(),
                task_budget: None,
            })
        } else {
            None
        };

        let mut converted_tools = Vec::new();
        let mut mcp_servers = Vec::new();
        if let Some(tools) = response_tools {
            for tool in tools {
                match tool {
                    ot::ResponseTool::Function(tool) => {
                        converted_tools.push(tool_from_function(tool))
                    }
                    ot::ResponseTool::Custom(tool) => {
                        converted_tools.push(ct::BetaToolUnion::Custom(ct::BetaTool {
                            input_schema: ct::BetaToolInputSchema {
                                type_: ct::BetaToolInputSchemaType::Object,
                                properties: None,
                                required: None,
                                extra_fields: Default::default(),
                            },
                            name: tool.name,
                            common: ct::BetaToolCommonFields::default(),
                            description: tool.description,
                            eager_input_streaming: None,
                            type_: Some(ct::BetaCustomToolType::Custom),
                        }));
                    }
                    ot::ResponseTool::CodeInterpreter(_)
                    | ot::ResponseTool::LocalShell(_)
                    | ot::ResponseTool::Shell(_)
                    | ot::ResponseTool::ApplyPatch(_) => {
                        converted_tools.push(ct::BetaToolUnion::CodeExecution20250825(
                            ct::BetaCodeExecutionTool20250825 {
                                name: ct::BetaCodeExecutionToolName::CodeExecution,
                                type_: ct::BetaCodeExecutionTool20250825Type::CodeExecution20250825,
                                common: ct::BetaToolCommonFields::default(),
                            },
                        ));
                    }
                    ot::ResponseTool::Computer(tool) => {
                        converted_tools.push(ct::BetaToolUnion::ComputerUse20251124(
                            ct::BetaToolComputerUse20251124 {
                                display_height_px: tool.display_height_or_default(),
                                display_width_px: tool.display_width_or_default(),
                                name: ct::BetaComputerToolName::Computer,
                                type_: ct::BetaToolComputerUse20251124Type::Computer20251124,
                                common: ct::BetaToolCommonFields::default(),
                                display_number: None,
                                enable_zoom: None,
                            },
                        ));
                    }
                    ot::ResponseTool::WebSearch(tool) => {
                        converted_tools.push(ct::BetaToolUnion::WebSearch20250305(
                            ct::BetaWebSearchTool20250305 {
                                name: ct::BetaWebSearchToolName::WebSearch,
                                type_: ct::BetaWebSearchTool20250305Type::WebSearch20250305,
                                common: ct::BetaToolCommonFields::default(),
                                allowed_domains: tool.filters.and_then(|f| f.allowed_domains),
                                blocked_domains: None,
                                max_uses: None,
                                user_location: tool.user_location.map(|location| {
                                    ct::BetaWebSearchUserLocation {
                                        type_: ct::BetaWebSearchUserLocationType::Approximate,
                                        city: location.city,
                                        country: location.country,
                                        region: location.region,
                                        timezone: location.timezone,
                                    }
                                }),
                            },
                        ));
                    }
                    ot::ResponseTool::WebSearchPreview(tool) => {
                        converted_tools.push(ct::BetaToolUnion::WebSearch20250305(
                            ct::BetaWebSearchTool20250305 {
                                name: ct::BetaWebSearchToolName::WebSearch,
                                type_: ct::BetaWebSearchTool20250305Type::WebSearch20250305,
                                common: ct::BetaToolCommonFields::default(),
                                allowed_domains: None,
                                blocked_domains: None,
                                max_uses: None,
                                user_location: tool.user_location.map(|location| {
                                    ct::BetaWebSearchUserLocation {
                                        type_: ct::BetaWebSearchUserLocationType::Approximate,
                                        city: location.city,
                                        country: location.country,
                                        region: location.region,
                                        timezone: location.timezone,
                                    }
                                }),
                            },
                        ));
                    }
                    ot::ResponseTool::FileSearch(_) => {
                        converted_tools.push(ct::BetaToolUnion::ToolSearchBm25_20251119(
                            ct::BetaToolSearchToolBm25_20251119 {
                                name: ct::BetaToolSearchToolBm25Name::ToolSearchToolBm25,
                                type_: ct::BetaToolSearchToolBm25Type::ToolSearchToolBm2520251119,
                                common: ct::BetaToolCommonFields::default(),
                            },
                        ));
                    }
                    ot::ResponseTool::Mcp(tool) => {
                        if let Some(server) = openai_mcp_tool_to_server(&tool) {
                            mcp_servers.push(server);
                        }
                        converted_tools.push(ct::BetaToolUnion::McpToolset(ct::BetaMcpToolset {
                            mcp_server_name: tool.server_label,
                            type_: ct::BetaMcpToolsetType::McpToolset,
                            cache_control: None,
                            configs: mcp_allowed_tools_to_configs(tool.allowed_tools.as_ref()),
                            default_config: None,
                        }));
                    }
                    ot::ResponseTool::Namespace(_) | ot::ResponseTool::ToolSearch(_) => {}
                    ot::ResponseTool::ImageGeneration(_) => {}
                }
            }
        }

        let claude_service_tier = match service_tier.clone() {
            Some(oct::ChatCompletionServiceTier::Auto) => Some(BetaServiceTierParam::Auto),
            Some(
                oct::ChatCompletionServiceTier::Default
                | oct::ChatCompletionServiceTier::Flex
                | oct::ChatCompletionServiceTier::Scale
                | oct::ChatCompletionServiceTier::Priority,
            ) => Some(BetaServiceTierParam::StandardOnly),
            None => None,
        };

        let speed = match service_tier {
            Some(oct::ChatCompletionServiceTier::Priority) => Some(BetaSpeed::Fast),
            _ => None,
        };

        let metadata_user_id = user.or_else(|| {
            metadata
                .as_ref()
                .and_then(|map| map.get("user_id").cloned())
        });
        let metadata = metadata_user_id.map(|user_id| BetaMetadata {
            user_id: Some(user_id),
        });

        let system = if system_blocks.is_empty() {
            None
        } else if system_blocks.len() == 1 {
            Some(ct::BetaSystemPrompt::Text(system_blocks[0].text.clone()))
        } else {
            Some(ct::BetaSystemPrompt::Blocks(system_blocks))
        };

        Ok(ClaudeCreateMessageRequest {
            method: ClaudeHttpMethod::Post,
            path: PathParameters::default(),
            query: QueryParameters::default(),
            headers: RequestHeaders::default(),
            body: RequestBody {
                max_tokens: claude_max_tokens,
                messages,
                model: claude_model,
                container: None,
                context_management: None,
                inference_geo: None,
                mcp_servers: if mcp_servers.is_empty() {
                    None
                } else {
                    Some(mcp_servers)
                },
                metadata,
                cache_control: None,
                output_config,
                service_tier: claude_service_tier,
                speed,
                stop_sequences: chat_stop_to_vec(stop),
                stream,
                system,
                temperature,
                thinking,
                tool_choice,
                tools: if converted_tools.is_empty() {
                    None
                } else {
                    Some(converted_tools)
                },
                top_k: None,
                top_p,
            },
        })
    }
}

fn default_chat_thinking() -> ct::BetaThinkingConfigParam {
    ct::BetaThinkingConfigParam::Disabled(ct::BetaThinkingConfigDisabled {
        type_: ct::BetaThinkingConfigDisabledType::Disabled,
    })
}
