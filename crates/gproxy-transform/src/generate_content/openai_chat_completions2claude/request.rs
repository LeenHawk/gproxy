use gproxy_protocol::claude::count_tokens::types::{
    BetaContentBlockParam as ClaudeContentBlockParam,
    BetaDocumentBlockType as ClaudeDocumentBlockType, BetaDocumentSource as ClaudeDocumentSource,
    BetaImageBlockParam as ClaudeImageBlockParam, BetaImageBlockType as ClaudeImageBlockType,
    BetaImageMediaType as ClaudeImageMediaType, BetaImageSource as ClaudeImageSource,
    BetaMessageContent as ClaudeMessageContent, BetaMessageParam as ClaudeMessageParam,
    BetaMessageRole as ClaudeMessageRole, BetaOutputConfig as ClaudeOutputConfig,
    BetaOutputEffort as ClaudeOutputEffort, BetaRequestDocumentBlock as ClaudeDocumentBlock,
    BetaSystemParam as ClaudeSystemParam, BetaThinkingConfigParam as ClaudeThinkingConfigParam,
    BetaTool as ClaudeTool, BetaToolBuiltin as ClaudeToolBuiltin,
    BetaToolChoice as ClaudeToolChoice, BetaToolCustom as ClaudeToolCustom,
    BetaToolInputSchema as ClaudeToolInputSchema,
    BetaToolInputSchemaType as ClaudeToolInputSchemaType,
    BetaToolResultBlockParam as ClaudeToolResultBlock,
    BetaToolResultBlockType as ClaudeToolResultBlockType,
    BetaToolResultContent as ClaudeToolResultContent, BetaToolUseBlockParam as ClaudeToolUseBlock,
    BetaToolUseBlockType as ClaudeToolUseBlockType, BetaUserLocation as ClaudeUserLocation,
    BetaWebSearchTool as ClaudeWebSearchTool, Model as ClaudeModel,
};
use gproxy_protocol::claude::create_message::request::{
    CreateMessageHeaders as ClaudeCreateMessageHeaders,
    CreateMessageRequest as ClaudeCreateMessageRequest,
    CreateMessageRequestBody as ClaudeCreateMessageRequestBody,
};
use gproxy_protocol::openai::create_chat_completions::request::CreateChatCompletionRequest as OpenAIChatCompletionRequest;
use gproxy_protocol::openai::create_chat_completions::types::{
    AllowedToolsMode, ChatCompletionAllowedTool, ChatCompletionAllowedToolsChoice,
    ChatCompletionAssistantContent, ChatCompletionFunctionCallChoice, ChatCompletionImageUrl,
    ChatCompletionInputFile, ChatCompletionMessageToolCall, ChatCompletionMessageToolCallFunction,
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestFunctionMessage,
    ChatCompletionRequestMessage, ChatCompletionRequestToolMessage,
    ChatCompletionRequestUserMessage, ChatCompletionTextContent, ChatCompletionTextContentPart,
    ChatCompletionToolChoiceMode, ChatCompletionToolChoiceOption, ChatCompletionToolDefinition,
    ChatCompletionUserContent, ChatCompletionUserContentPart, CustomToolDefinition, FunctionObject,
    ReasoningEffort, WebSearchOptions, WebSearchUserLocation,
};
use serde_json::Value as JsonValue;

