use gproxy_protocol::claude::count_tokens::request::{
    CountTokensHeaders as ClaudeCountTokensHeaders, CountTokensRequest as ClaudeCountTokensRequest,
    CountTokensRequestBody as ClaudeCountTokensRequestBody,
};
use gproxy_protocol::claude::count_tokens::types::{
    BetaContentBlockParam as ClaudeContentBlockParam,
    BetaDocumentBlockType as ClaudeDocumentBlockType, BetaDocumentSource as ClaudeDocumentSource,
    BetaImageBlockParam as ClaudeImageBlockParam, BetaImageBlockType as ClaudeImageBlockType,
    BetaImageSource as ClaudeImageSource, BetaJSONOutputFormat as ClaudeJSONOutputFormat,
    BetaJSONOutputFormatType as ClaudeJSONOutputFormatType, BetaMCPToolset as ClaudeMCPToolset,
    BetaMessageContent as ClaudeMessageContent, BetaMessageParam as ClaudeMessageParam,
    BetaMessageRole as ClaudeMessageRole, BetaOutputConfig as ClaudeOutputConfig,
    BetaOutputEffort as ClaudeOutputEffort, BetaRequestDocumentBlock as ClaudeDocumentBlock,
    BetaRequestMCPServerToolConfiguration as ClaudeMCPServerToolConfiguration,
    BetaRequestMCPServerURLDefinition as ClaudeMCPServerURLDefinition,
    BetaRequestMCPServerURLDefinitionType as ClaudeMCPServerURLDefinitionType,
    BetaSystemParam as ClaudeSystemParam, BetaTextBlockParam as ClaudeTextBlockParam,
    BetaTextBlockType as ClaudeTextBlockType, BetaThinkingConfigParam as ClaudeThinkingConfigParam,
    BetaTool as ClaudeTool, BetaToolBash as ClaudeToolBash, BetaToolBuiltin as ClaudeToolBuiltin,
    BetaToolChoice as ClaudeToolChoice, BetaToolCodeExecution as ClaudeToolCodeExecution,
    BetaToolComputerUse as ClaudeToolComputerUse, BetaToolCustom as ClaudeToolCustom,
    BetaToolCustomType as ClaudeToolCustomType, BetaToolInputSchema as ClaudeToolInputSchema,
    BetaToolInputSchemaType as ClaudeToolInputSchemaType,
    BetaToolSearchTool as ClaudeToolSearchTool, BetaToolTextEditor as ClaudeToolTextEditor,
    BetaUserLocation as ClaudeUserLocation, BetaUserLocationType as ClaudeUserLocationType,
    BetaWebSearchTool as ClaudeWebSearchTool, Model as ClaudeModel,
};
use gproxy_protocol::openai::count_tokens::request::InputTokenCountRequest as OpenAIInputTokenCountRequest;
use gproxy_protocol::openai::create_response::types::{
    AllowedTool, ApproximateLocation, EasyInputMessage, EasyInputMessageContent,
    EasyInputMessageRole, FunctionTool, InputContent, InputItem, InputMessage, InputMessageRole,
    InputParam, MCPAllowedTools, MCPTool, OutputMessage, OutputMessageContent, Reasoning,
    ReasoningEffort, ResponseTextParam, TextResponseFormatConfiguration, Tool, ToolChoiceAllowed,
    ToolChoiceAllowedMode, ToolChoiceBuiltInType, ToolChoiceOptions, ToolChoiceParam,
    ToolChoiceTypes, WebSearchApproximateLocation,
};
use serde_json::Value as JsonValue;

/// Convert an OpenAI input-tokens request into a Claude count-tokens request.
pub fn transform_request(request: OpenAIInputTokenCountRequest) -> ClaudeCountTokensRequest {
    let mut messages = Vec::new();
    let mut system_texts = Vec::new();

    if let Some(instructions) = request.body.instructions.clone() {
        push_system_text(&mut system_texts, instructions);
    }

    if let Some(input) = request.body.input {
        append_input_param(input, &mut messages, &mut system_texts);
    }

    let system = if system_texts.is_empty() {
        None
    } else {
        Some(ClaudeSystemParam::Text(system_texts.join("\n")))
    };

    let tools_input = request.body.tools;

    let mcp_servers = tools_input
        .as_ref()
        .map(|tools| extract_mcp_servers(tools.as_slice()))
        .and_then(|servers| {
            if servers.is_empty() {
                None
            } else {
                Some(servers)
            }
        });

    let tools = tools_input
        .map(map_tools)
        .and_then(|tools| if tools.is_empty() { None } else { Some(tools) });

    let tool_choice = request
        .body
        .tool_choice
        .map(|choice| map_tool_choice(choice, request.body.parallel_tool_calls));

    let (thinking, output_config) = map_reasoning(request.body.reasoning);
    let output_format = map_output_format(request.body.text);

    ClaudeCountTokensRequest {
        headers: ClaudeCountTokensHeaders::default(),
        body: ClaudeCountTokensRequestBody {
            messages,
            model: ClaudeModel::Custom(request.body.model),
            system,
            tools,
            tool_choice,
            thinking,
            output_config,
            output_format,
            context_management: None,
            mcp_servers,
        },
    }
}

