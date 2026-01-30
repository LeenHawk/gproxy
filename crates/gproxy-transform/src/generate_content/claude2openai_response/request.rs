use gproxy_protocol::claude::count_tokens::types::{
    BetaContentBlockParam as ClaudeContentBlockParam, BetaDocumentSource as ClaudeDocumentSource,
    BetaImageSource as ClaudeImageSource, BetaJSONOutputFormat as ClaudeJSONOutputFormat,
    BetaMCPToolset as ClaudeMCPToolset, BetaMessageContent as ClaudeMessageContent,
    BetaMessageParam as ClaudeMessageParam, BetaMessageRole as ClaudeMessageRole,
    BetaOutputConfig as ClaudeOutputConfig, BetaOutputEffort as ClaudeOutputEffort,
    BetaRequestDocumentBlock as ClaudeDocumentBlock,
    BetaRequestMCPServerURLDefinition as ClaudeMCPServerURLDefinition,
    BetaRequestMCPServerURLDefinitionType as ClaudeMCPServerURLDefinitionType,
    BetaThinkingConfigParam as ClaudeThinkingConfigParam, BetaTool as ClaudeTool,
    BetaToolBuiltin as ClaudeToolBuiltin, BetaToolChoice as ClaudeToolChoice,
    BetaToolCustom as ClaudeToolCustom, BetaUserLocation as ClaudeUserLocation,
    BetaWebSearchTool as ClaudeWebSearchTool, Model as ClaudeModel,
};
use gproxy_protocol::claude::create_message::request::CreateMessageRequest as ClaudeCreateMessageRequest;
use gproxy_protocol::openai::create_response::request::{
    CreateResponseRequest as OpenAIResponseRequest,
    CreateResponseRequestBody as OpenAIResponseRequestBody,
};
use gproxy_protocol::openai::create_response::types::{
    CodeInterpreterContainer, CodeInterpreterContainerParams, CodeInterpreterTool,
    ComputerEnvironment, ComputerUsePreviewTool, EasyInputMessage, EasyInputMessageContent,
    EasyInputMessageRole, EasyInputMessageType, FileSearchTool, FunctionShellTool, FunctionTool,
    InputContent, InputFileContent, InputImageContent, InputItem, InputMessage, InputMessageRole,
    InputParam, InputTextContent, Instructions, MCPAllowedTools, MCPTool, Reasoning,
    ReasoningEffort, ResponseTextParam, TextResponseFormatConfiguration, Tool, ToolChoiceOptions,
    ToolChoiceParam, WebSearchApproximateLocation, WebSearchFilters, WebSearchTool,
};
use serde_json::Value as JsonValue;

/// Convert a Claude create-message request into an OpenAI responses request.
pub fn transform_request(request: ClaudeCreateMessageRequest) -> OpenAIResponseRequest {
    let model = match &request.body.model {
        ClaudeModel::Custom(value) => value.clone(),
        ClaudeModel::Known(known) => match serde_json::to_value(known) {
            Ok(JsonValue::String(value)) => value,
            _ => "unknown".to_string(),
        },
    };

    let input = map_messages_to_input(&request.body.messages);
    let instructions = map_system_to_instructions(request.body.system);

    let tools = map_tools(request.body.tools, request.body.mcp_servers);
    let (tool_choice, parallel_tool_calls) = map_tool_choice(request.body.tool_choice);

    let reasoning = map_reasoning(request.body.thinking, request.body.output_config);
    let text = map_output_format(request.body.output_format);

    OpenAIResponseRequest {
        body: OpenAIResponseRequestBody {
            model,
            input,
            include: None,
            parallel_tool_calls,
            store: None,
            instructions,
            stream: request.body.stream,
            stream_options: None,
            conversation: None,
            previous_response_id: None,
            reasoning,
            background: None,
            max_output_tokens: Some(request.body.max_tokens as i64),
            max_tool_calls: None,
            text,
            tools,
            tool_choice,
            prompt: None,
            truncation: None,
            top_logprobs: None,
            metadata: map_metadata(request.body.metadata),
            temperature: request.body.temperature,
            top_p: request.body.top_p,
            user: None,
            safety_identifier: None,
            prompt_cache_key: None,
            service_tier: None,
            prompt_cache_retention: None,
        },
    }
}

