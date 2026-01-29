use gproxy_protocol::gemini::count_tokens::types::{
    Blob as GeminiBlob, Content as GeminiContent, ContentRole as GeminiContentRole,
    FileData as GeminiFileData, Modality as GeminiModality, Part as GeminiPart,
};
use gproxy_protocol::gemini::generate_content::request::{
    GenerateContentPath as GeminiGenerateContentPath,
    GenerateContentRequest as GeminiGenerateContentRequest,
    GenerateContentRequestBody as GeminiGenerateContentRequestBody,
};
use gproxy_protocol::gemini::generate_content::types::{
    CodeExecution, ComputerUse, Environment, FileSearch, FunctionCallingConfig,
    FunctionCallingMode, FunctionDeclaration, GenerationConfig, GoogleSearch, ImageConfig,
    ThinkingConfig, ThinkingLevel, Tool as GeminiTool, ToolConfig,
};
use gproxy_protocol::openai::create_response::request::CreateResponseRequest as OpenAIResponseRequest;
use gproxy_protocol::openai::create_response::types::{
    AllowedTool, CustomTool, EasyInputMessage, EasyInputMessageContent, EasyInputMessageRole,
    FunctionTool, ImageGenSize, ImageGenTool, InputContent, InputFileContent, InputImageContent,
    InputItem, InputMessage, InputMessageRole, InputParam, Instructions, OutputMessage,
    OutputMessageContent, Reasoning, ReasoningEffort, ResponseTextParam,
    TextResponseFormatConfiguration, Tool, ToolChoiceAllowed, ToolChoiceAllowedMode,
    ToolChoiceOptions, ToolChoiceParam,
};
use serde_json::Value as JsonValue;

/// Convert an OpenAI responses request into a Gemini generate-content request.
pub fn transform_request(request: OpenAIResponseRequest) -> GeminiGenerateContentRequest {
    let model = request.body.model.clone();

    let mut system_texts = Vec::new();
    let mut contents = Vec::new();

    if let Some(instructions) = request.body.instructions {
        push_system_texts_from_instructions(instructions, &mut contents, &mut system_texts);
    }

    if let Some(input) = request.body.input {
        append_input_param(input, &mut contents, &mut system_texts);
    }

    let system_instruction = if system_texts.is_empty() {
        None
    } else {
        Some(GeminiContent {
            parts: vec![text_part(system_texts.join("\n"))],
            role: None,
        })
    };

    let (tools, image_tool) = request.body.tools.map(map_tools).unwrap_or_default();
    let tools = if tools.is_empty() { None } else { Some(tools) };

    let tool_config = map_tool_choice(request.body.tool_choice);
    let generation_config = map_generation_config(
        request.body.text,
        request.body.reasoning,
        image_tool,
        request.body.max_output_tokens,
        request.body.temperature,
        request.body.top_p,
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
            cached_content: None,
        },
    }
}

fn push_system_texts_from_instructions(
    instructions: Instructions,
    contents: &mut Vec<GeminiContent>,
    system_texts: &mut Vec<String>,
) {
    match instructions {
        Instructions::Text(text) => push_system_text(system_texts, text),
        Instructions::Items(items) => {
            for item in items {
                append_input_item(item, contents, system_texts);
            }
        }
    }
}

fn append_input_param(
    input: InputParam,
    contents: &mut Vec<GeminiContent>,
    system_texts: &mut Vec<String>,
) {
    match input {
        InputParam::Text(text) => {
            if let Some(content) =
                make_content(Some(GeminiContentRole::User), vec![text_part(text)])
            {
                contents.push(content);
            }
        }
        InputParam::Items(items) => {
            for item in items {
                append_input_item(item, contents, system_texts);
            }
        }
    }
}

fn append_input_item(
    item: InputItem,
    contents: &mut Vec<GeminiContent>,
    system_texts: &mut Vec<String>,
) {
    match item {
        InputItem::EasyMessage(message) => {
            append_easy_message(message, contents, system_texts);
        }
        InputItem::Item(item) => match item {
            gproxy_protocol::openai::create_response::types::Item::InputMessage(message) => {
                append_input_message(message, contents, system_texts);
            }
            gproxy_protocol::openai::create_response::types::Item::OutputMessage(message) => {
                append_output_message(message, contents);
            }
            _ => {}
        },
        InputItem::Reference(_) => {}
    }
}