fn append_input_param(
    input: InputParam,
    messages: &mut Vec<ClaudeMessageParam>,
    system_texts: &mut Vec<String>,
) {
    match input {
        InputParam::Text(text) => {
            messages.push(ClaudeMessageParam {
                role: ClaudeMessageRole::User,
                content: ClaudeMessageContent::Text(text),
            });
        }
        InputParam::Items(items) => {
            for item in items {
                append_input_item(item, messages, system_texts);
            }
        }
    }
}

fn append_input_item(
    item: InputItem,
    messages: &mut Vec<ClaudeMessageParam>,
    system_texts: &mut Vec<String>,
) {
    match item {
        InputItem::EasyMessage(message) => {
            append_easy_message(message, messages, system_texts);
        }
        InputItem::Item(item) => match item {
            gproxy_protocol::openai::create_response::types::Item::InputMessage(message) => {
                append_input_message(message, messages, system_texts);
            }
            gproxy_protocol::openai::create_response::types::Item::OutputMessage(message) => {
                append_output_message(message, messages);
            }
            _ => {}
        },
        InputItem::Reference(_) => {}
    }
}

fn append_easy_message(
    message: EasyInputMessage,
    messages: &mut Vec<ClaudeMessageParam>,
    system_texts: &mut Vec<String>,
) {
    match message.role {
        EasyInputMessageRole::User => {
            if let Some(content) = easy_message_content_to_message_content(message.content) {
                messages.push(ClaudeMessageParam {
                    role: ClaudeMessageRole::User,
                    content,
                });
            }
        }
        EasyInputMessageRole::Assistant => {
            if let Some(content) = easy_message_content_to_message_content(message.content) {
                messages.push(ClaudeMessageParam {
                    role: ClaudeMessageRole::Assistant,
                    content,
                });
            }
        }
        EasyInputMessageRole::System | EasyInputMessageRole::Developer => {
            if let Some(text) = easy_message_content_to_text(message.content) {
                push_system_text(system_texts, text);
            }
        }
    }
}

fn append_input_message(
    message: InputMessage,
    messages: &mut Vec<ClaudeMessageParam>,
    system_texts: &mut Vec<String>,
) {
    match message.role {
        InputMessageRole::User => {
            if let Some(content) = input_contents_to_message_content(&message.content) {
                messages.push(ClaudeMessageParam {
                    role: ClaudeMessageRole::User,
                    content,
                });
            }
        }
        InputMessageRole::System | InputMessageRole::Developer => {
            if let Some(text) = input_contents_to_text(&message.content) {
                push_system_text(system_texts, text);
            }
        }
    }
}

fn append_output_message(message: OutputMessage, messages: &mut Vec<ClaudeMessageParam>) {
    if let Some(content) = output_contents_to_message_content(&message.content) {
        messages.push(ClaudeMessageParam {
            role: ClaudeMessageRole::Assistant,
            content,
        });
    }
}

fn easy_message_content_to_message_content(
    content: EasyInputMessageContent,
) -> Option<ClaudeMessageContent> {
    match content {
        EasyInputMessageContent::Text(text) => Some(ClaudeMessageContent::Text(text)),
        EasyInputMessageContent::Parts(parts) => input_contents_to_message_content(&parts),
    }
}

fn easy_message_content_to_text(content: EasyInputMessageContent) -> Option<String> {
    match content {
        EasyInputMessageContent::Text(text) => Some(text),
        EasyInputMessageContent::Parts(parts) => input_contents_to_text(&parts),
    }
}

