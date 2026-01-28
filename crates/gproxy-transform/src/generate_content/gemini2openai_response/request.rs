use gproxy_protocol::gemini::count_tokens::types::{
    Blob as GeminiBlob, Content as GeminiContent, ContentRole as GeminiContentRole,
    FileData as GeminiFileData, Part as GeminiPart,
};
use gproxy_protocol::gemini::generate_content::request::GenerateContentRequest as GeminiGenerateContentRequest;
use gproxy_protocol::gemini::generate_content::types::{
    FunctionCallingMode, FunctionDeclaration, GenerationConfig, Tool as GeminiTool, ToolConfig,
};
use gproxy_protocol::openai::create_response::request::{
    CreateResponseRequest as OpenAIResponseRequest,
    CreateResponseRequestBody as OpenAIResponseRequestBody,
};
use gproxy_protocol::openai::create_response::types::{
    AllowedTool, CodeInterpreterContainer, CodeInterpreterContainerParams, CodeInterpreterTool,
    ComputerEnvironment, ComputerUsePreviewTool, EasyInputMessage, EasyInputMessageContent,
    EasyInputMessageRole, EasyInputMessageType, FileSearchTool, FunctionTool, ImageGenSize,
    ImageGenTool, InputContent, InputFileContent, InputImageContent, InputItem, InputParam,
    InputTextContent, Instructions, Reasoning, ReasoningEffort, ResponseTextParam,
    TextResponseFormatConfiguration, Tool, ToolChoiceAllowed, ToolChoiceAllowedMode,
    ToolChoiceAllowedType, ToolChoiceOptions, ToolChoiceParam, WebSearchTool,
};
use serde::Serialize;
use serde_json::Value as JsonValue;

/// Convert a Gemini generate-content request into an OpenAI responses request.
pub fn transform_request(request: GeminiGenerateContentRequest) -> OpenAIResponseRequest {
    let model = request
        .path
        .model
        .strip_prefix("models/")
        .unwrap_or(&request.path.model)
        .to_string();

    let input = map_contents_to_input(&request.body.contents);
    let instructions = request
        .body
        .system_instruction
        .and_then(map_system_instruction)
        .map(Instructions::Text);

    let mut openai_tools = request.body.tools.map(map_tools).unwrap_or_default();
    if let Some(image_tool) = map_image_generation_tool(request.body.generation_config.as_ref()) {
        openai_tools.push(Tool::ImageGeneration(image_tool));
    }
    let tools = if openai_tools.is_empty() {
        None
    } else {
        Some(openai_tools)
    };

    let tool_choice = map_tool_choice(request.body.tool_config.as_ref());
    let text = map_response_format(request.body.generation_config.as_ref());
    let reasoning = map_reasoning(request.body.generation_config.as_ref());

    OpenAIResponseRequest {
        body: OpenAIResponseRequestBody {
            model,
            input,
            include: None,
            parallel_tool_calls: None,
            store: None,
            instructions,
            stream: None,
            stream_options: None,
            conversation: None,
            previous_response_id: None,
            reasoning,
            background: None,
            max_output_tokens: request
                .body
                .generation_config
                .as_ref()
                .and_then(|config| config.max_output_tokens.map(|value| value as i64)),
            max_tool_calls: None,
            text,
            tools,
            tool_choice,
            prompt: None,
            truncation: None,
            top_logprobs: None,
            metadata: None,
            temperature: request
                .body
                .generation_config
                .as_ref()
                .and_then(|config| config.temperature),
            top_p: request
                .body
                .generation_config
                .as_ref()
                .and_then(|config| config.top_p),
            user: None,
            safety_identifier: None,
            prompt_cache_key: None,
            service_tier: None,
            prompt_cache_retention: None,
        },
    }
}

fn map_contents_to_input(contents: &[GeminiContent]) -> Option<InputParam> {
    let mut items = Vec::new();
    for content in contents {
        if let Some(message) = map_content_to_easy_message(content) {
            items.push(InputItem::EasyMessage(message));
        }
    }

    if items.is_empty() {
        None
    } else {
        Some(InputParam::Items(items))
    }
}

fn map_content_to_easy_message(content: &GeminiContent) -> Option<EasyInputMessage> {
    let role = match content.role {
        Some(GeminiContentRole::Model) => EasyInputMessageRole::Assistant,
        _ => EasyInputMessageRole::User,
    };

    let parts = map_parts_to_input_contents(&content.parts);
    if parts.is_empty() {
        return None;
    }

    let content = if parts.len() == 1 {
        match &parts[0] {
            InputContent::InputText(text) => EasyInputMessageContent::Text(text.text.clone()),
            _ => EasyInputMessageContent::Parts(parts),
        }
    } else {
        EasyInputMessageContent::Parts(parts)
    };

    Some(EasyInputMessage {
        r#type: EasyInputMessageType::Message,
        role,
        content,
    })
}

