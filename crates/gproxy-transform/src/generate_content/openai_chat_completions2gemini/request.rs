use gproxy_protocol::gemini::count_tokens::types::{
    Blob as GeminiBlob, Content as GeminiContent, ContentRole as GeminiContentRole,
    FileData as GeminiFileData, FunctionCall as GeminiFunctionCall,
    FunctionResponse as GeminiFunctionResponse, Part as GeminiPart,
};
use gproxy_protocol::gemini::generate_content::request::{
    GenerateContentPath as GeminiGenerateContentPath,
    GenerateContentRequest as GeminiGenerateContentRequest,
    GenerateContentRequestBody as GeminiGenerateContentRequestBody,
};
use gproxy_protocol::gemini::generate_content::types::{
    FunctionCallingConfig, FunctionCallingMode, FunctionDeclaration, GenerationConfig,
    GoogleSearch, ThinkingConfig, ThinkingLevel, Tool as GeminiTool, ToolConfig,
};
use gproxy_protocol::openai::create_chat_completions::request::CreateChatCompletionRequest;
use gproxy_protocol::openai::create_chat_completions::types::{
    AllowedToolsMode, ChatCompletionAllowedTool, ChatCompletionAllowedToolsChoice,
    ChatCompletionAssistantContent, ChatCompletionAssistantContentPart,
    ChatCompletionFunctionCallChoice, ChatCompletionFunctionCallMode,
    ChatCompletionFunctionCallOption, ChatCompletionImageDetail, ChatCompletionInputAudioFormat,
    ChatCompletionInputFile, ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessage,
    ChatCompletionRequestFunctionMessage, ChatCompletionRequestMessage,
    ChatCompletionRequestToolMessage, ChatCompletionRequestUserMessage,
    ChatCompletionResponseFormat, ChatCompletionTextContent, ChatCompletionTextContentPart,
    ChatCompletionToolChoiceMode, ChatCompletionToolChoiceOption, ChatCompletionToolDefinition,
    ChatCompletionUserContent, ChatCompletionUserContentPart, FunctionObject, ReasoningEffort,
    ResponseModality,
};
use serde_json::Value as JsonValue;

/// Convert an OpenAI chat-completions request into a Gemini generate-content request.
pub fn transform_request(request: CreateChatCompletionRequest) -> GeminiGenerateContentRequest {
    let model = request.body.model.clone();

    let mut system_texts = Vec::new();
    let mut contents = Vec::new();
    let mut tool_call_index = 0usize;

    for message in request.body.messages {
        match message {
            ChatCompletionRequestMessage::System(system) => {
                push_system_text(&mut system_texts, system.content);
            }
            ChatCompletionRequestMessage::Developer(developer) => {
                push_system_text(&mut system_texts, developer.content);
            }
            ChatCompletionRequestMessage::User(user) => {
                if let Some(content) = map_user_message(user) {
                    contents.push(content);
                }
            }
            ChatCompletionRequestMessage::Assistant(assistant) => {
                if let Some(content) = map_assistant_message(assistant, &mut tool_call_index) {
                    contents.push(content);
                }
            }
            ChatCompletionRequestMessage::Tool(tool) => {
                if let Some(content) = map_tool_message(tool) {
                    contents.push(content);
                }
            }
            ChatCompletionRequestMessage::Function(function) => {
                if let Some(content) = map_function_message(function, &mut tool_call_index) {
                    contents.push(content);
                }
            }
        }
    }

    let system_instruction = if system_texts.is_empty() {
        None
    } else {
        Some(GeminiContent {
            parts: vec![GeminiPart {
                text: Some(system_texts.join("\n")),
                inline_data: None,
                function_call: None,
                function_response: None,
                file_data: None,
                executable_code: None,
                code_execution_result: None,
                thought: None,
                thought_signature: None,
                part_metadata: None,
                video_metadata: None,
            }],
            role: None,
        })
    };

    let mut tools = map_tools(request.body.tools);
    if request.body.web_search_options.is_some() {
        tools.push(GeminiTool {
            function_declarations: None,
            google_search_retrieval: None,
            code_execution: None,
            google_search: Some(GoogleSearch {
                time_range_filter: None,
            }),
            computer_use: None,
            url_context: None,
            file_search: None,
            google_maps: None,
        });
    }
    let tools = if tools.is_empty() { None } else { Some(tools) };

    let tool_config = map_tool_config(request.body.tool_choice, request.body.function_call);

    let model_id = model.strip_prefix("models/").unwrap_or(model.as_str());
    let (cached_content, extra_thinking_config) = map_extra_body(request.body.extra_body.as_ref());
    let generation_config = map_generation_config(
        request.body.max_completion_tokens,
        request.body.max_tokens,
        request.body.temperature,
        request.body.top_p,
        request.body.stop,
        request.body.response_format,
        request.body.modalities,
        request.body.reasoning_effort,
        extra_thinking_config,
        model_id,
    );

    GeminiGenerateContentRequest {
        path: GeminiGenerateContentPath { model },
        body: GeminiGenerateContentRequestBody {
            contents,
            model: None,
            tools,
            tool_config,
            safety_settings: None,
            system_instruction,
            generation_config,
            cached_content,
        },
    }
}