fn input_contents_to_message_content(contents: &[InputContent]) -> Option<ClaudeMessageContent> {
    let mut blocks = Vec::new();

    for content in contents {
        if let Some(block) = input_content_to_block(content) {
            blocks.push(block);
        }
    }

    if blocks.is_empty() {
        return None;
    }

    if blocks.len() == 1
        && let ClaudeContentBlockParam::Text(text_block) = &blocks[0]
    {
        return Some(ClaudeMessageContent::Text(text_block.text.clone()));
    }

    Some(ClaudeMessageContent::Blocks(blocks))
}

fn output_contents_to_message_content(
    contents: &[OutputMessageContent],
) -> Option<ClaudeMessageContent> {
    let mut blocks = Vec::new();

    for content in contents {
        let text = match content {
            OutputMessageContent::OutputText(value) => value.text.clone(),
            OutputMessageContent::Refusal(value) => value.refusal.clone(),
        };

        if !text.is_empty() {
            blocks.push(text_block(text));
        }
    }

    if blocks.is_empty() {
        return None;
    }

    if blocks.len() == 1
        && let ClaudeContentBlockParam::Text(text_block) = &blocks[0]
    {
        return Some(ClaudeMessageContent::Text(text_block.text.clone()));
    }

    Some(ClaudeMessageContent::Blocks(blocks))
}

fn input_contents_to_text(contents: &[InputContent]) -> Option<String> {
    let mut texts = Vec::new();

    for content in contents {
        if let InputContent::InputText(text) = content
            && !text.text.is_empty()
        {
            texts.push(text.text.clone());
        }
    }

    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}

fn input_content_to_block(content: &InputContent) -> Option<ClaudeContentBlockParam> {
    match content {
        InputContent::InputText(value) => Some(text_block(value.text.clone())),
        InputContent::InputImage(value) => {
            if let Some(url) = &value.image_url {
                Some(ClaudeContentBlockParam::Image(ClaudeImageBlockParam {
                    source: ClaudeImageSource::Url { url: url.clone() },
                    r#type: ClaudeImageBlockType::Image,
                    cache_control: None,
                }))
            } else {
                value.file_id.as_ref().map(|file_id| {
                    ClaudeContentBlockParam::Image(ClaudeImageBlockParam {
                        source: ClaudeImageSource::File {
                            file_id: file_id.clone(),
                        },
                        r#type: ClaudeImageBlockType::Image,
                        cache_control: None,
                    })
                })
            }
        }
        InputContent::InputFile(value) => {
            if let Some(file_id) = &value.file_id {
                Some(ClaudeContentBlockParam::Document(ClaudeDocumentBlock {
                    source: ClaudeDocumentSource::File {
                        file_id: file_id.clone(),
                    },
                    r#type: ClaudeDocumentBlockType::Document,
                    cache_control: None,
                    citations: None,
                    context: None,
                    title: value.filename.clone(),
                }))
            } else {
                value.file_url.as_ref().map(|file_url| {
                    ClaudeContentBlockParam::Document(ClaudeDocumentBlock {
                        source: ClaudeDocumentSource::Url {
                            url: file_url.clone(),
                        },
                        r#type: ClaudeDocumentBlockType::Document,
                        cache_control: None,
                        citations: None,
                        context: None,
                        title: value.filename.clone(),
                    })
                })
            }
        }
    }
}

fn text_block(text: String) -> ClaudeContentBlockParam {
    ClaudeContentBlockParam::Text(ClaudeTextBlockParam {
        text,
        r#type: ClaudeTextBlockType::Text,
        cache_control: None,
        citations: None,
    })
}

fn push_system_text(system_texts: &mut Vec<String>, text: String) {
    if !text.is_empty() {
        system_texts.push(text);
    }
}

fn extract_mcp_servers(tools: &[Tool]) -> Vec<ClaudeMCPServerURLDefinition> {
    tools
        .iter()
        .filter_map(|tool| match tool {
            Tool::MCP(mcp) => map_mcp_tool(mcp),
            _ => None,
        })
        .collect()
}

fn map_mcp_tool(tool: &MCPTool) -> Option<ClaudeMCPServerURLDefinition> {
    let url = tool.server_url.clone()?;

    let allowed_tools = match &tool.allowed_tools {
        Some(MCPAllowedTools::Names(names)) => Some(names.clone()),
        Some(MCPAllowedTools::Filter(filter)) => filter.tool_names.clone(),
        None => None,
    };

    let tool_configuration = if allowed_tools.is_some() {
        Some(ClaudeMCPServerToolConfiguration {
            allowed_tools,
            enabled: None,
        })
    } else {
        None
    };

    Some(ClaudeMCPServerURLDefinition {
        name: tool.server_label.clone(),
        r#type: ClaudeMCPServerURLDefinitionType::Url,
        url,
        authorization_token: tool.authorization.clone(),
        tool_configuration,
    })
}

