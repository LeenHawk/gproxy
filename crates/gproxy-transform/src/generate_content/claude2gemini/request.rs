use gproxy_protocol::claude::count_tokens::types::{
    BetaContentBlockParam as ClaudeContentBlockParam, BetaDocumentSource as ClaudeDocumentSource,
    BetaImageMediaType as ClaudeImageMediaType, BetaImageSource as ClaudeImageSource,
    BetaJSONOutputFormat as ClaudeJSONOutputFormat, BetaMessageContent as ClaudeMessageContent,
    BetaMessageParam as ClaudeMessageParam, BetaMessageRole as ClaudeMessageRole,
    BetaOutputConfig as ClaudeOutputConfig, BetaOutputEffort as ClaudeOutputEffort,
    BetaRequestDocumentBlock as ClaudeDocumentBlock, BetaSystemParam as ClaudeSystemParam,
    BetaThinkingConfigParam as ClaudeThinkingConfigParam, BetaTool as ClaudeTool,
    BetaToolBuiltin as ClaudeToolBuiltin, BetaToolChoice as ClaudeToolChoice,
    BetaToolCustom as ClaudeToolCustom, BetaToolInputSchema as ClaudeToolInputSchema,
    BetaWebSearchTool as ClaudeWebSearchTool, Model as ClaudeModel,
};
use gproxy_protocol::claude::create_message::request::CreateMessageRequest as ClaudeCreateMessageRequest;
use gproxy_protocol::gemini::count_tokens::types::{
    Blob as GeminiBlob, Content as GeminiContent, ContentRole as GeminiContentRole,
    FileData as GeminiFileData, Part as GeminiPart,
};
use gproxy_protocol::gemini::generate_content::request::{
    GenerateContentPath as GeminiGenerateContentPath,
    GenerateContentRequest as GeminiGenerateContentRequest,
    GenerateContentRequestBody as GeminiGenerateContentRequestBody,
};
use gproxy_protocol::gemini::generate_content::types::{
    CodeExecution, Environment, FileSearch, FunctionCallingConfig, FunctionCallingMode,
    FunctionDeclaration, GenerationConfig, GoogleSearch, ThinkingConfig, ThinkingLevel,
    Tool as GeminiTool, ToolConfig,
};
use serde_json::Value as JsonValue;