fn map_user_message(message: ChatCompletionRequestUserMessage) -> Option<GeminiContent> {
    let parts = map_user_content_to_parts(message.content);
    if parts.is_empty() {
        None
    } else {
        Some(GeminiContent {
            parts,
            role: Some(GeminiContentRole::User),
        })
    }
}

fn map_assistant_message(
    message: ChatCompletionRequestAssistantMessage,
    tool_call_index: &mut usize,
) -> Option<GeminiContent> {
    let mut parts = Vec::new();

    if let Some(content) = message.content {
        parts.extend(map_assistant_content_to_parts(content));
    }

    if let Some(refusal) = message.refusal
        && !refusal.is_empty()
    {
        parts.push(text_part(refusal));
    }

    if let Some(tool_calls) = message.tool_calls {
        for call in tool_calls {
            if let Some(part) = map_tool_call_to_part(call, tool_call_index) {
                parts.push(part);
            }
        }
    }

    if let Some(function_call) = message.function_call {
        let args = serde_json::from_str(&function_call.arguments)
            .unwrap_or(JsonValue::String(function_call.arguments));
        parts.push(GeminiPart {
            text: None,
            inline_data: None,
            function_call: Some(GeminiFunctionCall {
                id: Some(next_tool_call_id(tool_call_index)),
                name: function_call.name,
                args: Some(args),
            }),
            function_response: None,
            file_data: None,
            executable_code: None,
            code_execution_result: None,
            thought: None,
            thought_signature: None,
            part_metadata: None,
            video_metadata: None,
        });
    }

    if parts.is_empty() {
        None
    } else {
        Some(GeminiContent {
            parts,
            role: Some(GeminiContentRole::Model),
        })
    }
}

fn map_tool_message(message: ChatCompletionRequestToolMessage) -> Option<GeminiContent> {
    let response_text = map_text_content_to_string(message.content)?;
    let tool_call_id = message.tool_call_id;
    let part = GeminiPart {
        text: None,
        inline_data: None,
        function_call: None,
        function_response: Some(GeminiFunctionResponse {
            id: Some(tool_call_id.clone()),
            name: tool_call_id,
            response: JsonValue::String(response_text),
            parts: None,
            will_continue: None,
            scheduling: None,
        }),
        file_data: None,
        executable_code: None,
        code_execution_result: None,
        thought: None,
        thought_signature: None,
        part_metadata: None,
        video_metadata: None,
    };
    Some(GeminiContent {
        parts: vec![part],
        role: Some(GeminiContentRole::User),
    })
}

