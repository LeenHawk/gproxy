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
use gproxy_protocol::claude::create_message::request::{
    CreateMessageHeaders as ClaudeCreateMessageHeaders,
    CreateMessageRequest as ClaudeCreateMessageRequest,
    CreateMessageRequestBody as ClaudeCreateMessageRequestBody,
};
use gproxy_protocol::openai::create_response::request::CreateResponseRequest as OpenAIResponseRequest;
use gproxy_protocol::openai::create_response::types::{
    AllowedTool, EasyInputMessage, EasyInputMessageContent, EasyInputMessageRole, FunctionTool,
    InputContent, InputFileContent, InputItem, InputMessage, InputMessageRole, InputParam,
    Instructions, MCPAllowedTools, MCPTool, OutputMessage, OutputMessageContent, Reasoning,
    ReasoningEffort, ResponseTextParam, TextResponseFormatConfiguration, Tool, ToolChoiceAllowed,
    ToolChoiceAllowedMode, ToolChoiceBuiltInType, ToolChoiceOptions, ToolChoiceParam,
    ToolChoiceTypes,
};
use serde_json::Value as JsonValue;

/// Convert an OpenAI responses request into a Claude create-message request.
pub fn transform_request(request: OpenAIResponseRequest) -> ClaudeCreateMessageRequest {
    let mut messages = Vec::new();
    let mut system_texts = Vec::new();

    if let Some(instructions) = request.body.instructions {
        match instructions {
            Instructions::Text(text) => push_system_text(&mut system_texts, text),
            Instructions::Items(items) => {
                for item in items {
                    append_input_item(item, &mut messages, &mut system_texts);
                }
            }
        }
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

    ClaudeCreateMessageRequest {
        headers: ClaudeCreateMessageHeaders::default(),
        body: ClaudeCreateMessageRequestBody {
            max_tokens: map_max_tokens(request.body.max_output_tokens),
            messages,
            model: ClaudeModel::Custom(request.body.model),
            container: None,
            context_management: None,
            mcp_servers,
            metadata: map_metadata(request.body.metadata, request.body.user),
            output_config,
            output_format,
            service_tier: None,
            stop_sequences: None,
            stream: request.body.stream,
            system,
            temperature: request.body.temperature,
            thinking,
            tool_choice,
            tools,
            top_k: None,
            top_p: request.body.top_p,
        },
    }
}

fn map_max_tokens(max_output_tokens: Option<i64>) -> u32 {
    let value = max_output_tokens.unwrap_or(0);
    if value <= 0 {
        0
    } else if value > u32::MAX as i64 {
        u32::MAX
    } else {
        value as u32
    }
}

fn map_metadata(
    metadata: Option<gproxy_protocol::openai::create_response::types::Metadata>,
    user: Option<String>,
) -> Option<gproxy_protocol::claude::create_message::types::BetaMetadata> {
    let user_id = user.or_else(|| metadata.and_then(|meta| meta.get("user_id").cloned()));
    user_id.map(
        |user_id| gproxy_protocol::claude::create_message::types::BetaMetadata {
            user_id: Some(user_id),
        },
    )
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
        EasyInputMessageContent::Parts(parts) => {
            let mut blocks = Vec::new();
            for part in parts {
                if let Some(block) = map_input_content_to_block(&part) {
                    blocks.push(block);
                }
            }
            if blocks.is_empty() {
                None
            } else {
                Some(ClaudeMessageContent::Blocks(blocks))
            }
        }
    }
}

fn easy_message_content_to_text(content: EasyInputMessageContent) -> Option<String> {
    match content {
        EasyInputMessageContent::Text(text) => Some(text),
        EasyInputMessageContent::Parts(parts) => input_contents_to_text(&parts),
    }
}