fn map_tools(tools: Vec<Tool>) -> Vec<ClaudeTool> {
    tools
        .into_iter()
        .map(|tool| match tool {
            Tool::Function(function) => ClaudeTool::Custom(map_function_tool(function)),
            Tool::Custom(custom) => ClaudeTool::Custom(ClaudeToolCustom {
                input_schema: ClaudeToolInputSchema {
                    r#type: ClaudeToolInputSchemaType::Object,
                    properties: None,
                    required: None,
                },
                name: custom.name,
                allowed_callers: None,
                cache_control: None,
                defer_loading: None,
                description: custom.description,
                input_examples: None,
                strict: None,
                r#type: Some(ClaudeToolCustomType::Custom),
            }),
            Tool::CodeInterpreter(_) => ClaudeTool::Builtin(
                ClaudeToolBuiltin::CodeExecution20250522(ClaudeToolCodeExecution {
                    name: "code_execution".to_string(),
                    allowed_callers: None,
                    cache_control: None,
                    defer_loading: None,
                    strict: None,
                }),
            ),
            Tool::ComputerUsePreview(tool) => ClaudeTool::Builtin(
                ClaudeToolBuiltin::ComputerUse20241022(ClaudeToolComputerUse {
                    display_height_px: clamp_i64_to_u32(tool.display_height),
                    display_width_px: clamp_i64_to_u32(tool.display_width),
                    name: "computer".to_string(),
                    allowed_callers: None,
                    cache_control: None,
                    defer_loading: None,
                    display_number: None,
                    enable_zoom: None,
                    input_examples: None,
                    strict: None,
                }),
            ),
            Tool::LocalShell(_) | Tool::Shell(_) => {
                ClaudeTool::Builtin(ClaudeToolBuiltin::Bash20241022(ClaudeToolBash {
                    name: "bash".to_string(),
                    allowed_callers: None,
                    cache_control: None,
                    defer_loading: None,
                    input_examples: None,
                    strict: None,
                }))
            }
            Tool::ApplyPatch(_) => ClaudeTool::Builtin(ClaudeToolBuiltin::TextEditor20241022(
                ClaudeToolTextEditor {
                    name: "text_editor".to_string(),
                    allowed_callers: None,
                    cache_control: None,
                    defer_loading: None,
                    input_examples: None,
                    max_characters: None,
                    strict: None,
                },
            )),
            Tool::WebSearch(tool) | Tool::WebSearch20250826(tool) => {
                let allowed_domains = tool.filters.and_then(|filters| filters.allowed_domains);
                let user_location = tool.user_location.map(map_web_search_location);

                ClaudeTool::Builtin(ClaudeToolBuiltin::WebSearch20250305(ClaudeWebSearchTool {
                    name: "web_search".to_string(),
                    allowed_callers: None,
                    allowed_domains,
                    blocked_domains: None,
                    cache_control: None,
                    defer_loading: None,
                    max_uses: None,
                    strict: None,
                    user_location,
                }))
            }
            Tool::WebSearchPreview(tool) | Tool::WebSearchPreview20250311(tool) => {
                let user_location = tool.user_location.map(map_preview_location);

                ClaudeTool::Builtin(ClaudeToolBuiltin::WebSearch20250305(ClaudeWebSearchTool {
                    name: "web_search".to_string(),
                    allowed_callers: None,
                    allowed_domains: None,
                    blocked_domains: None,
                    cache_control: None,
                    defer_loading: None,
                    max_uses: None,
                    strict: None,
                    user_location,
                }))
            }
            Tool::FileSearch(_) => ClaudeTool::Builtin(ClaudeToolBuiltin::ToolSearchToolBm25(
                ClaudeToolSearchTool {
                    name: "file_search".to_string(),
                    allowed_callers: None,
                    cache_control: None,
                    defer_loading: None,
                    strict: None,
                },
            )),
            Tool::ImageGeneration(_) => ClaudeTool::Custom(ClaudeToolCustom {
                input_schema: ClaudeToolInputSchema {
                    r#type: ClaudeToolInputSchemaType::Object,
                    properties: None,
                    required: None,
                },
                name: "image_generation".to_string(),
                allowed_callers: None,
                cache_control: None,
                defer_loading: None,
                description: None,
                input_examples: None,
                strict: None,
                r#type: Some(ClaudeToolCustomType::Custom),
            }),
            Tool::MCP(tool) => {
                ClaudeTool::Builtin(ClaudeToolBuiltin::McpToolset(ClaudeMCPToolset {
                    mcp_server_name: tool.server_label,
                    cache_control: None,
                    configs: None,
                    default_config: None,
                }))
            }
        })
        .collect()
}

