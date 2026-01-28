use gproxy_protocol::claude::count_tokens::request::{
    CountTokensHeaders as ClaudeCountTokensHeaders, CountTokensRequest as ClaudeCountTokensRequest,
    CountTokensRequestBody as ClaudeCountTokensRequestBody,
};
use gproxy_protocol::claude::count_tokens::types::{
    BetaContentBlockParam as ClaudeContentBlockParam,
    BetaDocumentBlockType as ClaudeDocumentBlockType, BetaDocumentSource as ClaudeDocumentSource,
    BetaImageBlockParam as ClaudeImageBlockParam, BetaImageBlockType as ClaudeImageBlockType,
    BetaImageMediaType as ClaudeImageMediaType, BetaImageSource as ClaudeImageSource,
    BetaJSONOutputFormat as ClaudeJSONOutputFormat,
    BetaJSONOutputFormatType as ClaudeJSONOutputFormatType,
    BetaMessageContent as ClaudeMessageContent, BetaMessageParam as ClaudeMessageParam,
    BetaMessageRole as ClaudeMessageRole, BetaOutputConfig as ClaudeOutputConfig,
    BetaOutputEffort as ClaudeOutputEffort, BetaPdfMediaType as ClaudePdfMediaType,
    BetaRequestDocumentBlock as ClaudeDocumentBlock, BetaSystemParam as ClaudeSystemParam,
    BetaTextBlockParam as ClaudeTextBlockParam, BetaTextBlockType as ClaudeTextBlockType,
    BetaThinkingConfigParam as ClaudeThinkingConfigParam, BetaTool as ClaudeTool,
    BetaToolBuiltin as ClaudeToolBuiltin, BetaToolChoice as ClaudeToolChoice,
    BetaToolCodeExecution as ClaudeToolCodeExecution, BetaToolComputerUse as ClaudeToolComputerUse,
    BetaToolCustom as ClaudeToolCustom, BetaToolCustomType as ClaudeToolCustomType,
    BetaToolInputSchema as ClaudeToolInputSchema,
    BetaToolInputSchemaType as ClaudeToolInputSchemaType,
    BetaToolSearchTool as ClaudeToolSearchTool, BetaWebSearchTool as ClaudeWebSearchTool,
    Model as ClaudeModel,
};
use gproxy_protocol::gemini::count_tokens::request::CountTokensRequest as GeminiCountTokensRequest;
use gproxy_protocol::gemini::count_tokens::types::{
    Blob as GeminiBlob, Content as GeminiContent, ContentRole as GeminiContentRole,
    FileData as GeminiFileData, Part as GeminiPart,
};
use gproxy_protocol::gemini::generate_content::request::GenerateContentRequestBody;
use gproxy_protocol::gemini::generate_content::types::{
    FunctionCallingMode, FunctionDeclaration, GenerationConfig, ThinkingLevel, Tool as GeminiTool,
    ToolConfig,
};
use serde_json::Value as JsonValue;

/// Convert a Gemini count-tokens request into a Claude count-tokens request.
pub fn transform_request(request: GeminiCountTokensRequest) -> ClaudeCountTokensRequest {
    let model_id = request
        .path
        .model
        .strip_prefix("models/")
        .unwrap_or(&request.path.model)
        .to_string();

    let mut contents = request.body.contents.unwrap_or_default();
    let mut system_instruction = None;
    let mut tools = None;
    let mut tool_config = None;
    let mut generation_config = None;

    if let Some(generate_content_request) = request.body.generate_content_request
        && let Ok(body) =
            serde_json::from_value::<GenerateContentRequestBody>(generate_content_request)
    {
        contents = body.contents;
        system_instruction = body.system_instruction;
        tools = body.tools;
        tool_config = body.tool_config;
        generation_config = body.generation_config;
    }

    let messages = map_contents_to_messages(&contents);
    let system = map_system_instruction(system_instruction);
    let tools = tools
        .map(map_tools)
        .and_then(|tools| if tools.is_empty() { None } else { Some(tools) });
    let tool_choice = map_tool_choice(tool_config);
    let (thinking, output_config, output_format) = map_generation_config(generation_config);

    ClaudeCountTokensRequest {
        headers: ClaudeCountTokensHeaders::default(),
        body: ClaudeCountTokensRequestBody {
            messages,
            model: ClaudeModel::Custom(model_id),
            system,
            tools,
            tool_choice,
            thinking,
            output_config,
            output_format,
            context_management: None,
            mcp_servers: None,
        },
    }
}