fn map_system_to_instructions(
    system: Option<gproxy_protocol::claude::count_tokens::types::BetaSystemParam>,
) -> Option<Instructions> {
    let text = match system {
        Some(gproxy_protocol::claude::count_tokens::types::BetaSystemParam::Text(text)) => {
            Some(text)
        }
        Some(gproxy_protocol::claude::count_tokens::types::BetaSystemParam::Blocks(blocks)) => {
            let texts: Vec<String> = blocks.into_iter().map(|block| block.text).collect();
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n"))
            }
        }
        None => None,
    }?;

    Some(Instructions::Text(text))
}

fn map_messages_to_input(messages: &[ClaudeMessageParam]) -> Option<InputParam> {
    let mut items = Vec::new();

    for message in messages {
        if let Some(item) = map_message_to_item(message) {
            items.push(item);
        }
    }

    if items.is_empty() {
        None
    } else {
        Some(InputParam::Items(items))
    }
}

fn map_message_to_item(message: &ClaudeMessageParam) -> Option<InputItem> {
    match message.role {
        ClaudeMessageRole::User => {
            map_message_as_input(message, InputMessageRole::User).map(|msg| {
                InputItem::Item(
                    gproxy_protocol::openai::create_response::types::Item::InputMessage(msg),
                )
            })
        }
        ClaudeMessageRole::Assistant => {
            let content = map_message_content_to_easy_content(&message.content)?;
            Some(InputItem::EasyMessage(EasyInputMessage {
                r#type: EasyInputMessageType::Message,
                role: EasyInputMessageRole::Assistant,
                content,
            }))
        }
    }
}

fn map_message_as_input(
    message: &ClaudeMessageParam,
    role: InputMessageRole,
) -> Option<InputMessage> {
    let content = map_message_content_to_input_contents(&message.content);
    if content.is_empty() {
        None
    } else {
        Some(InputMessage {
            r#type: None,
            role,
            status: None,
            content,
        })
    }
}