fn input_contents_to_text(contents: &[InputContent]) -> Option<String> {
    let texts: Vec<String> = contents
        .iter()
        .filter_map(|content| match content {
            InputContent::InputText(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect();

    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}

fn input_contents_to_message_content(contents: &[InputContent]) -> Option<ClaudeMessageContent> {
    let mut blocks = Vec::new();
    for content in contents {
        if let Some(block) = map_input_content_to_block(content) {
            blocks.push(block);
        }
    }

    if blocks.is_empty() {
        None
    } else if blocks.len() == 1 {
        if let ClaudeContentBlockParam::Text(text) = &blocks[0] {
            Some(ClaudeMessageContent::Text(text.text.clone()))
        } else {
            Some(ClaudeMessageContent::Blocks(blocks))
        }
    } else {
        Some(ClaudeMessageContent::Blocks(blocks))
    }
}

fn output_contents_to_message_content(
    contents: &[OutputMessageContent],
) -> Option<ClaudeMessageContent> {
    let mut blocks = Vec::new();
    for content in contents {
        match content {
            OutputMessageContent::OutputText(text) => {
                push_text_block(&mut blocks, text.text.clone());
            }
            OutputMessageContent::Refusal(refusal) => {
                push_text_block(&mut blocks, refusal.refusal.clone());
            }
        }
    }

    if blocks.is_empty() {
        None
    } else if blocks.len() == 1 {
        if let ClaudeContentBlockParam::Text(text) = &blocks[0] {
            Some(ClaudeMessageContent::Text(text.text.clone()))
        } else {
            Some(ClaudeMessageContent::Blocks(blocks))
        }
    } else {
        Some(ClaudeMessageContent::Blocks(blocks))
    }
}

fn map_input_content_to_block(content: &InputContent) -> Option<ClaudeContentBlockParam> {
    match content {
        InputContent::InputText(text) => {
            Some(ClaudeContentBlockParam::Text(ClaudeTextBlockParam {
                text: text.text.clone(),
                r#type: ClaudeTextBlockType::Text,
                cache_control: None,
                citations: None,
            }))
        }
        InputContent::InputImage(image) => match (&image.image_url, &image.file_id) {
            (Some(url), _) => Some(ClaudeContentBlockParam::Image(ClaudeImageBlockParam {
                source: ClaudeImageSource::Url { url: url.clone() },
                r#type: ClaudeImageBlockType::Image,
                cache_control: None,
            })),
            (_, Some(file_id)) => Some(ClaudeContentBlockParam::Image(ClaudeImageBlockParam {
                source: ClaudeImageSource::File {
                    file_id: file_id.clone(),
                },
                r#type: ClaudeImageBlockType::Image,
                cache_control: None,
            })),
            _ => None,
        },
        InputContent::InputFile(file) => map_file_content_to_block(file),
    }
}

fn map_file_content_to_block(file: &InputFileContent) -> Option<ClaudeContentBlockParam> {
    if let Some(file_id) = &file.file_id {
        return Some(ClaudeContentBlockParam::Document(ClaudeDocumentBlock {
            source: ClaudeDocumentSource::File {
                file_id: file_id.clone(),
            },
            r#type: ClaudeDocumentBlockType::Document,
            cache_control: None,
            citations: None,
            context: None,
            title: file.filename.clone(),
        }));
    }

    if let Some(file_url) = &file.file_url {
        return Some(ClaudeContentBlockParam::Document(ClaudeDocumentBlock {
            source: ClaudeDocumentSource::Url {
                url: file_url.clone(),
            },
            r#type: ClaudeDocumentBlockType::Document,
            cache_control: None,
            citations: None,
            context: None,
            title: file.filename.clone(),
        }));
    }

    if let Some(file_data) = &file.file_data {
        return Some(ClaudeContentBlockParam::Document(ClaudeDocumentBlock {
            source: ClaudeDocumentSource::Base64 {
                data: file_data.clone(),
                media_type:
                    gproxy_protocol::claude::count_tokens::types::BetaPdfMediaType::ApplicationPdf,
            },
            r#type: ClaudeDocumentBlockType::Document,
            cache_control: None,
            citations: None,
            context: None,
            title: file.filename.clone(),
        }));
    }

    None
}

fn push_text_block(blocks: &mut Vec<ClaudeContentBlockParam>, text: String) {
    if !text.is_empty() {
        blocks.push(ClaudeContentBlockParam::Text(ClaudeTextBlockParam {
            text,
            r#type: ClaudeTextBlockType::Text,
            cache_control: None,
            citations: None,
        }));
    }
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
                    display_height_px: tool.display_height as u32,
                    display_width_px: tool.display_width as u32,
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

fn map_web_search_location(
    location: gproxy_protocol::openai::create_response::types::WebSearchApproximateLocation,
) -> ClaudeUserLocation {
    ClaudeUserLocation {
        r#type: ClaudeUserLocationType::Approximate,
        city: location.city,
        country: location.country,
        region: location.region,
        timezone: location.timezone,
    }
}

fn map_preview_location(
    location: gproxy_protocol::openai::create_response::types::ApproximateLocation,
) -> ClaudeUserLocation {
    ClaudeUserLocation {
        r#type: ClaudeUserLocationType::Approximate,
        city: location.city,
        country: location.country,
        region: location.region,
        timezone: location.timezone,
    }
}

fn map_tool_choice(choice: ToolChoiceParam, parallel_tool_calls: Option<bool>) -> ClaudeToolChoice {
    let disable_parallel_tool_use = parallel_tool_calls.map(|value| !value);

    match choice {
        ToolChoiceParam::Mode(mode) => match mode {
            ToolChoiceOptions::None => ClaudeToolChoice::None,
            ToolChoiceOptions::Auto => ClaudeToolChoice::Auto {
                disable_parallel_tool_use,
            },
            ToolChoiceOptions::Required => ClaudeToolChoice::Any {
                disable_parallel_tool_use,
            },
        },
        ToolChoiceParam::Allowed(allowed) => map_allowed_tools(allowed, disable_parallel_tool_use),
        ToolChoiceParam::BuiltIn(tool) => map_builtin_choice(tool, disable_parallel_tool_use),
        ToolChoiceParam::Function(tool) => ClaudeToolChoice::Tool {
            name: tool.name,
            disable_parallel_tool_use,
        },
        ToolChoiceParam::Custom(tool) => ClaudeToolChoice::Tool {
            name: tool.name,
            disable_parallel_tool_use,
        },
        ToolChoiceParam::MCP(tool) => ClaudeToolChoice::Tool {
            name: tool.name.unwrap_or(tool.server_label),
            disable_parallel_tool_use,
        },
        ToolChoiceParam::ApplyPatch(_) => ClaudeToolChoice::Tool {
            name: "text_editor".to_string(),
            disable_parallel_tool_use,
        },
        ToolChoiceParam::Shell(_) => ClaudeToolChoice::Tool {
            name: "bash".to_string(),
            disable_parallel_tool_use,
        },
    }
}

fn map_allowed_tools(
    allowed: ToolChoiceAllowed,
    disable_parallel_tool_use: Option<bool>,
) -> ClaudeToolChoice {
    let mode = match allowed.mode {
        ToolChoiceAllowedMode::Auto => ClaudeToolChoice::Auto {
            disable_parallel_tool_use,
        },
        ToolChoiceAllowedMode::Required => ClaudeToolChoice::Any {
            disable_parallel_tool_use,
        },
    };

    let mut names = Vec::new();
    for tool in allowed.tools {
        match tool {
            AllowedTool::Function { name } => names.push(name),
            AllowedTool::Custom { name } => names.push(name),
            AllowedTool::MCP { server_label, name } => names.push(name.unwrap_or(server_label)),
            AllowedTool::FileSearch => names.push("file_search".to_string()),
            AllowedTool::WebSearch
            | AllowedTool::WebSearch20250826
            | AllowedTool::WebSearchPreview
            | AllowedTool::WebSearchPreview20250311 => names.push("web_search".to_string()),
            AllowedTool::ComputerUsePreview => names.push("computer".to_string()),
            AllowedTool::CodeInterpreter => names.push("code_execution".to_string()),
            AllowedTool::ImageGeneration => names.push("image_generation".to_string()),
            AllowedTool::LocalShell | AllowedTool::Shell => names.push("bash".to_string()),
            AllowedTool::ApplyPatch => names.push("text_editor".to_string()),
        }
    }

    if names.len() == 1 {
        ClaudeToolChoice::Tool {
            name: names.remove(0),
            disable_parallel_tool_use,
        }
    } else {
        mode
    }
}

fn map_builtin_choice(
    choice: ToolChoiceTypes,
    disable_parallel_tool_use: Option<bool>,
) -> ClaudeToolChoice {
    match choice.r#type {
        ToolChoiceBuiltInType::FileSearch => ClaudeToolChoice::Tool {
            name: "file_search".to_string(),
            disable_parallel_tool_use,
        },
        ToolChoiceBuiltInType::WebSearchPreview
        | ToolChoiceBuiltInType::WebSearchPreview20250311 => ClaudeToolChoice::Tool {
            name: "web_search".to_string(),
            disable_parallel_tool_use,
        },
        ToolChoiceBuiltInType::ComputerUsePreview => ClaudeToolChoice::Tool {
            name: "computer".to_string(),
            disable_parallel_tool_use,
        },
        ToolChoiceBuiltInType::ImageGeneration => ClaudeToolChoice::Tool {
            name: "image_generation".to_string(),
            disable_parallel_tool_use,
        },
        ToolChoiceBuiltInType::CodeInterpreter => ClaudeToolChoice::Tool {
            name: "code_execution".to_string(),
            disable_parallel_tool_use,
        },
    }
}

fn map_reasoning(
    reasoning: Option<Reasoning>,
) -> (
    Option<ClaudeThinkingConfigParam>,
    Option<ClaudeOutputConfig>,
) {
    let effort = reasoning.and_then(|reasoning| reasoning.effort);

    let (thinking, output_effort) = match effort {
        None | Some(ReasoningEffort::None) => (Some(ClaudeThinkingConfigParam::Disabled), None),
        Some(ReasoningEffort::Minimal) => (
            Some(ClaudeThinkingConfigParam::Enabled {
                budget_tokens: 1024,
            }),
            Some(ClaudeOutputEffort::Low),
        ),
        Some(ReasoningEffort::Low) => (
            Some(ClaudeThinkingConfigParam::Enabled {
                budget_tokens: 1024,
            }),
            Some(ClaudeOutputEffort::Low),
        ),
        Some(ReasoningEffort::Medium) => (
            Some(ClaudeThinkingConfigParam::Enabled {
                budget_tokens: 1024,
            }),
            Some(ClaudeOutputEffort::Medium),
        ),
        Some(ReasoningEffort::High) | Some(ReasoningEffort::XHigh) => (
            Some(ClaudeThinkingConfigParam::Enabled {
                budget_tokens: 1024,
            }),
            Some(ClaudeOutputEffort::High),
        ),
    };

    let output_config = output_effort.map(|effort| ClaudeOutputConfig {
        effort: Some(effort),
    });

    (thinking, output_config)
}

fn map_output_format(text: Option<ResponseTextParam>) -> Option<ClaudeJSONOutputFormat> {
    let format = text.and_then(|text| text.format)?;

    match format {
        TextResponseFormatConfiguration::Text => None,
        TextResponseFormatConfiguration::JsonObject => Some(ClaudeJSONOutputFormat {
            schema: minimal_object_schema(),
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

fn minimal_object_schema() -> JsonValue {
    let mut map = serde_json::Map::new();
    map.insert("type".to_string(), JsonValue::String("object".to_string()));
    JsonValue::Object(map)
}