fn map_function_message(
    message: ChatCompletionRequestFunctionMessage,
    tool_call_index: &mut usize,
) -> Option<GeminiContent> {
    let response = message
        .content
        .map(JsonValue::String)
        .unwrap_or(JsonValue::Null);
    let part = GeminiPart {
        text: None,
        inline_data: None,
        function_call: None,
        function_response: Some(GeminiFunctionResponse {
            id: Some(next_tool_call_id(tool_call_index)),
            name: message.name,
            response,
            parts: None,
            will_continue: None,
            scheduling: None,
        }),
        file_data: None,
        executable_code: None,
        code_execution_result: None,
        thought: None,
        thought_signature: None,
        part_metadata: None,
        video_metadata: None,
    };
    Some(GeminiContent {
        parts: vec![part],
        role: Some(GeminiContentRole::User),
    })
}

fn map_user_content_to_parts(content: ChatCompletionUserContent) -> Vec<GeminiPart> {
    let mut parts = Vec::new();
    match content {
        ChatCompletionUserContent::Text(text) => {
            if !text.is_empty() {
                parts.push(text_part(text));
            }
        }
        ChatCompletionUserContent::Parts(items) => {
            for item in items {
                match item {
                    ChatCompletionUserContentPart::Text { text } => {
                        if !text.is_empty() {
                            parts.push(text_part(text));
                        }
                    }
                    ChatCompletionUserContentPart::ImageUrl { image_url } => {
                        if let Some(part) = map_image_url(image_url.url, image_url.detail) {
                            parts.push(part);
                        }
                    }
                    ChatCompletionUserContentPart::InputAudio { input_audio } => {
                        parts.push(map_input_audio(input_audio.format, input_audio.data));
                    }
                    ChatCompletionUserContentPart::File { file } => {
                        parts.push(map_input_file(file));
                    }
                }
            }
        }
    }
    parts
}

fn map_assistant_content_to_parts(content: ChatCompletionAssistantContent) -> Vec<GeminiPart> {
    let mut parts = Vec::new();
    match content {
        ChatCompletionAssistantContent::Text(text) => {
            if !text.is_empty() {
                parts.push(text_part(text));
            }
        }
        ChatCompletionAssistantContent::Parts(items) => {
            for item in items {
                match item {
                    ChatCompletionAssistantContentPart::Text { text } => {
                        if !text.is_empty() {
                            parts.push(text_part(text));
                        }
                    }
                    ChatCompletionAssistantContentPart::Refusal { refusal } => {
                        if !refusal.is_empty() {
                            parts.push(text_part(refusal));
                        }
                    }
                }
            }
        }
    }
    parts
}

fn map_tool_call_to_part(
    call: ChatCompletionMessageToolCall,
    _tool_call_index: &mut usize,
) -> Option<GeminiPart> {
    match call {
        ChatCompletionMessageToolCall::Function { id, function } => {
            let args = serde_json::from_str(&function.arguments)
                .unwrap_or(JsonValue::String(function.arguments));
            Some(GeminiPart {
                text: None,
                inline_data: None,
                function_call: Some(GeminiFunctionCall {
                    id: Some(id),
                    name: function.name,
                    args: Some(args),
                }),
                function_response: None,
                file_data: None,
                executable_code: None,
                code_execution_result: None,
                thought: None,
                thought_signature: None,
                part_metadata: None,
                video_metadata: None,
            })
        }
        ChatCompletionMessageToolCall::Custom { id, custom } => Some(GeminiPart {
            text: None,
            inline_data: None,
            function_call: Some(GeminiFunctionCall {
                id: Some(id),
                name: custom.name,
                args: Some(JsonValue::String(custom.input)),
            }),
            function_response: None,
            file_data: None,
            executable_code: None,
            code_execution_result: None,
            thought: None,
            thought_signature: None,
            part_metadata: None,
            video_metadata: None,
        }),
    }
}