fn map_message_content_to_easy_content(
    content: &ClaudeMessageContent,
) -> Option<EasyInputMessageContent> {
    match content {
        ClaudeMessageContent::Text(text) => Some(EasyInputMessageContent::Text(text.clone())),
        ClaudeMessageContent::Blocks(blocks) => {
            let mut parts = Vec::new();
            for block in blocks {
                if let Some(part) = map_block_to_input_content(block) {
                    parts.push(part);
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(EasyInputMessageContent::Parts(parts))
            }
        }
    }
}

fn map_message_content_to_input_contents(content: &ClaudeMessageContent) -> Vec<InputContent> {
    match content {
        ClaudeMessageContent::Text(text) => vec![InputContent::InputText(InputTextContent {
            text: text.clone(),
        })],
        ClaudeMessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(map_block_to_input_content)
            .collect(),
    }
}

fn map_block_to_input_content(block: &ClaudeContentBlockParam) -> Option<InputContent> {
    match block {
        ClaudeContentBlockParam::Text(text_block) => {
            Some(InputContent::InputText(InputTextContent {
                text: text_block.text.clone(),
            }))
        }
        ClaudeContentBlockParam::Image(image_block) => match &image_block.source {
            ClaudeImageSource::Url { url } => Some(InputContent::InputImage(InputImageContent {
                image_url: Some(url.clone()),
                file_id: None,
                detail: None,
            })),
            ClaudeImageSource::File { file_id } => {
                Some(InputContent::InputImage(InputImageContent {
                    image_url: None,
                    file_id: Some(file_id.clone()),
                    detail: None,
                }))
            }
            ClaudeImageSource::Base64 { data, .. } => {
                Some(InputContent::InputFile(InputFileContent {
                    file_id: None,
                    filename: None,
                    file_url: None,
                    file_data: Some(data.clone()),
                }))
            }
        },
        ClaudeContentBlockParam::Document(document) => map_document_to_input_content(document),
        _ => None,
    }
}

fn map_document_to_input_content(document: &ClaudeDocumentBlock) -> Option<InputContent> {
    match &document.source {
        ClaudeDocumentSource::File { file_id } => Some(InputContent::InputFile(InputFileContent {
            file_id: Some(file_id.clone()),
            filename: document.title.clone(),
            file_url: None,
            file_data: None,
        })),
        ClaudeDocumentSource::Url { url } => Some(InputContent::InputFile(InputFileContent {
            file_id: None,
            filename: document.title.clone(),
            file_url: Some(url.clone()),
            file_data: None,
        })),
        ClaudeDocumentSource::Base64 { data, .. } => {
            Some(InputContent::InputFile(InputFileContent {
                file_id: None,
                filename: document.title.clone(),
                file_url: None,
                file_data: Some(data.clone()),
            }))
        }
        _ => None,
    }
}

fn map_tools(
    tools: Option<Vec<ClaudeTool>>,
    mcp_servers: Option<Vec<ClaudeMCPServerURLDefinition>>,
) -> Option<Vec<Tool>> {
    let mut output = Vec::new();

    if let Some(tools) = tools {
        for tool in tools {
            if let Some(mapped) = map_tool(tool) {
                output.push(mapped);
            }
        }
    }

    if let Some(servers) = mcp_servers {
        for server in servers {
            if let Some(tool) = map_mcp_server(server) {
                output.push(tool);
            }
        }
    }

    if output.is_empty() {
        None
    } else {
        Some(output)
    }
}

fn map_tool(tool: ClaudeTool) -> Option<Tool> {
    match tool {
        ClaudeTool::Custom(custom) => Some(Tool::Function(map_custom_tool(custom))),
        ClaudeTool::Builtin(builtin) => map_builtin_tool(builtin),
    }
}

fn map_custom_tool(tool: ClaudeToolCustom) -> FunctionTool {
    let mut schema = serde_json::Map::new();
    schema.insert("type".to_string(), JsonValue::String("object".to_string()));

    if let Some(properties) = tool.input_schema.properties {
        schema.insert(
            "properties".to_string(),
            JsonValue::Object(properties.into_iter().collect()),
        );
    }

    if let Some(required) = tool.input_schema.required {
        schema.insert(
            "required".to_string(),
            JsonValue::Array(required.into_iter().map(JsonValue::String).collect()),
        );
    }

    FunctionTool {
        name: tool.name,
        description: tool.description,
        parameters: Some(JsonValue::Object(schema)),
        strict: tool.strict,
    }
}

fn map_builtin_tool(builtin: ClaudeToolBuiltin) -> Option<Tool> {
    match builtin {
        ClaudeToolBuiltin::Bash20241022(_) | ClaudeToolBuiltin::Bash20250124(_) => {
            Some(Tool::Shell(FunctionShellTool {}))
        }
        ClaudeToolBuiltin::CodeExecution20250522(_)
        | ClaudeToolBuiltin::CodeExecution20250825(_) => {
            Some(Tool::CodeInterpreter(CodeInterpreterTool {
                container: CodeInterpreterContainer::Params(CodeInterpreterContainerParams {
                    file_ids: Vec::new(),
                    memory_limit: None,
                }),
            }))
        }
        ClaudeToolBuiltin::ComputerUse20241022(tool)
        | ClaudeToolBuiltin::ComputerUse20250124(tool)
        | ClaudeToolBuiltin::ComputerUse20251124(tool) => {
            Some(Tool::ComputerUsePreview(ComputerUsePreviewTool {
                environment: ComputerEnvironment::Browser,
                display_width: tool.display_width_px as i64,
                display_height: tool.display_height_px as i64,
            }))
        }
        ClaudeToolBuiltin::TextEditor20241022(_)
        | ClaudeToolBuiltin::TextEditor20250124(_)
        | ClaudeToolBuiltin::TextEditor20250429(_)
        | ClaudeToolBuiltin::TextEditor20250728(_) => Some(Tool::ApplyPatch(
            gproxy_protocol::openai::create_response::types::ApplyPatchTool {},
        )),
        ClaudeToolBuiltin::Memory20250818(_)
        | ClaudeToolBuiltin::WebFetch20250910(_)
        | ClaudeToolBuiltin::ToolSearchToolRegex(_)
        | ClaudeToolBuiltin::ToolSearchToolRegex20251119(_) => None,
        ClaudeToolBuiltin::WebSearch20250305(tool) => {
            Some(Tool::WebSearch(map_web_search_tool(tool)))
        }
        ClaudeToolBuiltin::ToolSearchToolBm25(_tool)
        | ClaudeToolBuiltin::ToolSearchToolBm2520251119(_tool) => {
            Some(Tool::FileSearch(FileSearchTool {
                vector_store_ids: Vec::new(),
                max_num_results: None,
                ranking_options: None,
                filters: None,
            }))
        }
        ClaudeToolBuiltin::McpToolset(tool) => map_mcp_toolset(tool),
    }
}

fn map_web_search_tool(tool: ClaudeWebSearchTool) -> WebSearchTool {
    WebSearchTool {
        filters: tool
            .allowed_domains
            .map(|allowed_domains| WebSearchFilters {
                allowed_domains: Some(allowed_domains),
            }),
        user_location: tool.user_location.map(map_user_location),
        search_context_size: None,
    }
}

fn map_user_location(location: ClaudeUserLocation) -> WebSearchApproximateLocation {
    WebSearchApproximateLocation {
        r#type: gproxy_protocol::openai::create_response::types::WebSearchLocationType::Approximate,
        city: location.city,
        country: location.country,
        region: location.region,
        timezone: location.timezone,
    }
}

fn map_mcp_toolset(tool: ClaudeMCPToolset) -> Option<Tool> {
    Some(Tool::MCP(MCPTool {
        server_label: tool.mcp_server_name,
        allowed_tools: None,
        authorization: None,
        connector_id: None,
        headers: None,
        require_approval: None,
        server_description: None,
        server_url: None,
    }))
}

fn map_mcp_server(server: ClaudeMCPServerURLDefinition) -> Option<Tool> {
    let allowed_tools = server
        .tool_configuration
        .and_then(|config| config.allowed_tools)
        .map(MCPAllowedTools::Names);

    let tool = MCPTool {
        server_label: server.name,
        allowed_tools,
        authorization: server.authorization_token,
        connector_id: None,
        headers: None,
        require_approval: None,
        server_description: None,
        server_url: match server.r#type {
            ClaudeMCPServerURLDefinitionType::Url => Some(server.url),
        },
    };

    Some(Tool::MCP(tool))
}

