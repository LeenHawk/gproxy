use gproxy_protocol::claude::count_tokens::request::CountTokensRequest as ClaudeCountTokensRequest;
use gproxy_protocol::claude::count_tokens::types::{
    BetaContentBlockParam as ClaudeContentBlockParam, BetaDocumentSource as ClaudeDocumentSource,
    BetaImageSource as ClaudeImageSource, BetaJSONOutputFormat as ClaudeJSONOutputFormat,
    BetaMCPToolset as ClaudeMCPToolset, BetaMessageContent as ClaudeMessageContent,
    BetaMessageParam as ClaudeMessageParam, BetaMessageRole as ClaudeMessageRole,
    BetaOutputConfig as ClaudeOutputConfig, BetaOutputEffort as ClaudeOutputEffort,
    BetaRequestDocumentBlock as ClaudeDocumentBlock,
    BetaRequestMCPServerURLDefinition as ClaudeMCPServerURLDefinition,
    BetaSystemParam as ClaudeSystemParam, BetaThinkingConfigParam as ClaudeThinkingConfigParam,
    BetaTool as ClaudeTool, BetaToolBuiltin as ClaudeToolBuiltin,
    BetaToolChoice as ClaudeToolChoice, BetaToolCustom as ClaudeToolCustom,
    BetaUserLocation as ClaudeUserLocation, BetaWebSearchTool as ClaudeWebSearchTool,
    Model as ClaudeModel,
};
use gproxy_protocol::openai::count_tokens::request::{
    InputTokenCountRequest as OpenAIInputTokenCountRequest,
    InputTokenCountRequestBody as OpenAIInputTokenCountRequestBody,
};
use gproxy_protocol::openai::create_response::types::{
    CodeInterpreterContainer, CodeInterpreterContainerParams, CodeInterpreterTool,
    ComputerEnvironment, ComputerUsePreviewTool, EasyInputMessage, EasyInputMessageContent,
    EasyInputMessageRole, EasyInputMessageType, FileSearchTool, FunctionShellTool, FunctionTool,
    InputContent, InputFileContent, InputImageContent, InputItem, InputMessage, InputMessageRole,
    InputParam, InputTextContent, Item, MCPAllowedTools, MCPTool, Reasoning, ReasoningEffort,
    ResponseTextParam, SpecificApplyPatchParam, SpecificApplyPatchType, SpecificFunctionShellParam,
    SpecificFunctionShellType, TextResponseFormatConfiguration, Tool, ToolChoiceBuiltInType,
    ToolChoiceCustom, ToolChoiceCustomType, ToolChoiceOptions, ToolChoiceParam, ToolChoiceTypes,
    WebSearchApproximateLocation, WebSearchFilters, WebSearchTool,
};
use serde_json::Value as JsonValue;

/// Convert a Claude count-tokens request into OpenAI's input-tokens request shape.
pub fn transform_request(request: ClaudeCountTokensRequest) -> OpenAIInputTokenCountRequest {
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

    OpenAIInputTokenCountRequest {
        body: OpenAIInputTokenCountRequestBody {
            model,
            input,
            previous_response_id: None,
            tools,
            text,
            reasoning,
            truncation: None,
            instructions,
            conversation: None,
            tool_choice,
            parallel_tool_calls,
        },
    }
}

fn map_system_to_instructions(system: Option<ClaudeSystemParam>) -> Option<String> {
    match system {
        Some(ClaudeSystemParam::Text(text)) => Some(text),
        Some(ClaudeSystemParam::Blocks(blocks)) => {
            let texts: Vec<String> = blocks.into_iter().map(|block| block.text).collect();
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n"))
            }
        }
        None => None,
    }
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
        ClaudeMessageRole::User => map_message_as_input(message, InputMessageRole::User)
            .map(|msg| InputItem::Item(Item::InputMessage(msg))),
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
        ClaudeDocumentSource::Text { data, .. } => {
            Some(InputContent::InputText(InputTextContent {
                text: data.clone(),
            }))
        }
        ClaudeDocumentSource::Content { content } => match content {
            gproxy_protocol::claude::count_tokens::types::BetaContentBlockSourceContent::Text(
                text,
            ) => Some(InputContent::InputText(InputTextContent {
                text: text.clone(),
            })),
            gproxy_protocol::claude::count_tokens::types::BetaContentBlockSourceContent::Blocks(
                blocks,
            ) => {
                let mut texts = Vec::new();
                for block in blocks {
                    if let ClaudeContentBlockParam::Text(text_block) = block {
                        texts.push(text_block.text.clone());
                    }
                }
                if texts.is_empty() {
                    None
                } else {
                    Some(InputContent::InputText(InputTextContent {
                        text: texts.join("\n"),
                    }))
                }
            }
        },
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
        server_url: None,
        connector_id: None,
        authorization: None,
        server_description: None,
        headers: None,
        allowed_tools: None,
        require_approval: None,
    }))
}

