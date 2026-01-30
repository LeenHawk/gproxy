use gproxy_protocol::claude::count_tokens::types::{
    BetaContentBlockParam as ClaudeContentBlockParam, BetaDocumentSource as ClaudeDocumentSource,
    BetaImageMediaType as ClaudeImageMediaType, BetaImageSource as ClaudeImageSource,
    BetaMCPToolResultContent as ClaudeMcpToolResultContent,
    BetaMCPToolUseBlockParam as ClaudeMcpToolUseBlock, BetaMessageContent as ClaudeMessageContent,
    BetaMessageParam as ClaudeMessageParam, BetaMessageRole as ClaudeMessageRole,
    BetaOutputConfig as ClaudeOutputConfig, BetaOutputEffort as ClaudeOutputEffort,
    BetaRequestDocumentBlock as ClaudeDocumentBlock,
    BetaRequestMCPToolResultBlockParam as ClaudeMcpToolResultBlock,
    BetaServerToolUseBlockParam as ClaudeServerToolUseBlock, BetaSystemParam as ClaudeSystemParam,
    BetaThinkingConfigParam as ClaudeThinkingConfigParam, BetaTool as ClaudeTool,
    BetaToolBuiltin as ClaudeToolBuiltin, BetaToolChoice as ClaudeToolChoice,
    BetaToolCustom as ClaudeToolCustom, BetaToolResultBlockParam as ClaudeToolResultBlock,
    BetaToolResultContent as ClaudeToolResultContent,
    BetaToolResultContentBlockParam as ClaudeToolResultContentBlock,
    BetaToolUseBlockParam as ClaudeToolUseBlock, BetaUserLocation as ClaudeUserLocation,
    BetaWebSearchTool as ClaudeWebSearchTool, Model as ClaudeModel,
};
use gproxy_protocol::claude::create_message::request::CreateMessageRequest as ClaudeCreateMessageRequest;
use gproxy_protocol::openai::create_chat_completions::request::{
    CreateChatCompletionRequest as OpenAIChatCompletionRequest,
    CreateChatCompletionRequestBody as OpenAIChatCompletionRequestBody, StopConfiguration,
};
use gproxy_protocol::openai::create_chat_completions::types::{
    ChatCompletionAssistantContent, ChatCompletionAssistantContentPart, ChatCompletionImageUrl,
    ChatCompletionInputFile, ChatCompletionMessageToolCall, ChatCompletionMessageToolCallFunction,
    ChatCompletionNamedToolChoice, ChatCompletionNamedToolChoiceFunction,
    ChatCompletionNamedToolChoiceType, ChatCompletionRequestAssistantMessage,
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestToolMessage, ChatCompletionRequestUserMessage, ChatCompletionTextContent,
    ChatCompletionToolChoiceMode, ChatCompletionToolChoiceOption, ChatCompletionToolDefinition,
    ChatCompletionUserContent, ChatCompletionUserContentPart, FunctionObject, JsonSchema,
    JsonSchemaType, JsonSchemaTypeValue, ReasoningEffort, WebSearchLocation, WebSearchOptions,
    WebSearchUserLocation, WebSearchUserLocationType,
};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