fn map_tool_choice(choice: Option<ClaudeToolChoice>) -> (Option<ToolChoiceParam>, Option<bool>) {
    match choice {
        Some(ClaudeToolChoice::Auto {
            disable_parallel_tool_use,
        }) => (
            Some(ToolChoiceParam::Mode(ToolChoiceOptions::Auto)),
            disable_parallel_tool_use.map(|value| !value),
        ),
        Some(ClaudeToolChoice::Any {
            disable_parallel_tool_use,
        }) => (
            Some(ToolChoiceParam::Mode(ToolChoiceOptions::Required)),
            disable_parallel_tool_use.map(|value| !value),
        ),
        Some(ClaudeToolChoice::Tool {
            name,
            disable_parallel_tool_use,
        }) => (
            Some(ToolChoiceParam::Function(
                gproxy_protocol::openai::create_response::types::ToolChoiceFunction {
                    r#type: gproxy_protocol::openai::create_response::types::ToolChoiceFunctionType::Function,
                    name,
                },
            )),
            disable_parallel_tool_use.map(|value| !value),
        ),
        Some(ClaudeToolChoice::None) => (Some(ToolChoiceParam::Mode(ToolChoiceOptions::None)), None),
        None => (None, None),
    }
}

fn map_reasoning(
    thinking: Option<ClaudeThinkingConfigParam>,
    output_config: Option<ClaudeOutputConfig>,
) -> Option<Reasoning> {
    let effort = output_config.and_then(|config| config.effort);
    let thinking_enabled = matches!(thinking, Some(ClaudeThinkingConfigParam::Enabled { .. }));

    let effort = if !thinking_enabled {
        ReasoningEffort::Medium
    } else {
        match effort {
            Some(ClaudeOutputEffort::Low) => ReasoningEffort::Low,
            Some(ClaudeOutputEffort::Medium) => ReasoningEffort::Medium,
            Some(ClaudeOutputEffort::High) => ReasoningEffort::High,
            None => ReasoningEffort::Medium,
        }
    };

    Some(Reasoning {
        effort: Some(effort),
        summary: None,
        generate_summary: None,
    })
}

fn map_output_format(output_format: Option<ClaudeJSONOutputFormat>) -> Option<ResponseTextParam> {
    output_format.map(|format| ResponseTextParam {
        format: Some(TextResponseFormatConfiguration::JsonSchema {
            name: "response".to_string(),
            description: None,
            schema: format.schema,
            strict: None,
        }),
        verbosity: None,
    })
}

fn map_metadata(
    metadata: Option<gproxy_protocol::claude::create_message::types::BetaMetadata>,
) -> Option<gproxy_protocol::openai::create_response::types::Metadata> {
    let metadata = metadata?;
    let mut map = std::collections::BTreeMap::new();
    if let Some(user_id) = metadata.user_id {
        map.insert("user_id".to_string(), user_id);
    }
    if map.is_empty() { None } else { Some(map) }
}
