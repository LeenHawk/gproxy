use gproxy_protocol::gemini::count_tokens::types::{
    Blob as GeminiBlob, Content as GeminiContent, ContentRole as GeminiContentRole,
    FileData as GeminiFileData, FunctionResponse as GeminiFunctionResponse, Part as GeminiPart,
};
use gproxy_protocol::gemini::generate_content::request::GenerateContentRequest as GeminiGenerateContentRequest;
use gproxy_protocol::gemini::generate_content::types::{
    FunctionCallingMode, FunctionDeclaration, GenerationConfig, ThinkingLevel, Tool as GeminiTool,
    ToolConfig,
};
use gproxy_protocol::openai::create_chat_completions::request::{
    CreateChatCompletionRequest, CreateChatCompletionRequestBody, StopConfiguration,
};
use gproxy_protocol::openai::create_chat_completions::types::{
    AllowedToolsMode, ChatCompletionAllowedTool, ChatCompletionAllowedToolFunction,
    ChatCompletionAllowedTools, ChatCompletionAllowedToolsChoice,
    ChatCompletionAllowedToolsChoiceType, ChatCompletionAssistantContent,
    ChatCompletionAssistantContentPart, ChatCompletionImageUrl, ChatCompletionInputFile,
    ChatCompletionMessageToolCall, ChatCompletionMessageToolCallFunction,
    ChatCompletionRequestAssistantMessage, ChatCompletionRequestMessage,
    ChatCompletionRequestSystemMessage, ChatCompletionRequestToolMessage,
    ChatCompletionRequestUserMessage, ChatCompletionResponseFormat, ChatCompletionTextContent,
    ChatCompletionToolChoiceMode, ChatCompletionToolChoiceOption, ChatCompletionToolDefinition,
    ChatCompletionUserContent, ChatCompletionUserContentPart, FunctionObject, ReasoningEffort,
    ResponseFormatJsonSchema, ResponseModality, WebSearchOptions,
};
use serde::Serialize;

/// Convert a Gemini generate-content request into an OpenAI chat-completions request.
pub fn transform_request(request: GeminiGenerateContentRequest) -> CreateChatCompletionRequest {
    let model = request
        .path
        .model
        .strip_prefix("models/")
        .unwrap_or(&request.path.model)
        .to_string();

    let mut messages = Vec::new();
    let mut tool_call_index = 0usize;

    if let Some(system_instruction) = request.body.system_instruction
        && let Some(message) = map_system_instruction(system_instruction)
    {
        messages.push(message);
    }

    for content in request.body.contents {
        messages.extend(map_content_to_messages(content, &mut tool_call_index));
    }

    let tools_input = request.body.tools;
    let web_search_options = map_web_search_options(tools_input.as_ref());
    let tools = map_tools(tools_input);
    let tool_choice = map_tool_choice(request.body.tool_config.as_ref());

    let generation_config = request.body.generation_config.as_ref();
    let response_format = map_response_format(generation_config);
    let stop = map_stop_sequences(generation_config);
    let reasoning_effort = map_reasoning_effort(generation_config);
    let modalities = map_modalities(generation_config);

    CreateChatCompletionRequest {
        body: CreateChatCompletionRequestBody {
            messages,
            model,
            modalities,
            verbosity: None,
            reasoning_effort,
            max_completion_tokens: generation_config
                .and_then(|config| config.max_output_tokens.map(|v| v as i64)),
            frequency_penalty: None,
            presence_penalty: None,
            web_search_options,
            top_logprobs: None,
            response_format,
            audio: None,
            store: None,
            stream: None,
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
            parallel_tool_calls: None,
            function_call: None,
            functions: None,
            metadata: None,
            extra_body: None,
            temperature: generation_config.and_then(|config| config.temperature),
            top_p: generation_config.and_then(|config| config.top_p),
            user: None,
            safety_identifier: None,
            prompt_cache_key: None,
            service_tier: None,
            prompt_cache_retention: None,
        },
    }
}

fn map_system_instruction(system: GeminiContent) -> Option<ChatCompletionRequestMessage> {
    let texts: Vec<String> = system
        .parts
        .iter()
        .filter_map(|part| part.text.clone())
        .collect();

    if texts.is_empty() {
        None
    } else {
        Some(ChatCompletionRequestMessage::System(
            ChatCompletionRequestSystemMessage {
                content: ChatCompletionTextContent::Text(texts.join("\n")),
                name: None,
            },
        ))
    }
}

fn map_content_to_messages(
    content: GeminiContent,
    tool_call_index: &mut usize,
) -> Vec<ChatCompletionRequestMessage> {
    match content.role {
        Some(GeminiContentRole::Model) => map_model_content_to_messages(content, tool_call_index),
        _ => map_user_content_to_messages(content, tool_call_index),
    }
}

