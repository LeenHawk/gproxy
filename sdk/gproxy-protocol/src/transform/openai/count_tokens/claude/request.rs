use crate::claude::count_tokens::request::{
    ClaudeCountTokensRequest, PathParameters, QueryParameters, RequestBody, RequestHeaders,
};
use crate::claude::count_tokens::types as ct;
use crate::openai::count_tokens::request::OpenAiCountTokensRequest;
use crate::openai::count_tokens::types as ot;
use crate::transform::openai::count_tokens::claude::utils::{
    ClaudeToolUseIdMapper, mcp_allowed_tools_to_configs, openai_mcp_tool_to_server,
    openai_message_content_to_claude, openai_reasoning_to_claude, openai_role_to_claude,
    openai_tool_choice_to_claude, parallel_disable, push_message_block, tool_from_function,
};
use crate::transform::openai::count_tokens::utils::{
    openai_function_call_output_content_to_text, openai_input_to_items,
    openai_reasoning_summary_to_text,
};
use crate::transform::utils::TransformError;

impl TryFrom<OpenAiCountTokensRequest> for ClaudeCountTokensRequest {
    type Error = TransformError;

    fn try_from(value: OpenAiCountTokensRequest) -> Result<Self, TransformError> {
        let body = value.body;
        let mut messages = Vec::new();
        let mut tool_use_ids = ClaudeToolUseIdMapper::default();

        for item in openai_input_to_items(body.input) {
            match item {
                ot::ResponseInputItem::Message(message) => {
                    messages.push(ct::BetaMessageParam {
                        content: openai_message_content_to_claude(message.content),
                        role: openai_role_to_claude(message.role),
                    });
                }
                ot::ResponseInputItem::OutputMessage(message) => {
                    let text = message
                        .content
                        .into_iter()
                        .map(|part| match part {
                            ot::ResponseOutputContent::Text(text) => text.text,
                            ot::ResponseOutputContent::Refusal(refusal) => refusal.refusal,
                        })
                        .filter(|text| !text.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n");
                    if !text.is_empty() {
                        messages.push(ct::BetaMessageParam {
                            content: ct::BetaMessageContent::Text(text),
                            role: ct::BetaMessageRole::Assistant,
                        });
                    }
                }
                ot::ResponseInputItem::FunctionToolCall(tool_call) => {
                    let tool_use_id = tool_use_ids.tool_use_id(tool_call.call_id);
                    let input = serde_json::from_str::<ct::JsonObject>(&tool_call.arguments)
                        .unwrap_or_default();
                    push_message_block(
                        &mut messages,
                        ct::BetaMessageRole::Assistant,
                        ct::BetaContentBlockParam::ToolUse(ct::BetaToolUseBlockParam {
                            id: tool_use_id,
                            input,
                            name: tool_call.name,
                            type_: ct::BetaToolUseBlockType::ToolUse,
                            cache_control: None,
                            caller: None,
                        }),
                    );
                }
                ot::ResponseInputItem::FunctionCallOutput(tool_result) => {
                    let tool_use_id = tool_use_ids.tool_use_id(tool_result.call_id);
                    let output_text =
                        openai_function_call_output_content_to_text(&tool_result.output);
                    push_message_block(
                        &mut messages,
                        ct::BetaMessageRole::User,
                        ct::BetaContentBlockParam::ToolResult(ct::BetaToolResultBlockParam {
                            tool_use_id,
                            type_: ct::BetaToolResultBlockType::ToolResult,
                            cache_control: None,
                            content: if output_text.is_empty() {
                                None
                            } else {
                                Some(ct::BetaToolResultBlockParamContent::Text(output_text))
                            },
                            is_error: None,
                        }),
                    );
                }
                ot::ResponseInputItem::ReasoningItem(reasoning) => {
                    let thinking = openai_reasoning_summary_to_text(&reasoning.summary);
                    if !thinking.is_empty()
                        && let Some(signature) = reasoning.id.filter(|id| !id.is_empty())
                    {
                        messages.push(ct::BetaMessageParam {
                            content: ct::BetaMessageContent::Blocks(vec![
                                ct::BetaContentBlockParam::Thinking(ct::BetaThinkingBlockParam {
                                    signature,
                                    thinking,
                                    type_: ct::BetaThinkingBlockType::Thinking,
                                }),
                            ]),
                            role: ct::BetaMessageRole::Assistant,
                        });
                    }
                }
                other => {
                    let text = format!("{other:?}");
                    if !text.is_empty() {
                        messages.push(ct::BetaMessageParam {
                            content: ct::BetaMessageContent::Text(text),
                            role: ct::BetaMessageRole::User,
                        });
                    }
                }
            }
        }

        let disable_parallel_tool_use = parallel_disable(body.parallel_tool_calls);
        let tool_choice = openai_tool_choice_to_claude(body.tool_choice, disable_parallel_tool_use);
        let model = ct::Model::Custom(body.model.clone().unwrap_or_default());
        let thinking = openai_reasoning_to_claude(body.reasoning, None, Some(&model));

        let output_effort = body
            .text
            .as_ref()
            .and_then(|text| text.verbosity.as_ref())
            .map(|verbosity| match verbosity {
                ot::ResponseTextVerbosity::Low => ct::BetaOutputEffort::Low,
                ot::ResponseTextVerbosity::Medium => ct::BetaOutputEffort::Medium,
                ot::ResponseTextVerbosity::High => ct::BetaOutputEffort::High,
            });

        let output_format = body
            .text
            .as_ref()
            .and_then(|text| text.format.as_ref())
            .and_then(|format| match format {
                ot::ResponseTextFormatConfig::JsonSchema(schema) => {
                    Some(ct::BetaJsonOutputFormat {
                        schema: schema.schema.clone(),
                        type_: ct::BetaJsonOutputFormatType::JsonSchema,
                    })
                }
                _ => None,
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

        let context_management = match body.truncation {
            Some(ot::ResponseTruncation::Auto) => Some(ct::BetaContextManagementConfig {
                edits: Some(vec![ct::BetaContextManagementEdit::Compact(
                    ct::BetaCompact20260112Edit {
                        type_: ct::BetaCompactType::Compact20260112,
                        instructions: None,
                        pause_after_compaction: None,
                        trigger: None,
                    },
                )]),
            }),
            Some(ot::ResponseTruncation::Disabled) | None => None,
        };

        let mut converted_tools = Vec::new();
        let mut mcp_servers = Vec::new();
        if let Some(tools) = body.tools {
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

        let system = body.instructions.and_then(|text| {
            if text.is_empty() {
                None
            } else {
                Some(ct::BetaSystemPrompt::Text(text))
            }
        });

        Ok(ClaudeCountTokensRequest {
            method: ct::HttpMethod::Post,
            path: PathParameters::default(),
            query: QueryParameters::default(),
            headers: RequestHeaders::default(),
            body: RequestBody {
                messages,
                model,
                context_management,
                mcp_servers: if mcp_servers.is_empty() {
                    None
                } else {
                    Some(mcp_servers)
                },
                cache_control: None,
                output_config,
                speed: None,
                system,
                thinking,
                tool_choice,
                tools: if converted_tools.is_empty() {
                    None
                } else {
                    Some(converted_tools)
                },
            },
        })
    }
}