/// Convert an OpenAI chat-completions request into a Claude create-message request.
pub fn transform_request(request: OpenAIChatCompletionRequest) -> ClaudeCreateMessageRequest {
    let mut system_texts = Vec::new();
    let mut messages = Vec::new();

    for message in &request.body.messages {
        match message {
            ChatCompletionRequestMessage::System(system) => {
                push_system_text(&mut system_texts, system.content.clone());
            }
            ChatCompletionRequestMessage::Developer(developer) => {
                push_system_text(&mut system_texts, developer.content.clone());
            }
            _ => {
                messages.extend(map_request_message(message));
            }
        }
    }

    let system = if system_texts.is_empty() {
        None
    } else {
        Some(ClaudeSystemParam::Text(system_texts.join("\n")))
    };

    let max_tokens = map_max_tokens(&request.body);
    // Claude OpenAI-compat: metadata/user are ignored.
    let metadata = None;

    let (tools, web_search_tool) = map_tools(request.body.tools, request.body.functions);
    let tools = merge_web_search_tool(tools, web_search_tool, request.body.web_search_options);

    let (tool_choice, disable_parallel_tool_use) = map_tool_choice(
        request.body.tool_choice,
        request.body.function_call,
        request.body.parallel_tool_calls,
    );

    let tool_choice = tool_choice.map(|choice| match choice {
        ClaudeToolChoice::Auto {
            disable_parallel_tool_use: _,
        }
        | ClaudeToolChoice::Any {
            disable_parallel_tool_use: _,
        }
        | ClaudeToolChoice::Tool {
            disable_parallel_tool_use: _,
            ..
        } => match disable_parallel_tool_use {
            Some(value) => match choice {
                ClaudeToolChoice::Auto { .. } => ClaudeToolChoice::Auto {
                    disable_parallel_tool_use: Some(value),
                },
                ClaudeToolChoice::Any { .. } => ClaudeToolChoice::Any {
                    disable_parallel_tool_use: Some(value),
                },
                ClaudeToolChoice::Tool { name, .. } => ClaudeToolChoice::Tool {
                    name,
                    disable_parallel_tool_use: Some(value),
                },
                ClaudeToolChoice::None => ClaudeToolChoice::None,
            },
            None => choice,
        },
        ClaudeToolChoice::None => ClaudeToolChoice::None,
    });

    // Claude OpenAI-compat: ignore reasoning_effort; use extra_body.thinking when provided.
    let extra_thinking = map_extra_body_thinking(request.body.extra_body.as_ref());
    let (mut thinking, mut output_config) = map_reasoning(None);
    if let Some(extra_thinking) = extra_thinking {
        thinking = Some(extra_thinking);
        output_config = None;
    }
    // Claude OpenAI-compat: response_format is ignored.
    let output_format = None;
    let stop_sequences = map_stop_sequences(request.body.stop.clone());

    ClaudeCreateMessageRequest {
        headers: ClaudeCreateMessageHeaders::default(),
        body: ClaudeCreateMessageRequestBody {
            max_tokens,
            messages,
            model: ClaudeModel::Custom(request.body.model.clone()),
            container: None,
            context_management: None,
            mcp_servers: None,
            metadata,
            output_config,
            output_format,
            service_tier: None,
            stop_sequences,
            stream: request.body.stream,
            system,
            temperature: map_temperature(request.body.temperature),
            thinking,
            tool_choice,
            tools,
            top_k: None,
            top_p: request.body.top_p,
        },
    }
}

fn map_request_message(message: &ChatCompletionRequestMessage) -> Vec<ClaudeMessageParam> {
    match message {
        ChatCompletionRequestMessage::User(user) => map_user_message(user),
        ChatCompletionRequestMessage::Assistant(assistant) => map_assistant_message(assistant),
        ChatCompletionRequestMessage::Tool(tool) => map_tool_message(tool),
        ChatCompletionRequestMessage::Function(function) => map_function_message(function),
        ChatCompletionRequestMessage::System(_) | ChatCompletionRequestMessage::Developer(_) => {
            Vec::new()
        }
    }
}

fn map_user_message(message: &ChatCompletionRequestUserMessage) -> Vec<ClaudeMessageParam> {
    let mut blocks = Vec::new();
    match &message.content {
        ChatCompletionUserContent::Text(text) => {
            push_text_block(&mut blocks, text.clone());
        }
        ChatCompletionUserContent::Parts(parts) => {
            for part in parts {
                match part {
                    ChatCompletionUserContentPart::Text { text } => {
                        push_text_block(&mut blocks, text.clone());
                    }
                    ChatCompletionUserContentPart::ImageUrl { image_url } => {
                        if let Some(block) = map_image_url(image_url) {
                            blocks.push(block);
                        }
                    }
                    ChatCompletionUserContentPart::InputAudio { input_audio } => {
                        push_text_block(
                            &mut blocks,
                            format!("[input_audio:{:?}]", input_audio.format),
                        );
                    }
                    ChatCompletionUserContentPart::File { file } => {
                        if let Some(block) = map_input_file(file) {
                            blocks.push(block);
                        }
                    }
                }
            }
        }
    }

    let content = if blocks.len() == 1 {
        match &blocks[0] {
            ClaudeContentBlockParam::Text(text) => ClaudeMessageContent::Text(text.text.clone()),
            _ => ClaudeMessageContent::Blocks(blocks),
        }
    } else {
        ClaudeMessageContent::Blocks(blocks)
    };

    vec![ClaudeMessageParam {
        role: ClaudeMessageRole::User,
        content,
    }]
}