fn map_parts_to_input_contents(parts: &[GeminiPart]) -> Vec<InputContent> {
    let mut contents = Vec::new();
    for part in parts {
        if let Some(text) = part.text.clone() {
            push_text_content(&mut contents, text);
        }

        if let Some(blob) = &part.inline_data {
            push_inline_blob(&mut contents, blob);
        }

        if let Some(file) = &part.file_data {
            push_file_data(&mut contents, file);
        }

        if let Some(function_call) = &part.function_call {
            push_json_text(&mut contents, "function_call", function_call);
        }

        if let Some(function_response) = &part.function_response {
            push_json_text(&mut contents, "function_response", function_response);
        }

        if let Some(code) = &part.executable_code {
            push_json_text(&mut contents, "executable_code", code);
        }

        if let Some(result) = &part.code_execution_result {
            push_json_text(&mut contents, "code_execution_result", result);
        }
    }
    contents
}

fn push_text_content(contents: &mut Vec<InputContent>, text: String) {
    if !text.is_empty() {
        contents.push(InputContent::InputText(InputTextContent { text }));
    }
}

fn push_inline_blob(contents: &mut Vec<InputContent>, blob: &GeminiBlob) {
    contents.push(InputContent::InputFile(InputFileContent {
        file_id: None,
        filename: None,
        file_url: None,
        file_data: Some(blob.data.clone()),
    }));
}

fn push_file_data(contents: &mut Vec<InputContent>, file: &GeminiFileData) {
    if let Some(mime_type) = &file.mime_type
        && mime_type.starts_with("image/")
    {
        contents.push(InputContent::InputImage(InputImageContent {
            image_url: Some(file.file_uri.clone()),
            file_id: None,
            detail: None,
        }));
        return;
    }

    contents.push(InputContent::InputFile(InputFileContent {
        file_id: None,
        filename: None,
        file_url: Some(file.file_uri.clone()),
        file_data: None,
    }));
}

fn push_json_text<T: Serialize>(contents: &mut Vec<InputContent>, label: &str, value: &T) {
    if let Ok(json) = serde_json::to_string(value) {
        let text = format!("[{}] {}", label, json);
        contents.push(InputContent::InputText(InputTextContent { text }));
    }
}

fn map_system_instruction(system: GeminiContent) -> Option<String> {
    let texts: Vec<String> = system
        .parts
        .iter()
        .filter_map(|part| part.text.clone())
        .collect();

    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}

fn map_tools(tools: Vec<GeminiTool>) -> Vec<Tool> {
    let mut output = Vec::new();
    for tool in tools {
        if let Some(functions) = tool.function_declarations {
            for function in functions {
                output.push(Tool::Function(map_function_tool(function)));
            }
        }

        if tool.google_search.is_some() || tool.google_search_retrieval.is_some() {
            output.push(Tool::WebSearch(WebSearchTool {
                filters: None,
                user_location: None,
                search_context_size: None,
            }));
        }

        if tool.code_execution.is_some() {
            output.push(Tool::CodeInterpreter(CodeInterpreterTool {
                container: CodeInterpreterContainer::Params(CodeInterpreterContainerParams {
                    file_ids: Vec::new(),
                    memory_limit: None,
                }),
            }));
        }

        if tool.computer_use.is_some() {
            output.push(Tool::ComputerUsePreview(ComputerUsePreviewTool {
                environment: ComputerEnvironment::Browser,
                display_width: 1024,
                display_height: 768,
            }));
        }

        if let Some(file_search) = tool.file_search {
            output.push(Tool::FileSearch(FileSearchTool {
                vector_store_ids: file_search.file_search_store_names,
                max_num_results: file_search.top_k.map(|value| value as i64),
                ranking_options: None,
                filters: None,
            }));
        }
    }

    output
}

fn map_function_tool(function: FunctionDeclaration) -> FunctionTool {
    let parameters = function
        .parameters_json_schema
        .or_else(|| serde_json::to_value(&function.parameters).ok())
        .and_then(|value| value.as_object().cloned().map(JsonValue::Object));

    FunctionTool {
        name: function.name,
        description: Some(function.description),
        parameters,
        strict: None,
    }
}