fn map_user_content_to_messages(
    content: GeminiContent,
    tool_call_index: &mut usize,
) -> Vec<ChatCompletionRequestMessage> {
    let mut messages = Vec::new();
    let (user_content, tool_responses) = map_parts_for_user(&content.parts, tool_call_index);

    if let Some(user_content) = user_content {
        messages.push(ChatCompletionRequestMessage::User(
            ChatCompletionRequestUserMessage {
                content: user_content,
                name: None,
            },
        ));
    }

    for tool_response in tool_responses {
        messages.push(ChatCompletionRequestMessage::Tool(tool_response));
    }

    messages
}

fn map_model_content_to_messages(
    content: GeminiContent,
    tool_call_index: &mut usize,
) -> Vec<ChatCompletionRequestMessage> {
    let mut messages = Vec::new();
    let (assistant_message, tool_calls) = map_parts_for_assistant(&content.parts, tool_call_index);

    if assistant_message.is_some() || !tool_calls.is_empty() {
        messages.push(ChatCompletionRequestMessage::Assistant(
            ChatCompletionRequestAssistantMessage {
                content: assistant_message,
                refusal: None,
                name: None,
                audio: None,
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                },
                function_call: None,
            },
        ));
    }

    messages
}

fn map_parts_for_user(
    parts: &[GeminiPart],
    tool_call_index: &mut usize,
) -> (
    Option<ChatCompletionUserContent>,
    Vec<ChatCompletionRequestToolMessage>,
) {
    let mut user_parts = Vec::new();
    let mut tool_responses = Vec::new();

    for part in parts {
        if let Some(text) = part.text.clone()
            && !text.is_empty()
        {
            user_parts.push(ChatCompletionUserContentPart::Text { text });
        }

        if let Some(blob) = &part.inline_data
            && let Some(part) = map_inline_blob_to_user_part(blob)
        {
            user_parts.push(part);
        }

        if let Some(file) = &part.file_data
            && let Some(part) = map_file_data_to_user_part(file)
        {
            user_parts.push(part);
        }

        if let Some(response) = &part.function_response
            && let Some(tool_message) =
                map_function_response_to_tool_message(response, tool_call_index)
        {
            tool_responses.push(tool_message);
        }

        if let Some(function_call) = &part.function_call {
            // Gemini function calls in user content have no direct Chat Completions input equivalent.
            push_user_json_text(&mut user_parts, "function_call", function_call);
        }

        if let Some(code) = &part.executable_code {
            push_user_json_text(&mut user_parts, "executable_code", code);
        }

        if let Some(result) = &part.code_execution_result {
            push_user_json_text(&mut user_parts, "code_execution_result", result);
        }
    }

    let user_content = if user_parts.is_empty() {
        None
    } else if user_parts.len() == 1 {
        match &user_parts[0] {
            ChatCompletionUserContentPart::Text { text } => {
                Some(ChatCompletionUserContent::Text(text.clone()))
            }
            _ => Some(ChatCompletionUserContent::Parts(user_parts)),
        }
    } else {
        Some(ChatCompletionUserContent::Parts(user_parts))
    };

    (user_content, tool_responses)
}

fn map_parts_for_assistant(
    parts: &[GeminiPart],
    tool_call_index: &mut usize,
) -> (
    Option<ChatCompletionAssistantContent>,
    Vec<ChatCompletionMessageToolCall>,
) {
    let mut texts = Vec::new();
    let mut tool_calls = Vec::new();

    for part in parts {
        if let Some(text) = part.text.clone()
            && !text.is_empty()
        {
            texts.push(ChatCompletionAssistantContentPart::Text { text });
        }

        if let Some(function_call) = &part.function_call {
            let id = function_call
                .id
                .clone()
                .unwrap_or_else(|| next_tool_call_id(tool_call_index));
            let arguments = function_call
                .args
                .as_ref()
                .and_then(|value| serde_json::to_string(value).ok())
                .unwrap_or_else(|| "{}".to_string());
            tool_calls.push(ChatCompletionMessageToolCall::Function {
                id,
                function: ChatCompletionMessageToolCallFunction {
                    name: function_call.name.clone(),
                    arguments,
                },
            });
        }

        if let Some(function_response) = &part.function_response {
            // Assistant-side tool responses are not representable; serialize to text.
            push_assistant_json_text(&mut texts, "function_response", function_response);
        }

        if let Some(code) = &part.executable_code {
            push_assistant_json_text(&mut texts, "executable_code", code);
        }

        if let Some(result) = &part.code_execution_result {
            push_assistant_json_text(&mut texts, "code_execution_result", result);
        }

        if part.inline_data.is_some() {
            push_text_part(&mut texts, "[inline_data]".to_string());
        }

        if let Some(file) = &part.file_data {
            push_text_part(&mut texts, format!("[file:{}]", file.file_uri));
        }
    }

    let content = if texts.is_empty() {
        None
    } else if texts.len() == 1 {
        match &texts[0] {
            ChatCompletionAssistantContentPart::Text { text } => {
                Some(ChatCompletionAssistantContent::Text(text.clone()))
            }
            ChatCompletionAssistantContentPart::Refusal { refusal } => {
                Some(ChatCompletionAssistantContent::Text(refusal.clone()))
            }
        }
    } else {
        Some(ChatCompletionAssistantContent::Parts(texts))
    };

    (content, tool_calls)
}