fn map_image_url(url: String, detail: Option<ChatCompletionImageDetail>) -> Option<GeminiPart> {
    if let Some((mime, data)) = parse_data_url(&url) {
        let _ = detail;
        return Some(GeminiPart {
            text: None,
            inline_data: Some(GeminiBlob {
                mime_type: mime,
                data,
            }),
            function_call: None,
            function_response: None,
            file_data: None,
            executable_code: None,
            code_execution_result: None,
            thought: None,
            thought_signature: None,
            part_metadata: None,
            video_metadata: None,
        });
    }

    Some(GeminiPart {
        text: None,
        inline_data: None,
        function_call: None,
        function_response: None,
        file_data: Some(GeminiFileData {
            mime_type: None,
            file_uri: url,
        }),
        executable_code: None,
        code_execution_result: None,
        thought: None,
        thought_signature: None,
        part_metadata: None,
        video_metadata: None,
    })
}

fn map_input_audio(format: ChatCompletionInputAudioFormat, data: String) -> GeminiPart {
    let mime_type = match format {
        ChatCompletionInputAudioFormat::Wav => "audio/wav",
        ChatCompletionInputAudioFormat::Mp3 => "audio/mpeg",
    };
    GeminiPart {
        text: None,
        inline_data: Some(GeminiBlob {
            mime_type: mime_type.to_string(),
            data,
        }),
        function_call: None,
        function_response: None,
        file_data: None,
        executable_code: None,
        code_execution_result: None,
        thought: None,
        thought_signature: None,
        part_metadata: None,
        video_metadata: None,
    }
}

fn map_input_file(file: ChatCompletionInputFile) -> GeminiPart {
    if let Some(file_id) = file.file_id {
        return GeminiPart {
            text: None,
            inline_data: None,
            function_call: None,
            function_response: None,
            file_data: Some(GeminiFileData {
                mime_type: None,
                file_uri: file_id,
            }),
            executable_code: None,
            code_execution_result: None,
            thought: None,
            thought_signature: None,
            part_metadata: None,
            video_metadata: None,
        };
    }

    if let Some(data) = file.file_data {
        return GeminiPart {
            text: None,
            inline_data: Some(GeminiBlob {
                mime_type: "application/octet-stream".to_string(),
                data,
            }),
            function_call: None,
            function_response: None,
            file_data: None,
            executable_code: None,
            code_execution_result: None,
            thought: None,
            thought_signature: None,
            part_metadata: None,
            video_metadata: None,
        };
    }

    GeminiPart {
        text: Some(format!(
            "[file:{}]",
            file.filename.unwrap_or_else(|| "file".to_string())
        )),
        inline_data: None,
        function_call: None,
        function_response: None,
        file_data: None,
        executable_code: None,
        code_execution_result: None,
        thought: None,
        thought_signature: None,
        part_metadata: None,
        video_metadata: None,
    }
}

fn map_text_content_to_string(content: ChatCompletionTextContent) -> Option<String> {
    match content {
        ChatCompletionTextContent::Text(text) => {
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
        ChatCompletionTextContent::Parts(parts) => {
            let texts: Vec<String> = parts
                .into_iter()
                .filter_map(|part| match part {
                    ChatCompletionTextContentPart::Text { text } => {
                        if text.is_empty() {
                            None
                        } else {
                            Some(text)
                        }
                    }
                })
                .collect();
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n"))
            }
        }
    }
}

fn map_tools(tools: Option<Vec<ChatCompletionToolDefinition>>) -> Vec<GeminiTool> {
    let tools = match tools {
        Some(tools) => tools,
        None => return Vec::new(),
    };

    let mut function_declarations = Vec::new();
    for tool in tools {
        match tool {
            ChatCompletionToolDefinition::Function { function } => {
                function_declarations.push(map_function_declaration(function));
            }
            ChatCompletionToolDefinition::Custom { custom } => {
                function_declarations.push(FunctionDeclaration {
                    name: custom.name,
                    description: custom.description.unwrap_or_default(),
                    behavior: None,
                    parameters: None,
                    parameters_json_schema: None,
                    response: None,
                    response_json_schema: None,
                });
            }
        }
    }

    if function_declarations.is_empty() {
        Vec::new()
    } else {
        vec![GeminiTool {
            function_declarations: Some(function_declarations),
            google_search_retrieval: None,
            code_execution: None,
            google_search: None,
            computer_use: None,
            url_context: None,
            file_search: None,
            google_maps: None,
        }]
    }
}