fn map_mcp_server(server: ClaudeMCPServerURLDefinition) -> Option<Tool> {
    let allowed_tools = server
        .tool_configuration
        .and_then(|config| config.allowed_tools)
        .map(MCPAllowedTools::Names);

    Some(Tool::MCP(MCPTool {
        server_label: server.name,
        server_url: Some(server.url),
        connector_id: None,
        authorization: server.authorization_token,
        server_description: None,
        headers: None,
        allowed_tools,
        require_approval: None,
    }))
}

fn map_tool_choice(choice: Option<ClaudeToolChoice>) -> (Option<ToolChoiceParam>, Option<bool>) {
    let choice = match choice {
        Some(choice) => choice,
        None => return (None, None),
    };

    match choice {
        ClaudeToolChoice::None => (Some(ToolChoiceParam::Mode(ToolChoiceOptions::None)), None),
        ClaudeToolChoice::Auto {
            disable_parallel_tool_use,
        } => (
            Some(ToolChoiceParam::Mode(ToolChoiceOptions::Auto)),
            disable_parallel_tool_use.map(|value| !value),
        ),
        ClaudeToolChoice::Any {
            disable_parallel_tool_use,
        } => (
            Some(ToolChoiceParam::Mode(ToolChoiceOptions::Required)),
            disable_parallel_tool_use.map(|value| !value),
        ),
        ClaudeToolChoice::Tool {
            name,
            disable_parallel_tool_use,
        } => {
            let tool_choice = match name.as_str() {
                "file_search" => ToolChoiceParam::BuiltIn(ToolChoiceTypes {
                    r#type: ToolChoiceBuiltInType::FileSearch,
                }),
                "web_search" => ToolChoiceParam::BuiltIn(ToolChoiceTypes {
                    r#type: ToolChoiceBuiltInType::WebSearchPreview,
                }),
                "computer" => ToolChoiceParam::BuiltIn(ToolChoiceTypes {
                    r#type: ToolChoiceBuiltInType::ComputerUsePreview,
                }),
                "code_execution" => ToolChoiceParam::BuiltIn(ToolChoiceTypes {
                    r#type: ToolChoiceBuiltInType::CodeInterpreter,
                }),
                "image_generation" => ToolChoiceParam::BuiltIn(ToolChoiceTypes {
                    r#type: ToolChoiceBuiltInType::ImageGeneration,
                }),
                "bash" => ToolChoiceParam::Shell(SpecificFunctionShellParam {
                    r#type: SpecificFunctionShellType::Shell,
                }),
                "text_editor" => ToolChoiceParam::ApplyPatch(SpecificApplyPatchParam {
                    r#type: SpecificApplyPatchType::ApplyPatch,
                }),
                _ => ToolChoiceParam::Custom(ToolChoiceCustom {
                    r#type: ToolChoiceCustomType::Custom,
                    name,
                }),
            };

            (
                Some(tool_choice),
                disable_parallel_tool_use.map(|value| !value),
            )
        }
    }
}

fn map_reasoning(
    thinking: Option<ClaudeThinkingConfigParam>,
    output_config: Option<ClaudeOutputConfig>,
) -> Option<Reasoning> {
    let effort = output_config
        .and_then(|config| config.effort)
        .and_then(map_output_effort)
        .or(match thinking {
            Some(ClaudeThinkingConfigParam::Disabled) => Some(ReasoningEffort::None),
            Some(ClaudeThinkingConfigParam::Enabled { .. }) => Some(ReasoningEffort::Medium),
            None => None,
        });

    effort.map(|effort| Reasoning {
        effort: Some(effort),
        summary: None,
        generate_summary: None,
    })
}

fn map_output_effort(effort: ClaudeOutputEffort) -> Option<ReasoningEffort> {
    match effort {
        ClaudeOutputEffort::Low => Some(ReasoningEffort::Low),
        ClaudeOutputEffort::Medium => Some(ReasoningEffort::Medium),
        ClaudeOutputEffort::High => Some(ReasoningEffort::High),
    }
}

fn map_output_format(output_format: Option<ClaudeJSONOutputFormat>) -> Option<ResponseTextParam> {
    let output_format = output_format?;

    Some(ResponseTextParam {
        format: Some(TextResponseFormatConfiguration::JsonSchema {
            name: "schema".to_string(),
            description: None,
            schema: output_format.schema,
            strict: None,
        }),
        verbosity: None,
    })
}