fn map_inline_blob_to_user_part(blob: &GeminiBlob) -> Option<ChatCompletionUserContentPart> {
    if blob.mime_type.starts_with("image/") {
        let url = format!("data:{};base64,{}", blob.mime_type, blob.data);
        return Some(ChatCompletionUserContentPart::ImageUrl {
            image_url: ChatCompletionImageUrl { url, detail: None },
        });
    }

    Some(ChatCompletionUserContentPart::File {
        file: ChatCompletionInputFile {
            filename: None,
            file_data: Some(blob.data.clone()),
            file_id: None,
        },
    })
}

fn map_file_data_to_user_part(file: &GeminiFileData) -> Option<ChatCompletionUserContentPart> {
    if let Some(mime_type) = &file.mime_type
        && mime_type.starts_with("image/")
    {
        return Some(ChatCompletionUserContentPart::ImageUrl {
            image_url: ChatCompletionImageUrl {
                url: file.file_uri.clone(),
                detail: None,
            },
        });
    }

    Some(ChatCompletionUserContentPart::Text {
        text: format!("[file:{}]", file.file_uri),
    })
}

fn map_function_response_to_tool_message(
    response: &GeminiFunctionResponse,
    tool_call_index: &mut usize,
) -> Option<ChatCompletionRequestToolMessage> {
    let tool_call_id = response
        .id
        .clone()
        .unwrap_or_else(|| next_tool_call_id(tool_call_index));
    let response_text = serde_json::to_string(response).unwrap_or_default();
    if response_text.is_empty() {
        return None;
    }

    Some(ChatCompletionRequestToolMessage {
        content: ChatCompletionTextContent::Text(response_text),
        tool_call_id,
    })
}

fn push_user_json_text<T: Serialize>(
    parts: &mut Vec<ChatCompletionUserContentPart>,
    label: &str,
    value: &T,
) {
    if let Ok(json) = serde_json::to_string(value) {
        parts.push(ChatCompletionUserContentPart::Text {
            text: format!("[{}] {}", label, json),
        });
    }
}

fn push_text_part(parts: &mut Vec<ChatCompletionAssistantContentPart>, text: String) {
    if !text.is_empty() {
        parts.push(ChatCompletionAssistantContentPart::Text { text });
    }
}

fn push_assistant_json_text<T: Serialize>(
    parts: &mut Vec<ChatCompletionAssistantContentPart>,
    label: &str,
    value: &T,
) {
    if let Ok(json) = serde_json::to_string(value) {
        parts.push(ChatCompletionAssistantContentPart::Text {
            text: format!("[{}] {}", label, json),
        });
    }
}

fn next_tool_call_id(counter: &mut usize) -> String {
    let id = format!("tool_call_{}", counter);
    *counter += 1;
    id
}

fn map_tools(tools: Option<Vec<GeminiTool>>) -> Option<Vec<ChatCompletionToolDefinition>> {
    let tools = tools?;

    let mut output = Vec::new();
    for tool in tools {
        if let Some(functions) = tool.function_declarations {
            for function in functions {
                output.push(ChatCompletionToolDefinition::Function {
                    function: map_function_tool(function),
                });
            }
        }
        // Gemini built-in tools (search, code execution, file search, computer use) don't map to chat-completions.
    }

    if output.is_empty() {
        None
    } else {
        Some(output)
    }
}

fn map_function_tool(function: FunctionDeclaration) -> FunctionObject {
    let parameters = function
        .parameters_json_schema
        .or_else(|| serde_json::to_value(&function.parameters).ok())
        .and_then(|value| serde_json::from_value(value).ok());

    FunctionObject {
        name: function.name,
        description: Some(function.description),
        parameters,
        strict: None,
    }
}