fn map_function_declaration(function: FunctionObject) -> FunctionDeclaration {
    let parameters_json_schema = function
        .parameters
        .and_then(|schema| serde_json::to_value(schema).ok());

    FunctionDeclaration {
        name: function.name,
        description: function.description.unwrap_or_default(),
        behavior: None,
        parameters: None,
        parameters_json_schema,
        response: None,
        response_json_schema: None,
    }
}

fn map_tool_config(
    tool_choice: Option<ChatCompletionToolChoiceOption>,
    function_call: Option<ChatCompletionFunctionCallChoice>,
) -> Option<ToolConfig> {
    let config = tool_choice
        .and_then(map_tool_choice)
        .or_else(|| map_function_call_choice(function_call))?;

    Some(ToolConfig {
        function_calling_config: Some(config),
        retrieval_config: None,
    })
}

fn map_tool_choice(choice: ChatCompletionToolChoiceOption) -> Option<FunctionCallingConfig> {
    match choice {
        ChatCompletionToolChoiceOption::Mode(mode) => Some(FunctionCallingConfig {
            mode: Some(match mode {
                ChatCompletionToolChoiceMode::None => FunctionCallingMode::None,
                ChatCompletionToolChoiceMode::Auto => FunctionCallingMode::Auto,
                ChatCompletionToolChoiceMode::Required => FunctionCallingMode::Any,
            }),
            allowed_function_names: None,
        }),
        ChatCompletionToolChoiceOption::AllowedTools(allowed) => map_allowed_tools_choice(allowed),
        ChatCompletionToolChoiceOption::NamedTool(named) => Some(FunctionCallingConfig {
            mode: Some(FunctionCallingMode::Any),
            allowed_function_names: Some(vec![named.function.name]),
        }),
        ChatCompletionToolChoiceOption::NamedCustomTool(named) => Some(FunctionCallingConfig {
            mode: Some(FunctionCallingMode::Any),
            allowed_function_names: Some(vec![named.custom.name]),
        }),
    }
}

fn map_allowed_tools_choice(
    allowed: ChatCompletionAllowedToolsChoice,
) -> Option<FunctionCallingConfig> {
    let mut names = Vec::new();
    for tool in allowed.allowed_tools.tools {
        match tool {
            ChatCompletionAllowedTool::Function { function } => names.push(function.name),
            ChatCompletionAllowedTool::Custom { custom } => names.push(custom.name),
        }
    }

    let mode = match allowed.allowed_tools.mode {
        AllowedToolsMode::Auto => FunctionCallingMode::Auto,
        AllowedToolsMode::Required => FunctionCallingMode::Any,
    };

    Some(FunctionCallingConfig {
        mode: Some(mode),
        allowed_function_names: if names.is_empty() { None } else { Some(names) },
    })
}