/// Convert a Claude create-message request into an OpenAI chat-completions request.
pub fn transform_request(request: ClaudeCreateMessageRequest) -> OpenAIChatCompletionRequest {
    let model = map_model(&request.body.model);

    let mut messages = Vec::new();
    if let Some(system) = map_system_message(request.body.system) {
        messages.push(system);
    }

    for message in &request.body.messages {
        messages.extend(map_message(message));
    }

    let (tools, web_search_options) = map_tools(request.body.tools);
    let (tool_choice, parallel_tool_calls) = map_tool_choice(request.body.tool_choice);
    let reasoning_effort = map_reasoning(request.body.thinking, request.body.output_config);
    let response_format = map_output_format(request.body.output_format);

    let stop = map_stop_sequences(request.body.stop_sequences);

    OpenAIChatCompletionRequest {
        body: OpenAIChatCompletionRequestBody {
            messages,
            model,
            modalities: None,
            verbosity: None,
            reasoning_effort,
            max_completion_tokens: Some(request.body.max_tokens as i64),
            frequency_penalty: None,
            presence_penalty: None,
            web_search_options,
            top_logprobs: None,
            response_format,
            audio: None,
            store: None,
            stream: request.body.stream,
            stop,
            logit_bias: None,
            logprobs: None,
            max_tokens: None,
            n: None,
            prediction: None,
            seed: None,
            stream_options: None,
            tools,
            tool_choice,
            parallel_tool_calls,
            function_call: None,
            functions: None,
            metadata: request.body.metadata.and_then(map_metadata),
            extra_body: None,
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

fn map_model(model: &ClaudeModel) -> String {
    match model {
        ClaudeModel::Custom(value) => value.clone(),
        ClaudeModel::Known(known) => match serde_json::to_value(known) {
            Ok(JsonValue::String(value)) => value,
            _ => "unknown".to_string(),
        },
    }
}

fn map_system_message(system: Option<ClaudeSystemParam>) -> Option<ChatCompletionRequestMessage> {
    let text = match system {
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
    }?;

    Some(ChatCompletionRequestMessage::System(
        ChatCompletionRequestSystemMessage {
            content: ChatCompletionTextContent::Text(text),
            name: None,
        },
    ))
}

fn map_message(message: &ClaudeMessageParam) -> Vec<ChatCompletionRequestMessage> {
    match message.role {
        ClaudeMessageRole::User => map_user_message(&message.content),
        ClaudeMessageRole::Assistant => map_assistant_message(&message.content),
    }
}

fn map_user_message(content: &ClaudeMessageContent) -> Vec<ChatCompletionRequestMessage> {
    let mut output = Vec::new();
    let mut user_parts: Vec<ChatCompletionUserContentPart> = Vec::new();

    match content {
        ClaudeMessageContent::Text(text) => {
            push_user_text(&mut user_parts, text.clone());
        }
        ClaudeMessageContent::Blocks(blocks) => {
            for block in blocks {
                match block {
                    ClaudeContentBlockParam::ToolResult(result) => {
                        flush_user_parts(&mut output, &mut user_parts);
                        if let Some(tool_message) = map_tool_result_message(result) {
                            output.push(tool_message);
                        }
                    }
                    ClaudeContentBlockParam::McpToolResult(result) => {
                        flush_user_parts(&mut output, &mut user_parts);
                        if let Some(tool_message) = map_mcp_tool_result_message(result) {
                            output.push(tool_message);
                        }
                    }
                    ClaudeContentBlockParam::Text(text) => {
                        push_user_text(&mut user_parts, text.text.clone());
                    }
                    ClaudeContentBlockParam::Image(image) => {
                        if let Some(part) = map_image_part(&image.source) {
                            user_parts.push(part);
                        } else if let Some(placeholder) = map_image_fallback(&image.source) {
                            push_user_text(&mut user_parts, placeholder);
                        }
                    }
                    ClaudeContentBlockParam::Document(doc) => {
                        if let Some(part) = map_document_part(doc) {
                            user_parts.push(part);
                        } else if let Some(placeholder) = map_document_fallback(doc) {
                            push_user_text(&mut user_parts, placeholder);
                        }
                    }
                    _ => {
                        if let Ok(text) = serde_json::to_string(block) {
                            push_user_text(&mut user_parts, text);
                        }
                    }
                }
            }
        }
    }

    flush_user_parts(&mut output, &mut user_parts);
    output
}

fn map_assistant_message(content: &ClaudeMessageContent) -> Vec<ChatCompletionRequestMessage> {
    let mut tool_calls = Vec::new();
    let mut parts = Vec::new();

    match content {
        ClaudeMessageContent::Text(text) => {
            push_assistant_text(&mut parts, text.clone());
        }
        ClaudeMessageContent::Blocks(blocks) => {
            for block in blocks {
                match block {
                    ClaudeContentBlockParam::Text(text) => {
                        push_assistant_text(&mut parts, text.text.clone());
                    }
                    ClaudeContentBlockParam::ToolUse(tool) => {
                        tool_calls.push(map_tool_use(tool));
                    }
                    ClaudeContentBlockParam::ServerToolUse(tool) => {
                        tool_calls.push(map_server_tool_use(tool));
                    }
                    ClaudeContentBlockParam::McpToolUse(tool) => {
                        tool_calls.push(map_mcp_tool_use(tool));
                    }
                    _ => {
                        if let Ok(text) = serde_json::to_string(block) {
                            push_assistant_text(&mut parts, text);
                        }
                    }
                }
            }
        }
    }

    let content = if parts.is_empty() {
        None
    } else if parts.len() == 1 {
        match &parts[0] {
            ChatCompletionAssistantContentPart::Text { text } => {
                Some(ChatCompletionAssistantContent::Text(text.clone()))
            }
            ChatCompletionAssistantContentPart::Refusal { refusal } => {
                Some(ChatCompletionAssistantContent::Parts(vec![
                    ChatCompletionAssistantContentPart::Refusal {
                        refusal: refusal.clone(),
                    },
                ]))
            }
        }
    } else {
        Some(ChatCompletionAssistantContent::Parts(parts))
    };

    let tool_calls = if tool_calls.is_empty() {
        None
    } else {
        Some(tool_calls)
    };

    vec![ChatCompletionRequestMessage::Assistant(
        ChatCompletionRequestAssistantMessage {
            content,
            refusal: None,
            name: None,
            audio: None,
            tool_calls,
            function_call: None,
        },
    )]
}

fn push_user_text(parts: &mut Vec<ChatCompletionUserContentPart>, text: String) {
    if !text.is_empty() {
        parts.push(ChatCompletionUserContentPart::Text { text });
    }
}

fn push_assistant_text(parts: &mut Vec<ChatCompletionAssistantContentPart>, text: String) {
    if !text.is_empty() {
        parts.push(ChatCompletionAssistantContentPart::Text { text });
    }
}

fn flush_user_parts(
    output: &mut Vec<ChatCompletionRequestMessage>,
    parts: &mut Vec<ChatCompletionUserContentPart>,
) {
    if parts.is_empty() {
        return;
    }

    let content = if parts.len() == 1 {
        match &parts[0] {
            ChatCompletionUserContentPart::Text { text } => {
                ChatCompletionUserContent::Text(text.clone())
            }
            _ => ChatCompletionUserContent::Parts(parts.clone()),
        }
    } else {
        ChatCompletionUserContent::Parts(parts.clone())
    };

    output.push(ChatCompletionRequestMessage::User(
        ChatCompletionRequestUserMessage {
            content,
            name: None,
        },
    ));
    parts.clear();
}

fn map_image_part(source: &ClaudeImageSource) -> Option<ChatCompletionUserContentPart> {
    let url = match source {
        ClaudeImageSource::Url { url } => url.clone(),
        ClaudeImageSource::Base64 { data, media_type } => {
            let mime = map_image_media_type(media_type);
            format!("data:{};base64,{}", mime, data)
        }
        ClaudeImageSource::File { .. } => return None,
    };

    Some(ChatCompletionUserContentPart::ImageUrl {
        image_url: ChatCompletionImageUrl { url, detail: None },
    })
}

fn map_image_fallback(source: &ClaudeImageSource) -> Option<String> {
    match source {
        ClaudeImageSource::File { file_id } => Some(format!("[image file_id: {}]", file_id)),
        _ => None,
    }
}

fn map_image_media_type(media_type: &ClaudeImageMediaType) -> &'static str {
    match media_type {
        ClaudeImageMediaType::ImageJpeg => "image/jpeg",
        ClaudeImageMediaType::ImagePng => "image/png",
        ClaudeImageMediaType::ImageGif => "image/gif",
        ClaudeImageMediaType::ImageWebp => "image/webp",
    }
}

fn map_document_part(doc: &ClaudeDocumentBlock) -> Option<ChatCompletionUserContentPart> {
    match &doc.source {
        ClaudeDocumentSource::Base64 { data, .. } => Some(ChatCompletionUserContentPart::File {
            file: ChatCompletionInputFile {
                filename: doc.title.clone(),
                file_data: Some(data.clone()),
                file_id: None,
            },
        }),
        ClaudeDocumentSource::File { file_id } => Some(ChatCompletionUserContentPart::File {
            file: ChatCompletionInputFile {
                filename: doc.title.clone(),
                file_data: None,
                file_id: Some(file_id.clone()),
            },
        }),
        _ => None,
    }
}

fn map_document_fallback(doc: &ClaudeDocumentBlock) -> Option<String> {
    match &doc.source {
        ClaudeDocumentSource::Url { url } => Some(format!("[document url: {}]", url)),
        ClaudeDocumentSource::Text { data, .. } => Some(data.clone()),
        ClaudeDocumentSource::Content { content } => match content {
            gproxy_protocol::claude::count_tokens::types::BetaContentBlockSourceContent::Text(
                text,
            ) => Some(text.clone()),
            _ => serde_json::to_string(content).ok(),
        },
        _ => None,
    }
}

fn map_tool_result_message(result: &ClaudeToolResultBlock) -> Option<ChatCompletionRequestMessage> {
    let content = map_tool_result_content(result.content.as_ref())?;
    Some(ChatCompletionRequestMessage::Tool(
        ChatCompletionRequestToolMessage {
            content,
            tool_call_id: result.tool_use_id.clone(),
        },
    ))
}

fn map_mcp_tool_result_message(
    result: &ClaudeMcpToolResultBlock,
) -> Option<ChatCompletionRequestMessage> {
    let content = map_mcp_tool_result_content(result.content.as_ref())?;
    Some(ChatCompletionRequestMessage::Tool(
        ChatCompletionRequestToolMessage {
            content,
            tool_call_id: result.tool_use_id.clone(),
        },
    ))
}

fn map_tool_result_content(
    content: Option<&ClaudeToolResultContent>,
) -> Option<ChatCompletionTextContent> {
    let text = match content {
        Some(ClaudeToolResultContent::Text(text)) => text.clone(),
        Some(ClaudeToolResultContent::Blocks(blocks)) => blocks
            .iter()
            .filter_map(map_tool_result_block_text)
            .collect::<Vec<String>>()
            .join("\n"),
        None => String::new(),
    };

    if text.is_empty() {
        None
    } else {
        Some(ChatCompletionTextContent::Text(text))
    }
}

fn map_tool_result_block_text(block: &ClaudeToolResultContentBlock) -> Option<String> {
    match block {
        ClaudeToolResultContentBlock::Text(text) => Some(text.text.clone()),
        ClaudeToolResultContentBlock::Image(_) => Some("[tool_result image]".to_string()),
        ClaudeToolResultContentBlock::Document(_) => Some("[tool_result document]".to_string()),
        ClaudeToolResultContentBlock::SearchResult(result) => Some(result.title.clone()),
        ClaudeToolResultContentBlock::ToolReference(tool) => Some(tool.tool_name.clone()),
    }
}

fn map_mcp_tool_result_content(
    content: Option<&ClaudeMcpToolResultContent>,
) -> Option<ChatCompletionTextContent> {
    let text = match content {
        Some(ClaudeMcpToolResultContent::Text(text)) => text.clone(),
        Some(ClaudeMcpToolResultContent::Blocks(blocks)) => {
            let texts: Vec<String> = blocks.iter().map(|block| block.text.clone()).collect();
            texts.join("\n")
        }
        None => String::new(),
    };

    if text.is_empty() {
        None
    } else {
        Some(ChatCompletionTextContent::Text(text))
    }
}

fn map_tool_use(tool: &ClaudeToolUseBlock) -> ChatCompletionMessageToolCall {
    let arguments = serde_json::to_string(&tool.input).unwrap_or_else(|_| "{}".to_string());
    ChatCompletionMessageToolCall::Function {
        id: tool.id.clone(),
        function: ChatCompletionMessageToolCallFunction {
            name: tool.name.clone(),
            arguments,
        },
    }
}

fn map_server_tool_use(tool: &ClaudeServerToolUseBlock) -> ChatCompletionMessageToolCall {
    let arguments = serde_json::to_string(&tool.input).unwrap_or_else(|_| "{}".to_string());
    ChatCompletionMessageToolCall::Function {
        id: tool.id.clone(),
        function: ChatCompletionMessageToolCallFunction {
            name: format!("{:?}", tool.name),
            arguments,
        },
    }
}

fn map_mcp_tool_use(tool: &ClaudeMcpToolUseBlock) -> ChatCompletionMessageToolCall {
    let arguments = serde_json::to_string(&tool.input).unwrap_or_else(|_| "{}".to_string());
    ChatCompletionMessageToolCall::Function {
        id: tool.id.clone(),
        function: ChatCompletionMessageToolCallFunction {
            name: tool.name.clone(),
            arguments,
        },
    }
}

fn map_tools(
    tools: Option<Vec<ClaudeTool>>,
) -> (
    Option<Vec<ChatCompletionToolDefinition>>,
    Option<WebSearchOptions>,
) {
    let mut definitions = Vec::new();
    let mut web_search_options = None;

    if let Some(tools) = tools {
        for tool in tools {
            match tool {
                ClaudeTool::Custom(custom) => {
                    definitions.push(ChatCompletionToolDefinition::Function {
                        function: map_custom_tool(custom),
                    })
                }
                ClaudeTool::Builtin(ClaudeToolBuiltin::WebSearch20250305(tool)) => {
                    web_search_options = Some(map_web_search_options(tool));
                }
                _ => {
                    // Claude built-ins (except web_search) have no OpenAI Chat Completions equivalent.
                }
            }
        }
    }

    let definitions = if definitions.is_empty() {
        None
    } else {
        Some(definitions)
    };
    (definitions, web_search_options)
}

fn map_custom_tool(tool: ClaudeToolCustom) -> FunctionObject {
    FunctionObject {
        name: tool.name,
        description: tool.description,
        // Claude tool schemas are raw JSON; Chat Completions expects a typed schema.
        // Preserve only the object type to avoid invalid schema conversions.
        parameters: Some(minimal_object_schema()),
        strict: tool.strict,
    }
}

fn minimal_object_schema() -> JsonSchema {
    JsonSchema {
        r#type: Some(JsonSchemaType::Single(JsonSchemaTypeValue::Object)),
        format: None,
        title: None,
        description: None,
        nullable: None,
        enum_values: None,
        properties: None,
        required: None,
        items: None,
        any_of: None,
        one_of: None,
        all_of: None,
        min_items: None,
        max_items: None,
        min_length: None,
        max_length: None,
        minimum: None,
        maximum: None,
        pattern: None,
        default: None,
        example: None,
        property_ordering: None,
        additional_properties: None,
    }
}

fn map_web_search_options(tool: ClaudeWebSearchTool) -> WebSearchOptions {
    WebSearchOptions {
        user_location: tool.user_location.map(map_user_location),
        search_context_size: None,
    }
}

fn map_user_location(location: ClaudeUserLocation) -> WebSearchUserLocation {
    WebSearchUserLocation {
        r#type: WebSearchUserLocationType::Approximate,
        approximate: WebSearchLocation {
            country: location.country,
            region: location.region,
            city: location.city,
            timezone: location.timezone,
        },
    }
}

fn map_tool_choice(
    choice: Option<ClaudeToolChoice>,
) -> (Option<ChatCompletionToolChoiceOption>, Option<bool>) {
    let choice = match choice {
        Some(choice) => choice,
        None => return (None, None),
    };

    match choice {
        ClaudeToolChoice::Auto {
            disable_parallel_tool_use,
        } => (
            Some(ChatCompletionToolChoiceOption::Mode(
                ChatCompletionToolChoiceMode::Auto,
            )),
            disable_parallel_tool_use.map(|disabled| !disabled),
        ),
        ClaudeToolChoice::Any {
            disable_parallel_tool_use,
        } => (
            Some(ChatCompletionToolChoiceOption::Mode(
                ChatCompletionToolChoiceMode::Required,
            )),
            disable_parallel_tool_use.map(|disabled| !disabled),
        ),
        ClaudeToolChoice::Tool {
            name,
            disable_parallel_tool_use,
        } => (
            Some(ChatCompletionToolChoiceOption::NamedTool(
                ChatCompletionNamedToolChoice {
                    r#type: ChatCompletionNamedToolChoiceType::Function,
                    function: ChatCompletionNamedToolChoiceFunction { name },
                },
            )),
            disable_parallel_tool_use.map(|disabled| !disabled),
        ),
        ClaudeToolChoice::None => (
            Some(ChatCompletionToolChoiceOption::Mode(
                ChatCompletionToolChoiceMode::None,
            )),
            None,
        ),
    }
}

fn map_reasoning(
    thinking: Option<ClaudeThinkingConfigParam>,
    output_config: Option<ClaudeOutputConfig>,
) -> Option<ReasoningEffort> {
    let effort = output_config.and_then(|config| config.effort);
    let thinking_enabled = matches!(thinking, Some(ClaudeThinkingConfigParam::Enabled { .. }));

    if !thinking_enabled {
        return Some(ReasoningEffort::Medium);
    }

    match effort {
        Some(ClaudeOutputEffort::Low) => Some(ReasoningEffort::Low),
        Some(ClaudeOutputEffort::Medium) => Some(ReasoningEffort::Medium),
        Some(ClaudeOutputEffort::High) => Some(ReasoningEffort::High),
        None => Some(ReasoningEffort::Medium),
    }
}

fn map_output_format(
    output_format: Option<gproxy_protocol::claude::count_tokens::types::BetaJSONOutputFormat>,
) -> Option<gproxy_protocol::openai::create_chat_completions::types::ChatCompletionResponseFormat> {
    if output_format.is_some() {
        // Claude JSON schema is raw JSON; we can't reliably map it to the typed schema here.
        return Some(
            gproxy_protocol::openai::create_chat_completions::types::ChatCompletionResponseFormat::JsonObject,
        );
    }
    None
}

fn map_stop_sequences(stop_sequences: Option<Vec<String>>) -> Option<StopConfiguration> {
    let sequences = stop_sequences?;
    if sequences.is_empty() {
        None
    } else if sequences.len() == 1 {
        Some(StopConfiguration::Single(sequences[0].clone()))
    } else {
        Some(StopConfiguration::Many(sequences))
    }
}

fn map_metadata(
    metadata: gproxy_protocol::claude::create_message::types::BetaMetadata,
) -> Option<BTreeMap<String, String>> {
    let mut map = BTreeMap::new();
    if let Some(user_id) = metadata.user_id {
        map.insert("user_id".to_string(), user_id);
    }
    if map.is_empty() { None } else { Some(map) }
}