fn map_tool_choice(tool_config: Option<&ToolConfig>) -> Option<ToolChoiceParam> {
    let config =
        tool_config.and_then(|tool_config| tool_config.function_calling_config.as_ref())?;

    let mode = config.mode.unwrap_or(FunctionCallingMode::ModeUnspecified);
    let allowed = config.allowed_function_names.clone().unwrap_or_default();

    match mode {
        FunctionCallingMode::None => Some(ToolChoiceParam::Mode(ToolChoiceOptions::None)),
        FunctionCallingMode::Auto => {
            if allowed.is_empty() {
                Some(ToolChoiceParam::Mode(ToolChoiceOptions::Auto))
            } else {
                Some(ToolChoiceParam::Allowed(ToolChoiceAllowed {
                    r#type: ToolChoiceAllowedType::AllowedTools,
                    mode: ToolChoiceAllowedMode::Auto,
                    tools: allowed
                        .into_iter()
                        .map(|name| AllowedTool::Function { name })
                        .collect(),
                }))
            }
        }
        FunctionCallingMode::Any | FunctionCallingMode::Validated => {
            if allowed.is_empty() {
                Some(ToolChoiceParam::Mode(ToolChoiceOptions::Required))
            } else {
                Some(ToolChoiceParam::Allowed(ToolChoiceAllowed {
                    r#type: ToolChoiceAllowedType::AllowedTools,
                    mode: ToolChoiceAllowedMode::Required,
                    tools: allowed
                        .into_iter()
                        .map(|name| AllowedTool::Function { name })
                        .collect(),
                }))
            }
        }
        FunctionCallingMode::ModeUnspecified => None,
    }
}

fn map_response_format(config: Option<&GenerationConfig>) -> Option<ResponseTextParam> {
    let config = config?;
    let schema = config
        .response_json_schema
        .clone()
        .or_else(|| config.response_json_schema_internal.clone());

    let format = if let Some(schema) = schema {
        Some(TextResponseFormatConfiguration::JsonSchema {
            name: "response".to_string(),
            description: None,
            schema,
            strict: None,
        })
    } else if config.response_mime_type.as_deref() == Some("application/json") {
        Some(TextResponseFormatConfiguration::JsonObject)
    } else {
        None
    };

    format.map(|format| ResponseTextParam {
        format: Some(format),
        verbosity: None,
    })
}

fn map_reasoning(config: Option<&GenerationConfig>) -> Option<Reasoning> {
    let thinking = config.and_then(|config| config.thinking_config.as_ref())?;

    let effort = if !thinking.include_thoughts || thinking.thinking_budget == 0 {
        ReasoningEffort::None
    } else {
        match thinking.thinking_level {
            Some(gproxy_protocol::gemini::generate_content::types::ThinkingLevel::Minimal) => {
                ReasoningEffort::Minimal
            }
            Some(gproxy_protocol::gemini::generate_content::types::ThinkingLevel::Low) => {
                ReasoningEffort::Low
            }
            Some(gproxy_protocol::gemini::generate_content::types::ThinkingLevel::Medium) => {
                ReasoningEffort::Medium
            }
            Some(gproxy_protocol::gemini::generate_content::types::ThinkingLevel::High) => {
                ReasoningEffort::High
            }
            Some(gproxy_protocol::gemini::generate_content::types::ThinkingLevel::ThinkingLevelUnspecified)
            | None => ReasoningEffort::Low,
        }
    };

    Some(Reasoning {
        effort: Some(effort),
        summary: None,
        generate_summary: None,
    })
}

fn map_image_generation_tool(config: Option<&GenerationConfig>) -> Option<ImageGenTool> {
    let config = config?;
    let wants_image = config
        .response_modalities
        .as_ref()
        .map(|modalities| {
            modalities.iter().any(|modality| {
                matches!(
                    modality,
                    gproxy_protocol::gemini::count_tokens::types::Modality::Image
                )
            })
        })
        .unwrap_or(false);

    let image_config = config.image_config.as_ref();

    if !wants_image && image_config.is_none() {
        return None;
    }

    let size = image_config
        .and_then(|config| config.image_size.as_deref())
        .and_then(map_image_size);

    Some(ImageGenTool {
        model: None,
        quality: None,
        size,
        output_format: None,
        output_compression: None,
        moderation: None,
        background: None,
        input_fidelity: None,
        input_image_mask: None,
        partial_images: None,
    })
}

fn map_image_size(size: &str) -> Option<ImageGenSize> {
    match size {
        "1024x1024" => Some(ImageGenSize::S1024x1024),
        "1024x1536" => Some(ImageGenSize::S1024x1536),
        "1536x1024" => Some(ImageGenSize::S1536x1024),
        "auto" => Some(ImageGenSize::Auto),
        _ => None,
    }
}