fn map_contents_to_messages(contents: &[GeminiContent]) -> Vec<ClaudeMessageParam> {
    let mut messages = Vec::new();
    for content in contents {
        if let Some(message) = map_content_to_message(content) {
            messages.push(message);
        }
    }
    messages
}

fn map_content_to_message(content: &GeminiContent) -> Option<ClaudeMessageParam> {
    let role = match content.role {
        Some(GeminiContentRole::Model) => ClaudeMessageRole::Assistant,
        _ => ClaudeMessageRole::User,
    };

    let blocks = map_parts_to_blocks(&content.parts);
    if blocks.is_empty() {
        return None;
    }

    let message_content = if blocks.len() == 1 {
        if let ClaudeContentBlockParam::Text(text_block) = &blocks[0] {
            ClaudeMessageContent::Text(text_block.text.clone())
        } else {
            ClaudeMessageContent::Blocks(blocks)
        }
    } else {
        ClaudeMessageContent::Blocks(blocks)
    };

    Some(ClaudeMessageParam {
        role,
        content: message_content,
    })
}

fn map_parts_to_blocks(parts: &[GeminiPart]) -> Vec<ClaudeContentBlockParam> {
    let mut blocks = Vec::new();
    for part in parts {
        blocks.extend(map_part_to_blocks(part));
    }
    blocks
}

fn map_part_to_blocks(part: &GeminiPart) -> Vec<ClaudeContentBlockParam> {
    let mut blocks = Vec::new();

    if let Some(text) = part.text.clone() {
        push_text_block(&mut blocks, text);
    }

    if let Some(blob) = &part.inline_data
        && let Some(block) = map_inline_blob(blob)
    {
        blocks.push(block);
    }

    if let Some(file) = &part.file_data
        && let Some(block) = map_file_data(file)
    {
        blocks.push(block);
    }

    if let Some(function_call) = &part.function_call {
        push_json_block(&mut blocks, "function_call", function_call);
    }

    if let Some(function_response) = &part.function_response {
        push_json_block(&mut blocks, "function_response", function_response);
    }

    if let Some(code) = &part.executable_code {
        push_json_block(&mut blocks, "executable_code", code);
    }

    if let Some(result) = &part.code_execution_result {
        push_json_block(&mut blocks, "code_execution_result", result);
    }

    blocks
}

fn map_inline_blob(blob: &GeminiBlob) -> Option<ClaudeContentBlockParam> {
    if blob.mime_type.starts_with("image/") {
        let media_type = match blob.mime_type.as_str() {
            "image/jpeg" => Some(ClaudeImageMediaType::ImageJpeg),
            "image/png" => Some(ClaudeImageMediaType::ImagePng),
            "image/gif" => Some(ClaudeImageMediaType::ImageGif),
            "image/webp" => Some(ClaudeImageMediaType::ImageWebp),
            _ => None,
        }?;

        return Some(ClaudeContentBlockParam::Image(ClaudeImageBlockParam {
            source: ClaudeImageSource::Base64 {
                data: blob.data.clone(),
                media_type,
            },
            r#type: ClaudeImageBlockType::Image,
            cache_control: None,
        }));
    }

    if blob.mime_type == "application/pdf" {
        return Some(ClaudeContentBlockParam::Document(ClaudeDocumentBlock {
            source: ClaudeDocumentSource::Base64 {
                data: blob.data.clone(),
                media_type: ClaudePdfMediaType::ApplicationPdf,
            },
            r#type: ClaudeDocumentBlockType::Document,
            cache_control: None,
            citations: None,
            context: None,
            title: None,
        }));
    }

    None
}

fn map_file_data(file: &GeminiFileData) -> Option<ClaudeContentBlockParam> {
    if let Some(mime_type) = &file.mime_type
        && mime_type.starts_with("image/")
    {
        return Some(ClaudeContentBlockParam::Image(ClaudeImageBlockParam {
            source: ClaudeImageSource::Url {
                url: file.file_uri.clone(),
            },
            r#type: ClaudeImageBlockType::Image,
            cache_control: None,
        }));
    }

    Some(ClaudeContentBlockParam::Document(ClaudeDocumentBlock {
        source: ClaudeDocumentSource::Url {
            url: file.file_uri.clone(),
        },
        r#type: ClaudeDocumentBlockType::Document,
        cache_control: None,
        citations: None,
        context: None,
        title: None,
    }))
}