fn append_easy_message(
    message: EasyInputMessage,
    contents: &mut Vec<GeminiContent>,
    system_texts: &mut Vec<String>,
) {
    match message.role {
        EasyInputMessageRole::User => {
            if let Some(content) =
                easy_message_content_to_content(message.content, Some(GeminiContentRole::User))
            {
                contents.push(content);
            }
        }
        EasyInputMessageRole::Assistant => {
            if let Some(content) =
                easy_message_content_to_content(message.content, Some(GeminiContentRole::Model))
            {
                contents.push(content);
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
    contents: &mut Vec<GeminiContent>,
    system_texts: &mut Vec<String>,
) {
    match message.role {
        InputMessageRole::User => {
            let parts = input_contents_to_parts(&message.content);
            if let Some(content) = make_content(Some(GeminiContentRole::User), parts) {
                contents.push(content);
            }
        }
        InputMessageRole::System | InputMessageRole::Developer => {
            if let Some(text) = input_contents_to_text(&message.content) {
                push_system_text(system_texts, text);
            }
        }
    }
}

fn append_output_message(message: OutputMessage, contents: &mut Vec<GeminiContent>) {
    let parts = output_contents_to_parts(&message.content);
    if let Some(content) = make_content(Some(GeminiContentRole::Model), parts) {
        contents.push(content);
    }
}

fn easy_message_content_to_content(
    content: EasyInputMessageContent,
    role: Option<GeminiContentRole>,
) -> Option<GeminiContent> {
    match content {
        EasyInputMessageContent::Text(text) => make_content(role, vec![text_part(text)]),
        EasyInputMessageContent::Parts(parts) => {
            let parts = input_contents_to_parts(&parts);
            make_content(role, parts)
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

fn input_contents_to_parts(contents: &[InputContent]) -> Vec<GeminiPart> {
    let mut parts = Vec::new();
    for content in contents {
        if let Some(part) = map_input_content(content) {
            parts.push(part);
        }
    }
    parts
}

fn output_contents_to_parts(contents: &[OutputMessageContent]) -> Vec<GeminiPart> {
    let mut parts = Vec::new();
    for content in contents {
        let text = match content {
            OutputMessageContent::OutputText(value) => value.text.clone(),
            OutputMessageContent::Refusal(value) => value.refusal.clone(),
        };
        if !text.is_empty() {
            parts.push(text_part(text));
        }
    }
    parts
}

fn map_input_content(content: &InputContent) -> Option<GeminiPart> {
    match content {
        InputContent::InputText(text) => Some(text_part(text.text.clone())),
        InputContent::InputImage(image) => map_image_content(image),
        InputContent::InputFile(file) => map_file_content(file),
    }
}

fn map_image_content(content: &InputImageContent) -> Option<GeminiPart> {
    if let Some(url) = &content.image_url {
        return Some(file_part(url.clone(), None));
    }

    if let Some(file_id) = &content.file_id {
        return Some(file_part(file_id.clone(), None));
    }

    None
}

fn map_file_content(content: &InputFileContent) -> Option<GeminiPart> {
    if let Some(file_url) = &content.file_url {
        return Some(file_part(file_url.clone(), None));
    }

    if let Some(file_id) = &content.file_id {
        return Some(file_part(file_id.clone(), None));
    }

    if let Some(file_data) = &content.file_data {
        return Some(GeminiPart {
            text: None,
            inline_data: Some(GeminiBlob {
                mime_type: "application/octet-stream".to_string(),
                data: file_data.clone(),
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

    None
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

fn file_part(file_uri: String, mime_type: Option<String>) -> GeminiPart {
    GeminiPart {
        text: None,
        inline_data: None,
        function_call: None,
        function_response: None,
        file_data: Some(GeminiFileData {
            mime_type,
            file_uri,
        }),
        executable_code: None,
        code_execution_result: None,
        thought: None,
        thought_signature: None,
        part_metadata: None,
        video_metadata: None,
    }
}

fn make_content(role: Option<GeminiContentRole>, parts: Vec<GeminiPart>) -> Option<GeminiContent> {
    if parts.is_empty() {
        None
    } else {
        Some(GeminiContent { parts, role })
    }
}

fn map_tools(tools: Vec<Tool>) -> (Vec<GeminiTool>, Option<ImageGenTool>) {
    let mut output = Vec::new();
    let mut function_decls = Vec::new();
    let mut image_tool = None;

    for tool in tools {
        match tool {
            Tool::Function(function) => function_decls.push(map_function_tool(function)),
            Tool::Custom(custom) => function_decls.push(map_custom_tool(custom)),
            Tool::CodeInterpreter(_) => output.push(GeminiTool {
                function_declarations: None,
                google_search_retrieval: None,
                code_execution: Some(CodeExecution {}),
                google_search: None,
                computer_use: None,
                url_context: None,
                file_search: None,
                google_maps: None,
            }),
            Tool::ComputerUsePreview(_) => output.push(GeminiTool {
                function_declarations: None,
                google_search_retrieval: None,
                code_execution: None,
                google_search: None,
                computer_use: Some(ComputerUse {
                    environment: Environment::EnvironmentBrowser,
                    excluded_predefined_functions: None,
                }),
                url_context: None,
                file_search: None,
                google_maps: None,
            }),
            Tool::WebSearch(_)
            | Tool::WebSearch20250826(_)
            | Tool::WebSearchPreview(_)
            | Tool::WebSearchPreview20250311(_) => {
                output.push(GeminiTool {
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
            Tool::FileSearch(tool) => output.push(GeminiTool {
                function_declarations: None,
                google_search_retrieval: None,
                code_execution: None,
                google_search: None,
                computer_use: None,
                url_context: None,
                file_search: Some(FileSearch {
                    file_search_store_names: tool.vector_store_ids,
                    metadata_filter: None,
                    top_k: tool.max_num_results.map(|value| value as u32),
                }),
                google_maps: None,
            }),
            Tool::ImageGeneration(tool) => {
                image_tool = Some(tool);
            }
            Tool::LocalShell(_) | Tool::Shell(_) | Tool::ApplyPatch(_) => {}
            Tool::MCP(_) => {}
        }
    }

    if !function_decls.is_empty() {
        output.push(GeminiTool {
            function_declarations: Some(function_decls),
            google_search_retrieval: None,
            code_execution: None,
            google_search: None,
            computer_use: None,
            url_context: None,
            file_search: None,
            google_maps: None,
        });
    }

    (output, image_tool)
}

fn map_function_tool(function: FunctionTool) -> FunctionDeclaration {
    FunctionDeclaration {
        name: function.name,
        description: function.description.unwrap_or_default(),
        behavior: None,
        parameters: None,
        parameters_json_schema: function.parameters,
        response: None,
        response_json_schema: None,
    }
}

fn map_custom_tool(tool: CustomTool) -> FunctionDeclaration {
    let schema = minimal_object_schema();

    FunctionDeclaration {
        name: tool.name,
        description: tool.description.unwrap_or_default(),
        behavior: None,
        parameters: None,
        parameters_json_schema: Some(schema),
        response: None,
        response_json_schema: None,
    }
}

fn minimal_object_schema() -> JsonValue {
    let mut map = serde_json::Map::new();
    map.insert("type".to_string(), JsonValue::String("object".to_string()));
    JsonValue::Object(map)
}

fn map_tool_choice(choice: Option<ToolChoiceParam>) -> Option<ToolConfig> {
    let choice = choice?;

    let (mode, allowed_function_names) = match choice {
        ToolChoiceParam::Mode(mode) => match mode {
            ToolChoiceOptions::None => (FunctionCallingMode::None, None),
            ToolChoiceOptions::Auto => (FunctionCallingMode::Auto, None),
            ToolChoiceOptions::Required => (FunctionCallingMode::Any, None),
        },
        ToolChoiceParam::Allowed(allowed) => map_allowed_tools(allowed),
        ToolChoiceParam::BuiltIn(_) => (FunctionCallingMode::Any, None),
        ToolChoiceParam::Function(tool) => (FunctionCallingMode::Any, Some(vec![tool.name])),
        ToolChoiceParam::Custom(tool) => (FunctionCallingMode::Any, Some(vec![tool.name])),
        ToolChoiceParam::MCP(tool) => (
            FunctionCallingMode::Any,
            Some(vec![tool.name.unwrap_or(tool.server_label)]),
        ),
        ToolChoiceParam::ApplyPatch(_) => (
            FunctionCallingMode::Any,
            Some(vec!["apply_patch".to_string()]),
        ),
        ToolChoiceParam::Shell(_) => (FunctionCallingMode::Any, Some(vec!["shell".to_string()])),
    };

    Some(ToolConfig {
        function_calling_config: Some(FunctionCallingConfig {
            mode: Some(mode),
            allowed_function_names,
        }),
        retrieval_config: None,
    })
}

fn map_allowed_tools(allowed: ToolChoiceAllowed) -> (FunctionCallingMode, Option<Vec<String>>) {
    let mode = match allowed.mode {
        ToolChoiceAllowedMode::Auto => FunctionCallingMode::Auto,
        ToolChoiceAllowedMode::Required => FunctionCallingMode::Any,
    };

    let mut names = Vec::new();
    for tool in allowed.tools {
        match tool {
            AllowedTool::Function { name } => names.push(name),
            AllowedTool::Custom { name } => names.push(name),
            _ => {}
        }
    }

    if names.is_empty() {
        (mode, None)
    } else {
        (mode, Some(names))
    }
}

fn map_generation_config(
    text: Option<ResponseTextParam>,
    reasoning: Option<Reasoning>,
    image_tool: Option<ImageGenTool>,
    max_output_tokens: Option<i64>,
    temperature: Option<f64>,
    top_p: Option<f64>,
) -> Option<GenerationConfig> {
    let response_json_schema = map_response_schema(text);
    let response_mime_type = response_json_schema
        .as_ref()
        .map(|_| "application/json".to_string());
    let thinking_config = map_reasoning(reasoning);
    let (image_config, response_modalities) = map_image_config(image_tool);
    let max_output_tokens = max_output_tokens.map(|value| value.max(0) as u32);

    if response_json_schema.is_none()
        && thinking_config.is_none()
        && image_config.is_none()
        && response_modalities.is_none()
        && max_output_tokens.is_none()
        && temperature.is_none()
        && top_p.is_none()
    {
        return None;
    }

    Some(GenerationConfig {
        stop_sequences: None,
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
        image_config,
        media_resolution: None,
    })
}

fn map_response_schema(text: Option<ResponseTextParam>) -> Option<JsonValue> {
    let format = text.and_then(|text| text.format)?;

    match format {
        TextResponseFormatConfiguration::Text => None,
        TextResponseFormatConfiguration::JsonObject => Some(minimal_object_schema()),
        TextResponseFormatConfiguration::JsonSchema { schema, .. } => Some(schema),
    }
}

fn map_reasoning(reasoning: Option<Reasoning>) -> Option<ThinkingConfig> {
    let effort = reasoning.and_then(|reasoning| reasoning.effort)?;

    let (thinking_level, thinking_budget, include_thoughts) = match effort {
        ReasoningEffort::None => (None, 0, false),
        ReasoningEffort::Minimal => (Some(ThinkingLevel::Minimal), 1024, true),
        ReasoningEffort::Low => (Some(ThinkingLevel::Low), 1024, true),
        ReasoningEffort::Medium => (Some(ThinkingLevel::Medium), 1024, true),
        ReasoningEffort::High | ReasoningEffort::XHigh => (Some(ThinkingLevel::High), 1024, true),
    };

    Some(ThinkingConfig {
        include_thoughts,
        thinking_budget,
        thinking_level,
    })
}

fn map_image_config(
    image_tool: Option<ImageGenTool>,
) -> (Option<ImageConfig>, Option<Vec<GeminiModality>>) {
    let tool = match image_tool {
        Some(tool) => tool,
        None => return (None, None),
    };

    let image_size = tool.size.map(map_image_size);

    (
        Some(ImageConfig {
            aspect_ratio: None,
            image_size,
        }),
        Some(vec![GeminiModality::Image]),
    )
}

fn map_image_size(size: ImageGenSize) -> String {
    match size {
        ImageGenSize::S1024x1024 => "1024x1024",
        ImageGenSize::S1024x1536 => "1024x1536",
        ImageGenSize::S1536x1024 => "1536x1024",
        ImageGenSize::Auto => "auto",
    }
    .to_string()
}

fn push_system_text(system_texts: &mut Vec<String>, text: String) {
    if !text.is_empty() {
        system_texts.push(text);
    }
}