fn map_function_tool(function: FunctionTool) -> ClaudeToolCustom {
    let schema = function
        .parameters
        .as_ref()
        .and_then(parse_input_schema)
        .unwrap_or(ClaudeToolInputSchema {
            r#type: ClaudeToolInputSchemaType::Object,
            properties: None,
            required: None,
        });

    ClaudeToolCustom {
        input_schema: schema,
        name: function.name,
        allowed_callers: None,
        cache_control: None,
        defer_loading: None,
        description: function.description,
        input_examples: None,
        strict: function.strict,
        r#type: Some(ClaudeToolCustomType::Custom),
    }
}

fn parse_input_schema(schema: &JsonValue) -> Option<ClaudeToolInputSchema> {
    let object = schema.as_object()?;
    let properties = object
        .get("properties")
        .and_then(|value| value.as_object())
        .map(|map| map.clone().into_iter().collect());

    let required = object
        .get("required")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(|value| value.to_string()))
                .collect::<Vec<String>>()
        });

    Some(ClaudeToolInputSchema {
        r#type: ClaudeToolInputSchemaType::Object,
        properties,
        required,
    })
}

fn map_tool_choice(choice: ToolChoiceParam, parallel_tool_calls: Option<bool>) -> ClaudeToolChoice {
    let disable_parallel = parallel_tool_calls.map(|value| !value);

    match choice {
        ToolChoiceParam::Mode(mode) => match mode {
            ToolChoiceOptions::None => ClaudeToolChoice::None,
            ToolChoiceOptions::Auto => ClaudeToolChoice::Auto {
                disable_parallel_tool_use: disable_parallel,
            },
            ToolChoiceOptions::Required => ClaudeToolChoice::Any {
                disable_parallel_tool_use: disable_parallel,
            },
        },
        ToolChoiceParam::Allowed(allowed) => map_allowed_tool_choice(allowed, disable_parallel),
        ToolChoiceParam::BuiltIn(types) => map_builtin_tool_choice(types, disable_parallel),
        ToolChoiceParam::Function(tool) => ClaudeToolChoice::Tool {
            name: tool.name,
            disable_parallel_tool_use: disable_parallel,
        },
        ToolChoiceParam::Custom(tool) => ClaudeToolChoice::Tool {
            name: tool.name,
            disable_parallel_tool_use: disable_parallel,
        },
        ToolChoiceParam::MCP(tool) => ClaudeToolChoice::Tool {
            name: tool.name.unwrap_or(tool.server_label),
            disable_parallel_tool_use: disable_parallel,
        },
        ToolChoiceParam::ApplyPatch(_) => ClaudeToolChoice::Tool {
            name: "text_editor".to_string(),
            disable_parallel_tool_use: disable_parallel,
        },
        ToolChoiceParam::Shell(_) => ClaudeToolChoice::Tool {
            name: "bash".to_string(),
            disable_parallel_tool_use: disable_parallel,
        },
    }
}

fn map_allowed_tool_choice(
    allowed: ToolChoiceAllowed,
    disable_parallel: Option<bool>,
) -> ClaudeToolChoice {
    if allowed.tools.len() == 1
        && let Some(name) = allowed_tool_name(&allowed.tools[0])
    {
        return ClaudeToolChoice::Tool {
            name,
            disable_parallel_tool_use: disable_parallel,
        };
    }

    match allowed.mode {
        ToolChoiceAllowedMode::Required => ClaudeToolChoice::Any {
            disable_parallel_tool_use: disable_parallel,
        },
        ToolChoiceAllowedMode::Auto => ClaudeToolChoice::Auto {
            disable_parallel_tool_use: disable_parallel,
        },
    }
}