fn map_system_instruction(system: Option<GeminiContent>) -> Option<ClaudeSystemParam> {
    let system = system?;
    let texts: Vec<String> = system
        .parts
        .iter()
        .filter_map(|part| part.text.clone())
        .collect();

    if texts.is_empty() {
        None
    } else {
        Some(ClaudeSystemParam::Text(texts.join("\n")))
    }
}

fn push_text_block(blocks: &mut Vec<ClaudeContentBlockParam>, text: String) {
    if text.is_empty() {
        return;
    }
    blocks.push(ClaudeContentBlockParam::Text(ClaudeTextBlockParam {
        text,
        r#type: ClaudeTextBlockType::Text,
        cache_control: None,
        citations: None,
    }));
}

fn push_json_block<T: serde::Serialize>(
    blocks: &mut Vec<ClaudeContentBlockParam>,
    label: &str,
    value: &T,
) {
    if let Ok(json) = serde_json::to_string(value) {
        push_text_block(blocks, format!("{label}: {json}"));
    }
}

fn map_tools(tools: Vec<GeminiTool>) -> Vec<ClaudeTool> {
    let mut output = Vec::new();

    for tool in tools {
        if let Some(functions) = tool.function_declarations {
            for function in functions {
                output.push(ClaudeTool::Custom(map_function_declaration(function)));
            }
        }

        if tool.code_execution.is_some() {
            output.push(ClaudeTool::Builtin(
                ClaudeToolBuiltin::CodeExecution20250522(ClaudeToolCodeExecution {
                    name: "code_execution".to_string(),
                    allowed_callers: None,
                    cache_control: None,
                    defer_loading: None,
                    strict: None,
                }),
            ));
        }

        if tool.google_search.is_some() || tool.google_search_retrieval.is_some() {
            output.push(ClaudeTool::Builtin(ClaudeToolBuiltin::WebSearch20250305(
                ClaudeWebSearchTool {
                    name: "web_search".to_string(),
                    allowed_callers: None,
                    allowed_domains: None,
                    blocked_domains: None,
                    cache_control: None,
                    defer_loading: None,
                    max_uses: None,
                    strict: None,
                    user_location: None,
                },
            )));
        }

        if tool.computer_use.is_some() {
            output.push(ClaudeTool::Builtin(ClaudeToolBuiltin::ComputerUse20241022(
                ClaudeToolComputerUse {
                    display_height_px: 768,
                    display_width_px: 1024,
                    name: "computer".to_string(),
                    allowed_callers: None,
                    cache_control: None,
                    defer_loading: None,
                    display_number: None,
                    enable_zoom: None,
                    input_examples: None,
                    strict: None,
                },
            )));
        }

        if tool.file_search.is_some() {
            output.push(ClaudeTool::Builtin(ClaudeToolBuiltin::ToolSearchToolBm25(
                ClaudeToolSearchTool {
                    name: "file_search".to_string(),
                    allowed_callers: None,
                    cache_control: None,
                    defer_loading: None,
                    strict: None,
                },
            )));
        }
    }

    output
}

fn map_function_declaration(function: FunctionDeclaration) -> ClaudeToolCustom {
    let input_schema = if let Some(schema) = function.parameters_json_schema {
        json_schema_to_input_schema(schema)
    } else if let Some(schema) = function.parameters {
        json_schema_to_input_schema(schema_to_json(schema))
    } else {
        ClaudeToolInputSchema {
            r#type: ClaudeToolInputSchemaType::Object,
            properties: None,
            required: None,
        }
    };

    ClaudeToolCustom {
        input_schema,
        name: function.name,
        allowed_callers: None,
        cache_control: None,
        defer_loading: None,
        description: Some(function.description),
        input_examples: None,
        strict: None,
        r#type: Some(ClaudeToolCustomType::Custom),
    }
}

fn json_schema_to_input_schema(schema: JsonValue) -> ClaudeToolInputSchema {
    let properties = schema
        .get("properties")
        .and_then(|value| value.as_object())
        .map(|map| map.clone().into_iter().collect());

    let required = schema
        .get("required")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(|value| value.to_string()))
                .collect::<Vec<String>>()
        });

    ClaudeToolInputSchema {
        r#type: ClaudeToolInputSchemaType::Object,
        properties,
        required,
    }
}