/// Convert a Claude create-message request into a Gemini generate-content request.
pub fn transform_request(request: ClaudeCreateMessageRequest) -> GeminiGenerateContentRequest {
    let model_id = match &request.body.model {
        ClaudeModel::Custom(value) => value.clone(),
        ClaudeModel::Known(known) => match serde_json::to_value(known) {
            Ok(JsonValue::String(value)) => value,
            _ => "unknown".to_string(),
        },
    };
    let model = model_id;

    let contents = map_messages_to_contents(&request.body.messages);
    let system_instruction = map_system_to_content(request.body.system);
    let tools = map_tools(request.body.tools);
    let tool_config = map_tool_choice(request.body.tool_choice);
    let generation_config = map_generation_config(
        request.body.max_tokens,
        request.body.temperature,
        request.body.top_p,
        request.body.top_k,
        request.body.stop_sequences,
        request.body.thinking,
        request.body.output_config,
        request.body.output_format,
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

fn map_system_to_content(system: Option<ClaudeSystemParam>) -> Option<GeminiContent> {
    match system {
        Some(ClaudeSystemParam::Text(text)) => text_to_content(text, None),
        Some(ClaudeSystemParam::Blocks(blocks)) => {
            let text = blocks
                .into_iter()
                .map(|block| block.text)
                .collect::<Vec<String>>()
                .join("\n");
            text_to_content(text, None)
        }
        None => None,
    }
}

fn map_messages_to_contents(messages: &[ClaudeMessageParam]) -> Vec<GeminiContent> {
    let mut contents = Vec::new();

    for message in messages {
        if let Some(content) = map_message_to_content(message) {
            contents.push(content);
        }
    }

    contents
}

fn map_message_to_content(message: &ClaudeMessageParam) -> Option<GeminiContent> {
    let role = match message.role {
        ClaudeMessageRole::User => Some(GeminiContentRole::User),
        ClaudeMessageRole::Assistant => Some(GeminiContentRole::Model),
    };

    let parts = map_message_content_to_parts(&message.content);
    if parts.is_empty() {
        None
    } else {
        Some(GeminiContent { parts, role })
    }
}

fn map_message_content_to_parts(content: &ClaudeMessageContent) -> Vec<GeminiPart> {
    match content {
        ClaudeMessageContent::Text(text) => text_to_parts(text),
        ClaudeMessageContent::Blocks(blocks) => {
            blocks.iter().filter_map(map_block_to_part).collect()
        }
    }
}

fn text_to_content(text: String, role: Option<GeminiContentRole>) -> Option<GeminiContent> {
    if text.is_empty() {
        None
    } else {
        Some(GeminiContent {
            parts: text_to_parts(&text),
            role,
        })
    }
}

fn text_to_parts(text: &str) -> Vec<GeminiPart> {
    vec![GeminiPart {
        text: Some(text.to_string()),
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
    }]
}

fn map_block_to_part(block: &ClaudeContentBlockParam) -> Option<GeminiPart> {
    match block {
        ClaudeContentBlockParam::Text(text_block) => Some(GeminiPart {
            text: Some(text_block.text.clone()),
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
        }),
        ClaudeContentBlockParam::Image(image_block) => match &image_block.source {
            ClaudeImageSource::Url { url } => Some(GeminiPart {
                text: None,
                inline_data: None,
                function_call: None,
                function_response: None,
                file_data: Some(GeminiFileData {
                    mime_type: None,
                    file_uri: url.clone(),
                }),
                executable_code: None,
                code_execution_result: None,
                thought: None,
                thought_signature: None,
                part_metadata: None,
                video_metadata: None,
            }),
            ClaudeImageSource::File { file_id } => Some(GeminiPart {
                text: None,
                inline_data: None,
                function_call: None,
                function_response: None,
                file_data: Some(GeminiFileData {
                    mime_type: None,
                    file_uri: file_id.clone(),
                }),
                executable_code: None,
                code_execution_result: None,
                thought: None,
                thought_signature: None,
                part_metadata: None,
                video_metadata: None,
            }),
            ClaudeImageSource::Base64 { data, media_type } => Some(GeminiPart {
                text: None,
                inline_data: Some(GeminiBlob {
                    mime_type: map_image_media_type(media_type),
                    data: data.clone(),
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
            }),
        },
        ClaudeContentBlockParam::Document(document) => map_document_to_part(document),
        _ => None,
    }
}

fn map_document_to_part(document: &ClaudeDocumentBlock) -> Option<GeminiPart> {
    match &document.source {
        ClaudeDocumentSource::Url { url } => Some(GeminiPart {
            text: None,
            inline_data: None,
            function_call: None,
            function_response: None,
            file_data: Some(GeminiFileData {
                mime_type: None,
                file_uri: url.clone(),
            }),
            executable_code: None,
            code_execution_result: None,
            thought: None,
            thought_signature: None,
            part_metadata: None,
            video_metadata: None,
        }),
        ClaudeDocumentSource::File { file_id } => Some(GeminiPart {
            text: None,
            inline_data: None,
            function_call: None,
            function_response: None,
            file_data: Some(GeminiFileData {
                mime_type: None,
                file_uri: file_id.clone(),
            }),
            executable_code: None,
            code_execution_result: None,
            thought: None,
            thought_signature: None,
            part_metadata: None,
            video_metadata: None,
        }),
        ClaudeDocumentSource::Base64 { data, media_type } => Some(GeminiPart {
            text: None,
            inline_data: Some(GeminiBlob {
                mime_type: map_pdf_media_type(media_type),
                data: data.clone(),
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
        }),
        ClaudeDocumentSource::Text { data, .. } => Some(GeminiPart {
            text: Some(data.clone()),
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
        }),
        ClaudeDocumentSource::Content { content } => match content {
            gproxy_protocol::claude::count_tokens::types::BetaContentBlockSourceContent::Text(
                text,
            ) => Some(GeminiPart {
                text: Some(text.clone()),
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
            }),
            gproxy_protocol::claude::count_tokens::types::BetaContentBlockSourceContent::Blocks(
                blocks,
            ) => {
                let text = blocks
                    .iter()
                    .filter_map(|block| match block {
                        ClaudeContentBlockParam::Text(text_block) => Some(text_block.text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<String>>()
                    .join("\n");

                if text.is_empty() {
                    None
                } else {
                    Some(GeminiPart {
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
                    })
                }
            }
        },
    }
}

fn map_image_media_type(media_type: &ClaudeImageMediaType) -> String {
    match media_type {
        ClaudeImageMediaType::ImageJpeg => "image/jpeg",
        ClaudeImageMediaType::ImagePng => "image/png",
        ClaudeImageMediaType::ImageGif => "image/gif",
        ClaudeImageMediaType::ImageWebp => "image/webp",
    }
    .to_string()
}

fn map_pdf_media_type(
    media_type: &gproxy_protocol::claude::count_tokens::types::BetaPdfMediaType,
) -> String {
    match media_type {
        gproxy_protocol::claude::count_tokens::types::BetaPdfMediaType::ApplicationPdf => {
            "application/pdf".to_string()
        }
    }
}

fn map_tools(tools: Option<Vec<ClaudeTool>>) -> Option<Vec<GeminiTool>> {
    let tools = tools?;

    let mut output = Vec::new();
    let mut functions = Vec::new();

    for tool in tools {
        match tool {
            ClaudeTool::Custom(custom) => {
                functions.push(map_custom_tool(custom));
            }
            ClaudeTool::Builtin(builtin) => {
                if let Some(mapped) = map_builtin_tool(builtin) {
                    output.push(mapped);
                }
            }
        }
    }

    if !functions.is_empty() {
        output.push(GeminiTool {
            function_declarations: Some(functions),
            google_search_retrieval: None,
            code_execution: None,
            google_search: None,
            computer_use: None,
            url_context: None,
            file_search: None,
            google_maps: None,
        });
    }

    if output.is_empty() {
        None
    } else {
        Some(output)
    }
}

fn map_custom_tool(tool: ClaudeToolCustom) -> FunctionDeclaration {
    let schema = map_input_schema(tool.input_schema);

    FunctionDeclaration {
        name: tool.name,
        description: tool.description.unwrap_or_default(),
        behavior: None,
        parameters: None,
        parameters_json_schema: schema,
        response: None,
        response_json_schema: None,
    }
}

fn map_input_schema(schema: ClaudeToolInputSchema) -> Option<JsonValue> {
    let mut map = serde_json::Map::new();
    map.insert("type".to_string(), JsonValue::String("object".to_string()));

    if let Some(properties) = schema.properties {
        map.insert(
            "properties".to_string(),
            JsonValue::Object(properties.into_iter().collect()),
        );
    }

    if let Some(required) = schema.required {
        map.insert(
            "required".to_string(),
            JsonValue::Array(required.into_iter().map(JsonValue::String).collect()),
        );
    }

    Some(JsonValue::Object(map))
}

fn map_builtin_tool(builtin: ClaudeToolBuiltin) -> Option<GeminiTool> {
    match builtin {
        ClaudeToolBuiltin::WebSearch20250305(tool) => Some(GeminiTool {
            function_declarations: None,
            google_search_retrieval: None,
            code_execution: None,
            google_search: Some(map_web_search_tool(tool)),
            computer_use: None,
            url_context: None,
            file_search: None,
            google_maps: None,
        }),
        ClaudeToolBuiltin::CodeExecution20250522(_)
        | ClaudeToolBuiltin::CodeExecution20250825(_) => Some(GeminiTool {
            function_declarations: None,
            google_search_retrieval: None,
            code_execution: Some(CodeExecution {}),
            google_search: None,
            computer_use: None,
            url_context: None,
            file_search: None,
            google_maps: None,
        }),
        ClaudeToolBuiltin::ComputerUse20241022(_)
        | ClaudeToolBuiltin::ComputerUse20250124(_)
        | ClaudeToolBuiltin::ComputerUse20251124(_) => Some(GeminiTool {
            function_declarations: None,
            google_search_retrieval: None,
            code_execution: None,
            google_search: None,
            computer_use: Some(
                gproxy_protocol::gemini::generate_content::types::ComputerUse {
                    environment: Environment::EnvironmentBrowser,
                    excluded_predefined_functions: None,
                },
            ),
            url_context: None,
            file_search: None,
            google_maps: None,
        }),
        ClaudeToolBuiltin::ToolSearchToolBm25(_tool)
        | ClaudeToolBuiltin::ToolSearchToolBm2520251119(_tool) => Some(GeminiTool {
            function_declarations: None,
            google_search_retrieval: None,
            code_execution: None,
            google_search: None,
            computer_use: None,
            url_context: None,
            file_search: Some(FileSearch {
                file_search_store_names: Vec::new(),
                metadata_filter: None,
                top_k: None,
            }),
            google_maps: None,
        }),
        ClaudeToolBuiltin::McpToolset(_)
        | ClaudeToolBuiltin::Bash20241022(_)
        | ClaudeToolBuiltin::Bash20250124(_)
        | ClaudeToolBuiltin::TextEditor20241022(_)
        | ClaudeToolBuiltin::TextEditor20250124(_)
        | ClaudeToolBuiltin::TextEditor20250429(_)
        | ClaudeToolBuiltin::TextEditor20250728(_)
        | ClaudeToolBuiltin::Memory20250818(_)
        | ClaudeToolBuiltin::WebFetch20250910(_)
        | ClaudeToolBuiltin::ToolSearchToolRegex(_)
        | ClaudeToolBuiltin::ToolSearchToolRegex20251119(_) => None,
    }
}

fn map_web_search_tool(_tool: ClaudeWebSearchTool) -> GoogleSearch {
    GoogleSearch {
        time_range_filter: None,
    }
}

fn map_tool_choice(choice: Option<ClaudeToolChoice>) -> Option<ToolConfig> {
    let choice = choice?;

    let function_calling_config = match choice {
        ClaudeToolChoice::None => FunctionCallingConfig {
            mode: Some(FunctionCallingMode::None),
            allowed_function_names: None,
        },
        ClaudeToolChoice::Auto { .. } => FunctionCallingConfig {
            mode: Some(FunctionCallingMode::Auto),
            allowed_function_names: None,
        },
        ClaudeToolChoice::Any { .. } => FunctionCallingConfig {
            mode: Some(FunctionCallingMode::Any),
            allowed_function_names: None,
        },
        ClaudeToolChoice::Tool { name, .. } => FunctionCallingConfig {
            mode: Some(FunctionCallingMode::Any),
            allowed_function_names: Some(vec![name]),
        },
    };

    Some(ToolConfig {
        function_calling_config: Some(function_calling_config),
        retrieval_config: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn map_generation_config(
    max_tokens: u32,
    temperature: Option<f64>,
    top_p: Option<f64>,
    top_k: Option<u32>,
    stop_sequences: Option<Vec<String>>,
    thinking: Option<ClaudeThinkingConfigParam>,
    output_config: Option<ClaudeOutputConfig>,
    output_format: Option<ClaudeJSONOutputFormat>,
) -> Option<GenerationConfig> {
    let thinking_config = map_thinking_config(thinking, output_config.as_ref());
    let response_json_schema = output_format.map(|format| format.schema);
    let response_mime_type = response_json_schema
        .as_ref()
        .map(|_| "application/json".to_string());

    let has_config = thinking_config.is_some()
        || response_json_schema.is_some()
        || stop_sequences.is_some()
        || max_tokens > 0
        || temperature.is_some()
        || top_p.is_some()
        || top_k.is_some();

    if !has_config {
        return None;
    }

    Some(GenerationConfig {
        stop_sequences,
        response_mime_type,
        response_schema: None,
        response_json_schema_internal: None,
        response_json_schema,
        response_modalities: None,
        candidate_count: None,
        max_output_tokens: if max_tokens > 0 {
            Some(max_tokens)
        } else {
            None
        },
        temperature,
        top_p,
        top_k,
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

fn map_thinking_config(
    thinking: Option<ClaudeThinkingConfigParam>,
    output_config: Option<&ClaudeOutputConfig>,
) -> Option<ThinkingConfig> {
    let effort = output_config
        .and_then(|config| config.effort)
        .and_then(map_effort_to_thinking_level);

    match thinking {
        Some(ClaudeThinkingConfigParam::Enabled { budget_tokens }) => Some(ThinkingConfig {
            include_thoughts: true,
            thinking_budget: budget_tokens,
            thinking_level: effort,
        }),
        Some(ClaudeThinkingConfigParam::Disabled) => Some(ThinkingConfig {
            include_thoughts: false,
            thinking_budget: 0,
            thinking_level: effort,
        }),
        None => effort.map(|level| ThinkingConfig {
            include_thoughts: true,
            thinking_budget: 0,
            thinking_level: Some(level),
        }),
    }
}

fn map_effort_to_thinking_level(effort: ClaudeOutputEffort) -> Option<ThinkingLevel> {
    match effort {
        ClaudeOutputEffort::Low => Some(ThinkingLevel::Low),
        ClaudeOutputEffort::Medium => Some(ThinkingLevel::Medium),
        ClaudeOutputEffort::High => Some(ThinkingLevel::High),
    }
}