fn map_tool_choice(tool_config: Option<&ToolConfig>) -> Option<ChatCompletionToolChoiceOption> {
    let config = tool_config.and_then(|config| config.function_calling_config.as_ref())?;

    let mode = config.mode.unwrap_or(FunctionCallingMode::ModeUnspecified);
    let allowed = config.allowed_function_names.clone().unwrap_or_default();

    match mode {
        FunctionCallingMode::None => Some(ChatCompletionToolChoiceOption::Mode(
            ChatCompletionToolChoiceMode::None,
        )),
        FunctionCallingMode::Auto => {
            if allowed.is_empty() {
                Some(ChatCompletionToolChoiceOption::Mode(
                    ChatCompletionToolChoiceMode::Auto,
                ))
            } else {
                Some(ChatCompletionToolChoiceOption::AllowedTools(
                    ChatCompletionAllowedToolsChoice {
                        r#type: ChatCompletionAllowedToolsChoiceType::AllowedTools,
                        allowed_tools: ChatCompletionAllowedTools {
                            mode: AllowedToolsMode::Auto,
                            tools: allowed
                                .into_iter()
                                .map(|name| ChatCompletionAllowedTool::Function {
                                    function: ChatCompletionAllowedToolFunction { name },
                                })
                                .collect(),
                        },
                    },
                ))
            }
        }
        FunctionCallingMode::Any | FunctionCallingMode::Validated => {
            if allowed.is_empty() {
                Some(ChatCompletionToolChoiceOption::Mode(
                    ChatCompletionToolChoiceMode::Required,
                ))
            } else {
                Some(ChatCompletionToolChoiceOption::AllowedTools(
                    ChatCompletionAllowedToolsChoice {
                        r#type: ChatCompletionAllowedToolsChoiceType::AllowedTools,
                        allowed_tools: ChatCompletionAllowedTools {
                            mode: AllowedToolsMode::Required,
                            tools: allowed
                                .into_iter()
                                .map(|name| ChatCompletionAllowedTool::Function {
                                    function: ChatCompletionAllowedToolFunction { name },
                                })
                                .collect(),
                        },
                    },
                ))
            }
        }
        FunctionCallingMode::ModeUnspecified => None,
    }
}

fn map_response_format(config: Option<&GenerationConfig>) -> Option<ChatCompletionResponseFormat> {
    let config = config?;

    let schema = config
        .response_json_schema
        .clone()
        .or_else(|| config.response_json_schema_internal.clone());

    if let Some(schema) = schema {
        let parsed_schema = serde_json::from_value(schema).ok();
        return Some(ChatCompletionResponseFormat::JsonSchema {
            json_schema: ResponseFormatJsonSchema {
                name: "response".to_string(),
                description: None,
                schema: parsed_schema,
                strict: None,
            },
        });
    }

    if config.response_mime_type.as_deref() == Some("application/json") {
        return Some(ChatCompletionResponseFormat::JsonObject);
    }

    None
}

fn map_stop_sequences(config: Option<&GenerationConfig>) -> Option<StopConfiguration> {
    let sequences = config.and_then(|config| config.stop_sequences.clone())?;
    if sequences.is_empty() {
        None
    } else if sequences.len() == 1 {
        Some(StopConfiguration::Single(sequences[0].clone()))
    } else {
        Some(StopConfiguration::Many(sequences))
    }
}

fn map_reasoning_effort(config: Option<&GenerationConfig>) -> Option<ReasoningEffort> {
    let thinking = config.and_then(|config| config.thinking_config.as_ref())?;
    if !thinking.include_thoughts {
        return Some(ReasoningEffort::None);
    }

    Some(match thinking.thinking_level {
        Some(ThinkingLevel::Minimal) => ReasoningEffort::Minimal,
        Some(ThinkingLevel::Low) => ReasoningEffort::Low,
        Some(ThinkingLevel::Medium) => ReasoningEffort::Medium,
        Some(ThinkingLevel::High) => ReasoningEffort::High,
        _ => ReasoningEffort::Low,
    })
}

fn map_modalities(config: Option<&GenerationConfig>) -> Option<Vec<ResponseModality>> {
    let modalities = config.and_then(|config| config.response_modalities.as_ref())?;
    let mut output = Vec::new();
    for modality in modalities {
        match modality {
            gproxy_protocol::gemini::count_tokens::types::Modality::Audio => {
                output.push(ResponseModality::Audio)
            }
            gproxy_protocol::gemini::count_tokens::types::Modality::Text => {
                output.push(ResponseModality::Text)
            }
            _ => {}
        }
    }
    if output.is_empty() {
        None
    } else {
        Some(output)
    }
}

fn map_web_search_options(tools: Option<&Vec<GeminiTool>>) -> Option<WebSearchOptions> {
    let tools = tools?;
    for tool in tools {
        if tool.google_search.is_some() || tool.google_search_retrieval.is_some() {
            return Some(WebSearchOptions {
                user_location: None,
                search_context_size: None,
            });
        }
    }
    None
}