fn map_assistant_message(
    message: &ChatCompletionRequestAssistantMessage,
) -> Vec<ClaudeMessageParam> {
    let mut blocks = Vec::new();

    if let Some(content) = &message.content {
        match content {
            ChatCompletionAssistantContent::Text(text) => {
                push_text_block(&mut blocks, text.clone());
            }
            ChatCompletionAssistantContent::Parts(parts) => {
                for part in parts {
                    match part {
                        gproxy_protocol::openai::create_chat_completions::types::ChatCompletionAssistantContentPart::Text { text } => {
                            push_text_block(&mut blocks, text.clone());
                        }
                        gproxy_protocol::openai::create_chat_completions::types::ChatCompletionAssistantContentPart::Refusal { refusal } => {
                            push_text_block(&mut blocks, refusal.clone());
                        }
                    }
                }
            }
        }
    }

    if let Some(refusal) = &message.refusal {
        push_text_block(&mut blocks, refusal.clone());
    }

    if let Some(tool_calls) = &message.tool_calls {
        for tool_call in tool_calls {
            blocks.push(map_tool_call(tool_call));
        }
    }

    if let Some(function_call) = &message.function_call {
        let tool_call = ChatCompletionMessageToolCall::Function {
            id: "function_call".to_string(),
            function: ChatCompletionMessageToolCallFunction {
                name: function_call.name.clone(),
                arguments: function_call.arguments.clone(),
            },
        };
        blocks.push(map_tool_call(&tool_call));
    }

    let content = if blocks.len() == 1 {
        match &blocks[0] {
            ClaudeContentBlockParam::Text(text) => ClaudeMessageContent::Text(text.text.clone()),
            _ => ClaudeMessageContent::Blocks(blocks),
        }
    } else {
        ClaudeMessageContent::Blocks(blocks)
    };

    vec![ClaudeMessageParam {
        role: ClaudeMessageRole::Assistant,
        content,
    }]
}

fn map_tool_message(message: &ChatCompletionRequestToolMessage) -> Vec<ClaudeMessageParam> {
    let content =
        ClaudeToolResultContent::Text(map_text_content_to_string(message.content.clone()));
    let block = ClaudeToolResultBlock {
        tool_use_id: message.tool_call_id.clone(),
        r#type: ClaudeToolResultBlockType::ToolResult,
        cache_control: None,
        content: Some(content),
        is_error: None,
    };

    vec![ClaudeMessageParam {
        role: ClaudeMessageRole::User,
        content: ClaudeMessageContent::Blocks(vec![ClaudeContentBlockParam::ToolResult(block)]),
    }]
}

fn map_function_message(message: &ChatCompletionRequestFunctionMessage) -> Vec<ClaudeMessageParam> {
    let text = format!(
        "[function:{}] {}",
        message.name,
        message.content.clone().unwrap_or_default()
    );
    vec![ClaudeMessageParam {
        role: ClaudeMessageRole::User,
        content: ClaudeMessageContent::Text(text),
    }]
}

fn map_text_content_to_string(content: ChatCompletionTextContent) -> String {
    match content {
        ChatCompletionTextContent::Text(text) => text,
        ChatCompletionTextContent::Parts(parts) => parts
            .into_iter()
            .map(|part| {
                let ChatCompletionTextContentPart::Text { text } = part;
                text
            })
            .collect::<Vec<String>>()
            .join("\n"),
    }
}

fn map_image_url(image: &ChatCompletionImageUrl) -> Option<ClaudeContentBlockParam> {
    if let Some((media_type, data)) = parse_data_url(&image.url)
        && let Some(media_type) = map_image_media_type(&media_type)
    {
        return Some(ClaudeContentBlockParam::Image(ClaudeImageBlockParam {
            source: ClaudeImageSource::Base64 { data, media_type },
            r#type: ClaudeImageBlockType::Image,
            cache_control: None,
        }));
    }
    // Unknown MIME type: fall back to URL with data URI.

    Some(ClaudeContentBlockParam::Image(ClaudeImageBlockParam {
        source: ClaudeImageSource::Url {
            url: image.url.clone(),
        },
        r#type: ClaudeImageBlockType::Image,
        cache_control: None,
    }))
}