fn schema_to_json(schema: gproxy_protocol::gemini::generate_content::types::Schema) -> JsonValue {
    use gproxy_protocol::gemini::generate_content::types::Type as GeminiType;

    let mut map = serde_json::Map::new();
    let schema_type = match schema.r#type {
        GeminiType::String => "string",
        GeminiType::Number => "number",
        GeminiType::Integer => "integer",
        GeminiType::Boolean => "boolean",
        GeminiType::Array => "array",
        GeminiType::Object => "object",
        GeminiType::Null => "null",
        _ => "object",
    };
    map.insert(
        "type".to_string(),
        JsonValue::String(schema_type.to_string()),
    );

    if let Some(description) = schema.description {
        map.insert("description".to_string(), JsonValue::String(description));
    }

    if let Some(properties) = schema.properties {
        let mut props = serde_json::Map::new();
        for (key, value) in properties {
            props.insert(key, schema_to_json(value));
        }
        map.insert("properties".to_string(), JsonValue::Object(props));
    }

    if let Some(required) = schema.required {
        map.insert(
            "required".to_string(),
            JsonValue::Array(required.into_iter().map(JsonValue::String).collect()),
        );
    }

    if let Some(items) = schema.items {
        map.insert("items".to_string(), schema_to_json(*items));
    }

    if let Some(enum_values) = schema.enum_values {
        map.insert(
            "enum".to_string(),
            JsonValue::Array(enum_values.into_iter().map(JsonValue::String).collect()),
        );
    }

    JsonValue::Object(map)
}

fn map_tool_choice(tool_config: Option<ToolConfig>) -> Option<ClaudeToolChoice> {
    let config = tool_config?.function_calling_config?;

    let mode = config.mode.unwrap_or(FunctionCallingMode::ModeUnspecified);
    match mode {
        FunctionCallingMode::None => Some(ClaudeToolChoice::None),
        FunctionCallingMode::Auto | FunctionCallingMode::ModeUnspecified => {
            Some(ClaudeToolChoice::Auto {
                disable_parallel_tool_use: None,
            })
        }
        FunctionCallingMode::Any | FunctionCallingMode::Validated => {
            if let Some(names) = config.allowed_function_names
                && names.len() == 1
            {
                return Some(ClaudeToolChoice::Tool {
                    name: names[0].clone(),
                    disable_parallel_tool_use: None,
                });
            }
            Some(ClaudeToolChoice::Any {
                disable_parallel_tool_use: None,
            })
        }
    }
}

fn map_generation_config(
    generation_config: Option<GenerationConfig>,
) -> (
    Option<ClaudeThinkingConfigParam>,
    Option<ClaudeOutputConfig>,
    Option<ClaudeJSONOutputFormat>,
) {
    let config = match generation_config {
        Some(config) => config,
        None => return (None, None, None),
    };

    let output_effort = config
        .thinking_config
        .as_ref()
        .and_then(|thinking| thinking.thinking_level)
        .and_then(map_thinking_level_to_effort);

    let output_config = output_effort.map(|effort| ClaudeOutputConfig {
        effort: Some(effort),
    });

    let thinking = config.thinking_config.as_ref().map(|thinking| {
        if thinking.include_thoughts {
            ClaudeThinkingConfigParam::Enabled {
                budget_tokens: thinking.thinking_budget,
            }
        } else {
            ClaudeThinkingConfigParam::Disabled
        }
    });

    let output_format = config
        .response_json_schema
        .or(config.response_json_schema_internal)
        .map(|schema| ClaudeJSONOutputFormat {
            schema,
            r#type: ClaudeJSONOutputFormatType::JsonSchema,
        })
        .or_else(|| {
            config.response_schema.map(|schema| ClaudeJSONOutputFormat {
                schema: schema_to_json(schema),
                r#type: ClaudeJSONOutputFormatType::JsonSchema,
            })
        });

    (thinking, output_config, output_format)
}

fn map_thinking_level_to_effort(level: ThinkingLevel) -> Option<ClaudeOutputEffort> {
    match level {
        ThinkingLevel::Minimal | ThinkingLevel::Low => Some(ClaudeOutputEffort::Low),
        ThinkingLevel::Medium => Some(ClaudeOutputEffort::Medium),
        ThinkingLevel::High => Some(ClaudeOutputEffort::High),
        _ => None,
    }
}