fn map_builtin_tool_choice(
    types: ToolChoiceTypes,
    disable_parallel: Option<bool>,
) -> ClaudeToolChoice {
    let name = match types.r#type {
        ToolChoiceBuiltInType::FileSearch => "file_search",
        ToolChoiceBuiltInType::WebSearchPreview
        | ToolChoiceBuiltInType::WebSearchPreview20250311 => "web_search",
        ToolChoiceBuiltInType::ComputerUsePreview => "computer",
        ToolChoiceBuiltInType::ImageGeneration => "image_generation",
        ToolChoiceBuiltInType::CodeInterpreter => "code_execution",
    };

    ClaudeToolChoice::Tool {
        name: name.to_string(),
        disable_parallel_tool_use: disable_parallel,
    }
}

fn allowed_tool_name(tool: &AllowedTool) -> Option<String> {
    match tool {
        AllowedTool::Function { name } => Some(name.clone()),
        AllowedTool::Custom { name } => Some(name.clone()),
        AllowedTool::MCP { server_label, name } => {
            Some(name.clone().unwrap_or_else(|| server_label.clone()))
        }
        AllowedTool::FileSearch => Some("file_search".to_string()),
        AllowedTool::WebSearch | AllowedTool::WebSearch20250826 => Some("web_search".to_string()),
        AllowedTool::WebSearchPreview | AllowedTool::WebSearchPreview20250311 => {
            Some("web_search".to_string())
        }
        AllowedTool::ComputerUsePreview => Some("computer".to_string()),
        AllowedTool::CodeInterpreter => Some("code_execution".to_string()),
        AllowedTool::ImageGeneration => Some("image_generation".to_string()),
        AllowedTool::LocalShell | AllowedTool::Shell => Some("bash".to_string()),
        AllowedTool::ApplyPatch => Some("text_editor".to_string()),
    }
}

fn map_reasoning(
    reasoning: Option<Reasoning>,
) -> (
    Option<ClaudeThinkingConfigParam>,
    Option<ClaudeOutputConfig>,
) {
    let reasoning = match reasoning {
        Some(reasoning) => reasoning,
        None => return (None, None),
    };

    let output_effort = reasoning.effort.and_then(map_output_effort);
    let output_config = output_effort.map(|effort| ClaudeOutputConfig {
        effort: Some(effort),
    });

    let thinking = match reasoning.effort {
        Some(ReasoningEffort::None) => Some(ClaudeThinkingConfigParam::Disabled),
        Some(_) => Some(ClaudeThinkingConfigParam::Enabled {
            budget_tokens: 1024,
        }),
        None => None,
    };

    (thinking, output_config)
}

fn map_output_format(text: Option<ResponseTextParam>) -> Option<ClaudeJSONOutputFormat> {
    let format = text.and_then(|text| text.format)?;

    match format {
        TextResponseFormatConfiguration::Text => None,
        TextResponseFormatConfiguration::JsonObject => Some(ClaudeJSONOutputFormat {
            schema: json_object_schema(),
            r#type: ClaudeJSONOutputFormatType::JsonSchema,
        }),
        TextResponseFormatConfiguration::JsonSchema { schema, .. } => {
            Some(ClaudeJSONOutputFormat {
                schema,
                r#type: ClaudeJSONOutputFormatType::JsonSchema,
            })
        }
    }
}

fn json_object_schema() -> JsonValue {
    let mut map = serde_json::Map::new();
    map.insert("type".to_string(), JsonValue::String("object".to_string()));
    JsonValue::Object(map)
}

fn map_output_effort(effort: ReasoningEffort) -> Option<ClaudeOutputEffort> {
    match effort {
        ReasoningEffort::None => None,
        ReasoningEffort::Minimal => Some(ClaudeOutputEffort::Low),
        ReasoningEffort::Low => Some(ClaudeOutputEffort::Low),
        ReasoningEffort::Medium => Some(ClaudeOutputEffort::Medium),
        ReasoningEffort::High | ReasoningEffort::XHigh => Some(ClaudeOutputEffort::High),
    }
}

fn map_web_search_location(location: WebSearchApproximateLocation) -> ClaudeUserLocation {
    ClaudeUserLocation {
        r#type: ClaudeUserLocationType::Approximate,
        city: location.city,
        country: location.country,
        region: location.region,
        timezone: location.timezone,
    }
}

fn map_preview_location(location: ApproximateLocation) -> ClaudeUserLocation {
    ClaudeUserLocation {
        r#type: ClaudeUserLocationType::Approximate,
        city: location.city,
        country: location.country,
        region: location.region,
        timezone: location.timezone,
    }
}

fn clamp_i64_to_u32(value: i64) -> u32 {
    if value <= 0 {
        0
    } else if value > i64::from(u32::MAX) {
        u32::MAX
    } else {
        value as u32
    }
}