fn map_input_file(file: &ChatCompletionInputFile) -> Option<ClaudeContentBlockParam> {
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

    if let Some(file_data) = &file.file_data {
        // OpenAI does not expose MIME type here; default to application/pdf.
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

fn map_tool_call(tool_call: &ChatCompletionMessageToolCall) -> ClaudeContentBlockParam {
    let (id, name, input) = match tool_call {
        ChatCompletionMessageToolCall::Function { id, function } => {
            let input = parse_tool_arguments(&function.arguments);
            (id.clone(), function.name.clone(), input)
        }
        ChatCompletionMessageToolCall::Custom { id, custom } => {
            let mut input = std::collections::BTreeMap::new();
            input.insert("input".to_string(), JsonValue::String(custom.input.clone()));
            (id.clone(), custom.name.clone(), input)
        }
    };

    ClaudeContentBlockParam::ToolUse(ClaudeToolUseBlock {
        id,
        input,
        name,
        r#type: ClaudeToolUseBlockType::ToolUse,
        cache_control: None,
        caller: None,
    })
}

fn parse_tool_arguments(arguments: &str) -> std::collections::BTreeMap<String, JsonValue> {
    match serde_json::from_str::<JsonValue>(arguments) {
        Ok(JsonValue::Object(map)) => map.into_iter().collect(),
        Ok(other) => {
            let mut map = std::collections::BTreeMap::new();
            map.insert("arguments".to_string(), other);
            map
        }
        Err(_) => {
            let mut map = std::collections::BTreeMap::new();
            map.insert(
                "arguments".to_string(),
                JsonValue::String(arguments.to_string()),
            );
            map
        }
    }
}

fn map_tools(
    tools: Option<Vec<ChatCompletionToolDefinition>>,
    functions: Option<
        Vec<gproxy_protocol::openai::create_chat_completions::types::ChatCompletionFunctions>,
    >,
) -> (Option<Vec<ClaudeTool>>, Option<ClaudeWebSearchTool>) {
    let mut output = Vec::new();
    let web_search = None;

    if let Some(tools) = tools {
        for tool in tools {
            match tool {
                ChatCompletionToolDefinition::Function { function } => {
                    output.push(ClaudeTool::Custom(map_function_tool(function)))
                }
                ChatCompletionToolDefinition::Custom { custom } => {
                    output.push(ClaudeTool::Custom(map_custom_tool(custom)))
                }
            }
        }
    }

    if let Some(functions) = functions {
        for function in functions {
            let function = FunctionObject {
                name: function.name,
                description: function.description,
                parameters: function.parameters,
                strict: None,
            };
            output.push(ClaudeTool::Custom(map_function_tool(function)));
        }
    }

    if output.is_empty() {
        (None, web_search)
    } else {
        (Some(output), web_search)
    }
}

fn merge_web_search_tool(
    tools: Option<Vec<ClaudeTool>>,
    mut web_search_tool: Option<ClaudeWebSearchTool>,
    web_search_options: Option<WebSearchOptions>,
) -> Option<Vec<ClaudeTool>> {
    if let Some(options) = web_search_options {
        web_search_tool = Some(ClaudeWebSearchTool {
            name: "web_search".to_string(),
            allowed_callers: None,
            allowed_domains: None,
            blocked_domains: None,
            cache_control: None,
            defer_loading: None,
            max_uses: None,
            strict: None,
            user_location: options.user_location.map(map_user_location),
        });
    }

    let mut tools = tools.unwrap_or_default();
    if let Some(tool) = web_search_tool {
        tools.push(ClaudeTool::Builtin(ClaudeToolBuiltin::WebSearch20250305(
            tool,
        )));
    }

    if tools.is_empty() { None } else { Some(tools) }
}

fn map_function_tool(function: FunctionObject) -> ClaudeToolCustom {
    let input_schema = function
        .parameters
        .as_ref()
        .and_then(|schema| serde_json::to_value(schema).ok())
        .and_then(parse_input_schema)
        .unwrap_or(ClaudeToolInputSchema {
            r#type: ClaudeToolInputSchemaType::Object,
            properties: None,
            required: None,
        });

    ClaudeToolCustom {
        input_schema,
        name: function.name,
        allowed_callers: None,
        cache_control: None,
        defer_loading: None,
        description: function.description,
        input_examples: None,
        // Claude OpenAI-compat: strict is ignored.
        strict: None,
        r#type: Some(gproxy_protocol::claude::count_tokens::types::BetaToolCustomType::Custom),
    }
}

fn map_custom_tool(tool: CustomToolDefinition) -> ClaudeToolCustom {
    ClaudeToolCustom {
        input_schema: ClaudeToolInputSchema {
            r#type: ClaudeToolInputSchemaType::Object,
            properties: None,
            required: None,
        },
        name: tool.name,
        allowed_callers: None,
        cache_control: None,
        defer_loading: None,
        description: tool.description,
        input_examples: None,
        strict: None,
        r#type: Some(gproxy_protocol::claude::count_tokens::types::BetaToolCustomType::Custom),
    }
}

fn parse_input_schema(schema: JsonValue) -> Option<ClaudeToolInputSchema> {
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

fn map_tool_choice(
    tool_choice: Option<ChatCompletionToolChoiceOption>,
    function_call: Option<ChatCompletionFunctionCallChoice>,
    parallel_tool_calls: Option<bool>,
) -> (Option<ClaudeToolChoice>, Option<bool>) {
    let disable_parallel = parallel_tool_calls.map(|value| !value);
    let (choice, disable_parallel_override) = match tool_choice {
        Some(ChatCompletionToolChoiceOption::Mode(mode)) => (
            Some(match mode {
                ChatCompletionToolChoiceMode::None => ClaudeToolChoice::None,
                ChatCompletionToolChoiceMode::Auto => ClaudeToolChoice::Auto {
                    disable_parallel_tool_use: None,
                },
                ChatCompletionToolChoiceMode::Required => ClaudeToolChoice::Any {
                    disable_parallel_tool_use: None,
                },
            }),
            None,
        ),
        Some(ChatCompletionToolChoiceOption::NamedTool(named)) => (
            Some(ClaudeToolChoice::Tool {
                name: named.function.name,
                disable_parallel_tool_use: None,
            }),
            None,
        ),
        Some(ChatCompletionToolChoiceOption::NamedCustomTool(named)) => (
            Some(ClaudeToolChoice::Tool {
                name: named.custom.name,
                disable_parallel_tool_use: None,
            }),
            None,
        ),
        Some(ChatCompletionToolChoiceOption::AllowedTools(choice)) => {
            let names = extract_allowed_tool_names(&choice);
            if names.len() == 1 {
                (
                    Some(ClaudeToolChoice::Tool {
                        name: names[0].clone(),
                        disable_parallel_tool_use: None,
                    }),
                    None,
                )
            } else {
                let mode = match choice.allowed_tools.mode {
                    AllowedToolsMode::Auto => ClaudeToolChoice::Auto {
                        disable_parallel_tool_use: None,
                    },
                    AllowedToolsMode::Required => ClaudeToolChoice::Any {
                        disable_parallel_tool_use: None,
                    },
                };
                (Some(mode), None)
            }
        }
        None => (None, None),
    };

    if choice.is_some() {
        return (choice, disable_parallel_override.or(disable_parallel));
    }

    let function_choice = match function_call {
        Some(ChatCompletionFunctionCallChoice::Mode(mode)) => match mode {
            gproxy_protocol::openai::create_chat_completions::types::ChatCompletionFunctionCallMode::None => {
                Some(ClaudeToolChoice::None)
            }
            gproxy_protocol::openai::create_chat_completions::types::ChatCompletionFunctionCallMode::Auto => {
                Some(ClaudeToolChoice::Auto {
                    disable_parallel_tool_use: None,
                })
            }
        },
        Some(ChatCompletionFunctionCallChoice::Named(named)) => Some(ClaudeToolChoice::Tool {
            name: named.name,
            disable_parallel_tool_use: None,
        }),
        None => None,
    };

    (function_choice, disable_parallel)
}

fn extract_allowed_tool_names(choice: &ChatCompletionAllowedToolsChoice) -> Vec<String> {
    let mut names = Vec::new();
    for tool in &choice.allowed_tools.tools {
        match tool {
            ChatCompletionAllowedTool::Function { function } => names.push(function.name.clone()),
            ChatCompletionAllowedTool::Custom { custom } => names.push(custom.name.clone()),
        }
    }
    names
}

fn map_reasoning(
    effort: Option<ReasoningEffort>,
) -> (
    Option<ClaudeThinkingConfigParam>,
    Option<ClaudeOutputConfig>,
) {
    let effort = match effort {
        Some(ReasoningEffort::None) => return (Some(ClaudeThinkingConfigParam::Disabled), None),
        Some(ReasoningEffort::Minimal) => ClaudeOutputEffort::Low,
        Some(ReasoningEffort::Low) => ClaudeOutputEffort::Low,
        Some(ReasoningEffort::Medium) => ClaudeOutputEffort::Medium,
        Some(ReasoningEffort::High) | Some(ReasoningEffort::XHigh) => ClaudeOutputEffort::High,
        None => return (None, None),
    };

    (
        Some(ClaudeThinkingConfigParam::Enabled {
            budget_tokens: 1024,
        }),
        Some(ClaudeOutputConfig {
            effort: Some(effort),
        }),
    )
}

fn map_extra_body_thinking(extra_body: Option<&JsonValue>) -> Option<ClaudeThinkingConfigParam> {
    let extra_body = extra_body?.as_object()?;
    let thinking = extra_body.get("thinking")?.as_object()?;
    let thinking_type = thinking.get("type")?.as_str()?;
    match thinking_type {
        "enabled" => {
            let budget = thinking
                .get("budget_tokens")
                .and_then(|value| value.as_u64())?;
            let budget_tokens = if budget > u32::MAX as u64 {
                u32::MAX
            } else {
                budget as u32
            };
            Some(ClaudeThinkingConfigParam::Enabled { budget_tokens })
        }
        "disabled" => Some(ClaudeThinkingConfigParam::Disabled),
        _ => None,
    }
}

fn map_stop_sequences(
    stop: Option<gproxy_protocol::openai::create_chat_completions::request::StopConfiguration>,
) -> Option<Vec<String>> {
    match stop {
        Some(
            gproxy_protocol::openai::create_chat_completions::request::StopConfiguration::Single(
                value,
            ),
        ) => {
            let value = value.trim();
            if value.is_empty() {
                None
            } else {
                Some(vec![value.to_string()])
            }
        }
        Some(
            gproxy_protocol::openai::create_chat_completions::request::StopConfiguration::Many(
                values,
            ),
        ) => {
            let values: Vec<String> = values
                .into_iter()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect();
            if values.is_empty() {
                None
            } else {
                Some(values)
            }
        }
        None => None,
    }
}

fn map_temperature(temperature: Option<f64>) -> Option<f64> {
    temperature.map(|value| value.clamp(0.0, 1.0))
}

fn map_max_tokens(
    body: &gproxy_protocol::openai::create_chat_completions::request::CreateChatCompletionRequestBody,
) -> u32 {
    let value = body.max_completion_tokens.or(body.max_tokens).unwrap_or(0);
    if value <= 0 {
        0
    } else if value > u32::MAX as i64 {
        u32::MAX
    } else {
        value as u32
    }
}

fn map_user_location(location: WebSearchUserLocation) -> ClaudeUserLocation {
    ClaudeUserLocation {
        r#type: gproxy_protocol::claude::count_tokens::types::BetaUserLocationType::Approximate,
        city: location.approximate.city,
        country: location.approximate.country,
        region: location.approximate.region,
        timezone: location.approximate.timezone,
    }
}

fn push_text_block(blocks: &mut Vec<ClaudeContentBlockParam>, text: String) {
    if !text.is_empty() {
        blocks.push(ClaudeContentBlockParam::Text(
            gproxy_protocol::claude::count_tokens::types::BetaTextBlockParam {
                text,
                r#type: gproxy_protocol::claude::count_tokens::types::BetaTextBlockType::Text,
                cache_control: None,
                citations: None,
            },
        ));
    }
}

fn push_system_text(system_texts: &mut Vec<String>, content: ChatCompletionTextContent) {
    match content {
        ChatCompletionTextContent::Text(text) => system_texts.push(text),
        ChatCompletionTextContent::Parts(parts) => {
            for part in parts {
                let ChatCompletionTextContentPart::Text { text } = part;
                system_texts.push(text);
            }
        }
    }
}

fn parse_data_url(url: &str) -> Option<(String, String)> {
    let url = url.strip_prefix("data:")?;
    let (meta, data) = url.split_once(",")?;
    let (mime, encoding) = meta.split_once(";")?;
    if encoding != "base64" {
        return None;
    }
    Some((mime.to_string(), data.to_string()))
}

fn map_image_media_type(mime: &str) -> Option<ClaudeImageMediaType> {
    match mime {
        "image/jpeg" => Some(ClaudeImageMediaType::ImageJpeg),
        "image/png" => Some(ClaudeImageMediaType::ImagePng),
        "image/gif" => Some(ClaudeImageMediaType::ImageGif),
        "image/webp" => Some(ClaudeImageMediaType::ImageWebp),
        _ => None,
    }
}