fn map_function_call_choice(
    choice: Option<ChatCompletionFunctionCallChoice>,
) -> Option<FunctionCallingConfig> {
    match choice? {
        ChatCompletionFunctionCallChoice::Mode(mode) => Some(FunctionCallingConfig {
            mode: Some(match mode {
                ChatCompletionFunctionCallMode::None => FunctionCallingMode::None,
                ChatCompletionFunctionCallMode::Auto => FunctionCallingMode::Auto,
            }),
            allowed_function_names: None,
        }),
        ChatCompletionFunctionCallChoice::Named(ChatCompletionFunctionCallOption { name }) => {
            Some(FunctionCallingConfig {
                mode: Some(FunctionCallingMode::Any),
                allowed_function_names: Some(vec![name]),
            })
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn map_generation_config(
    max_completion_tokens: Option<i64>,
    max_tokens: Option<i64>,
    temperature: Option<f64>,
    top_p: Option<f64>,
    stop: Option<gproxy_protocol::openai::create_chat_completions::request::StopConfiguration>,
    response_format: Option<ChatCompletionResponseFormat>,
    modalities: Option<Vec<ResponseModality>>,
    reasoning_effort: Option<ReasoningEffort>,
    extra_thinking_config: Option<ThinkingConfig>,
    model_id: &str,
) -> Option<GenerationConfig> {
    let max_output_tokens = max_completion_tokens
        .or(max_tokens)
        .map(|value| value.max(0) as u32);

    let stop_sequences = match stop {
        Some(
            gproxy_protocol::openai::create_chat_completions::request::StopConfiguration::Single(
                value,
            ),
        ) => Some(vec![value]),
        Some(
            gproxy_protocol::openai::create_chat_completions::request::StopConfiguration::Many(
                values,
            ),
        ) => Some(values),
        None => None,
    };

    let (response_json_schema, response_mime_type) = map_response_format(response_format);

    let response_modalities = modalities.map(|modalities| {
        modalities
            .into_iter()
            .map(|modality| match modality {
                ResponseModality::Text => {
                    gproxy_protocol::gemini::count_tokens::types::Modality::Text
                }
                ResponseModality::Audio => {
                    gproxy_protocol::gemini::count_tokens::types::Modality::Audio
                }
            })
            .collect::<Vec<_>>()
    });

    let thinking_config =
        extra_thinking_config.or_else(|| map_thinking_config(reasoning_effort, model_id));

    if max_output_tokens.is_none()
        && temperature.is_none()
        && top_p.is_none()
        && stop_sequences.is_none()
        && response_json_schema.is_none()
        && response_mime_type.is_none()
        && response_modalities.as_ref().is_none_or(|m| m.is_empty())
        && thinking_config.is_none()
    {
        return None;
    }

    Some(GenerationConfig {
        stop_sequences,
        response_mime_type,
        response_schema: None,
        response_json_schema_internal: None,
        response_json_schema,
        response_modalities,
        candidate_count: None,
        max_output_tokens,
        temperature,
        top_p,
        top_k: None,
        seed: None,
        presence_penalty: None,
        frequency_penalty: None,
        response_logprobs: None,
        logprobs: None,
        enable_enhanced_civic_answers: None,
        speech_config: None,
        thinking_config,
        image_config: None,
        media_resolution: None,
    })
}

fn map_response_format(
    format: Option<ChatCompletionResponseFormat>,
) -> (Option<JsonValue>, Option<String>) {
    match format {
        Some(ChatCompletionResponseFormat::JsonSchema { json_schema }) => {
            let schema = json_schema
                .schema
                .and_then(|schema| serde_json::to_value(schema).ok())
                .unwrap_or_else(|| serde_json::json!({"type": "object"}));
            (Some(schema), None)
        }
        Some(ChatCompletionResponseFormat::JsonObject) => {
            (None, Some("application/json".to_string()))
        }
        _ => (None, None),
    }
}

fn map_thinking_config(
    reasoning_effort: Option<ReasoningEffort>,
    model_id: &str,
) -> Option<ThinkingConfig> {
    let effort = reasoning_effort?;
    let model_id = model_id.to_ascii_lowercase();

    if model_id.contains("gemini-2.5") {
        let is_pro =
            model_id.contains("gemini-2.5-pro") || model_id.contains("gemini-2.5-pro-preview");
        let budget = match effort {
            ReasoningEffort::None => {
                if is_pro {
                    return None;
                }
                0
            }
            ReasoningEffort::Minimal | ReasoningEffort::Low => 1024,
            ReasoningEffort::Medium => 8192,
            ReasoningEffort::High | ReasoningEffort::XHigh => 24576,
        };

        return Some(ThinkingConfig {
            include_thoughts: budget > 0,
            thinking_budget: budget,
            thinking_level: None,
        });
    }

    if model_id.contains("gemini-3") {
        let is_pro = model_id.contains("gemini-3-pro") || model_id.contains("gemini-3-pro-preview");
        let thinking_level = match effort {
            ReasoningEffort::None => None,
            ReasoningEffort::Minimal => {
                if is_pro {
                    Some(ThinkingLevel::Low)
                } else {
                    Some(ThinkingLevel::Minimal)
                }
            }
            ReasoningEffort::Low => Some(ThinkingLevel::Low),
            ReasoningEffort::Medium => {
                if is_pro {
                    None
                } else {
                    Some(ThinkingLevel::Medium)
                }
            }
            ReasoningEffort::High | ReasoningEffort::XHigh => Some(ThinkingLevel::High),
        };

        return thinking_level.map(|thinking_level| ThinkingConfig {
            include_thoughts: true,
            thinking_budget: 0,
            thinking_level: Some(thinking_level),
        });
    }

    let thinking_level = match effort {
        ReasoningEffort::None => None,
        ReasoningEffort::Minimal => Some(ThinkingLevel::Minimal),
        ReasoningEffort::Low => Some(ThinkingLevel::Low),
        ReasoningEffort::Medium => Some(ThinkingLevel::Medium),
        ReasoningEffort::High | ReasoningEffort::XHigh => Some(ThinkingLevel::High),
    };

    if thinking_level.is_none() {
        return Some(ThinkingConfig {
            include_thoughts: false,
            thinking_budget: 0,
            thinking_level: None,
        });
    }

    Some(ThinkingConfig {
        include_thoughts: true,
        thinking_budget: 0,
        thinking_level,
    })
}

fn map_extra_body(extra_body: Option<&JsonValue>) -> (Option<String>, Option<ThinkingConfig>) {
    let extra_body = match extra_body.and_then(|value| value.as_object()) {
        Some(value) => value,
        None => return (None, None),
    };
    let google = match extra_body.get("google").and_then(|value| value.as_object()) {
        Some(value) => value,
        None => return (None, None),
    };

    let cached_content = google
        .get("cached_content")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());

    let thinking_config = google
        .get("thinking_config")
        .and_then(map_thinking_config_from_value);

    (cached_content, thinking_config)
}

fn map_thinking_config_from_value(value: &JsonValue) -> Option<ThinkingConfig> {
    let object = value.as_object()?;
    let include_thoughts = object
        .get("include_thoughts")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let thinking_budget = object
        .get("thinking_budget")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    let thinking_level = object
        .get("thinking_level")
        .and_then(|value| value.as_str())
        .and_then(map_thinking_level);

    Some(ThinkingConfig {
        include_thoughts,
        thinking_budget: if thinking_budget > u32::MAX as u64 {
            u32::MAX
        } else {
            thinking_budget as u32
        },
        thinking_level,
    })
}

fn map_thinking_level(value: &str) -> Option<ThinkingLevel> {
    match value.to_ascii_lowercase().as_str() {
        "minimal" => Some(ThinkingLevel::Minimal),
        "low" => Some(ThinkingLevel::Low),
        "medium" => Some(ThinkingLevel::Medium),
        "high" => Some(ThinkingLevel::High),
        "thinking_level_unspecified" | "unspecified" => {
            Some(ThinkingLevel::ThinkingLevelUnspecified)
        }
        _ => None,
    }
}

fn push_system_text(output: &mut Vec<String>, content: ChatCompletionTextContent) {
    match content {
        ChatCompletionTextContent::Text(text) => {
            if !text.is_empty() {
                output.push(text);
            }
        }
        ChatCompletionTextContent::Parts(parts) => {
            for part in parts {
                let ChatCompletionTextContentPart::Text { text } = part;
                if !text.is_empty() {
                    output.push(text);
                }
            }
        }
    }
}

fn text_part(text: String) -> GeminiPart {
    GeminiPart {
        text: Some(text),
        inline_data: None,
        function_call: None,
        function_response: None,
        file_data: None,
        executable_code: None,
        code_execution_result: None,
        thought: None,
        thought_signature: None,
        part_metadata: None,
        video_metadata: None,
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

fn next_tool_call_id(counter: &mut usize) -> String {
    let id = format!("tool_call_{}", counter);
    *counter += 1;
    id
}
