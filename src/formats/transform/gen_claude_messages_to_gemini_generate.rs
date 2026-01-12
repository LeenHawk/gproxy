use crate::formats::{claude, gemini};
use serde_json::Value;
use super::TransformError;

fn text_part(text: String) -> gemini::types::Part {
    gemini::types::Part {
        data: gemini::types::PartData::Text { text },
        thought: None,
        thought_signature: None,
        part_metadata: None,
        video_metadata: None,
    }
}

fn thought_part(text: String, signature: Option<String>) -> gemini::types::Part {
    gemini::types::Part {
        data: gemini::types::PartData::Text { text },
        thought: Some(true),
        thought_signature: signature,
        part_metadata: None,
        video_metadata: None,
    }
}

fn inline_part(mime_type: String, data: String) -> gemini::types::Part {
    gemini::types::Part {
        data: gemini::types::PartData::InlineData {
            inline_data: gemini::types::Blob { mime_type, data },
        },
        thought: None,
        thought_signature: None,
        part_metadata: None,
        video_metadata: None,
    }
}

fn function_call_part(
    id: Option<String>,
    name: String,
    args: Option<gemini::types::JsonStruct>,
) -> gemini::types::Part {
    gemini::types::Part {
        data: gemini::types::PartData::FunctionCall {
            function_call: gemini::types::FunctionCall { id, name, args },
        },
        thought: None,
        thought_signature: None,
        part_metadata: None,
        video_metadata: None,
    }
}

fn function_response_part(
    id: Option<String>,
    name: String,
    response: gemini::types::JsonStruct,
) -> gemini::types::Part {
    gemini::types::Part {
        data: gemini::types::PartData::FunctionResponse {
            function_response: gemini::types::FunctionResponse {
                id,
                name,
                response,
                parts: None,
                will_continue: None,
                scheduling: None,
            },
        },
        thought: None,
        thought_signature: None,
        part_metadata: None,
        video_metadata: None,
    }
}

fn tool_result_to_output(content: Option<claude::types::ToolResultContentParam>) -> String {
    match content {
        Some(claude::types::ToolResultContentParam::Text(text)) => text,
        Some(claude::types::ToolResultContentParam::Blocks(blocks)) => blocks
            .into_iter()
            .filter_map(|block| match block {
                claude::types::ToolResultContentBlockParam::Text(text_block) => {
                    Some(text_block.text)
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        None => String::new(),
    }
}

fn message_content_to_parts(content: claude::types::MessageContent) -> Vec<gemini::types::Part> {
    match content {
        claude::types::MessageContent::Text(text) => vec![text_part(text)],
        claude::types::MessageContent::Blocks(blocks) => blocks
            .into_iter()
            .filter_map(|block| match block {
                claude::types::BetaContentBlockParam::Text(text_block) => {
                    Some(text_part(text_block.text))
                }
                claude::types::BetaContentBlockParam::Image(image_block) => match image_block.source {
                    claude::types::BetaImageSource::Base64(base64) => Some(inline_part(
                        match base64.media_type {
                            claude::types::ImageMediaType::Jpeg => "image/jpeg".to_string(),
                            claude::types::ImageMediaType::Png => "image/png".to_string(),
                            claude::types::ImageMediaType::Gif => "image/gif".to_string(),
                            claude::types::ImageMediaType::Webp => "image/webp".to_string(),
                        },
                        base64.data,
                    )),
                    claude::types::BetaImageSource::Url(url) => Some(text_part(url.url)),
                    claude::types::BetaImageSource::File(file) => Some(text_part(file.file_id)),
                },
                claude::types::BetaContentBlockParam::Thinking(thinking) => {
                    Some(thought_part(thinking.thinking, Some(thinking.signature)))
                }
                claude::types::BetaContentBlockParam::RedactedThinking(thinking) => {
                    Some(thought_part(thinking.data, None))
                }
                claude::types::BetaContentBlockParam::ToolUse(tool_use) => {
                    Some(function_call_part(
                        Some(tool_use.id),
                        tool_use.name,
                        Some(tool_use.input.into_iter().collect()),
                    ))
                }
                claude::types::BetaContentBlockParam::ToolResult(tool_result) => {
                    let output = tool_result_to_output(tool_result.content);
                    let mut response = gemini::types::JsonStruct::new();
                    response.insert("output".to_string(), Value::String(output));
                    Some(function_response_part(
                        Some(tool_result.tool_use_id),
                        "tool_result".to_string(),
                        response,
                    ))
                }
                _ => None,
            })
            .collect(),
    }
}

fn system_prompt_to_parts(prompt: claude::types::SystemPrompt) -> Vec<gemini::types::TextPart> {
    match prompt {
        claude::types::SystemPrompt::Text(text) => vec![gemini::types::TextPart { text }],
        claude::types::SystemPrompt::Blocks(blocks) => blocks
            .into_iter()
            .map(|block| gemini::types::TextPart { text: block.text })
            .collect(),
    }
}

fn build_function_declarations(
    tools: &Option<Vec<claude::types::BetaToolUnion>>,
) -> Vec<gemini::types::FunctionDeclaration> {
    let mut declarations = Vec::new();
    if let Some(tools) = tools {
        for tool in tools {
            if let claude::types::BetaToolUnion::Tool(custom_tool) = tool {
                let schema = serde_json::to_value(&custom_tool.input_schema).ok();
                declarations.push(gemini::types::FunctionDeclaration {
                    name: custom_tool.name.clone(),
                    description: custom_tool.description.clone().unwrap_or_default(),
                    behavior: None,
                    parameters: None,
                    parameters_json_schema: schema,
                    response: None,
                    response_json_schema: None,
                });
            }
        }
    }
    declarations
}

fn tool_choice_to_config(choice: &claude::types::BetaToolChoice) -> gemini::types::ToolConfig {
    let mut config = gemini::types::ToolConfig {
        function_calling_config: None,
        retrieval_config: None,
    };

    let mut allowed = None;
    let mode = match choice {
        claude::types::BetaToolChoice::Auto { .. } => gemini::types::FunctionCallingMode::Auto,
        claude::types::BetaToolChoice::Any { .. } => gemini::types::FunctionCallingMode::Any,
        claude::types::BetaToolChoice::Tool { name, .. } => {
            allowed = Some(vec![name.clone()]);
            gemini::types::FunctionCallingMode::Any
        }
        claude::types::BetaToolChoice::None => gemini::types::FunctionCallingMode::None,
    };

    config.function_calling_config = Some(gemini::types::FunctionCallingConfig {
        mode: Some(mode),
        allowed_function_names: allowed,
    });

    config
}

fn generation_config_from_claude(
    request: &claude::messages::MessageCreateRequest,
) -> Option<gemini::types::GenerationConfig> {
    let mut config = gemini::types::GenerationConfig {
        stop_sequences: request.stop_sequences.clone(),
        response_mime_type: None,
        response_schema: None,
        response_json_schema_internal: None,
        response_json_schema: None,
        response_modalities: None,
        candidate_count: None,
        max_output_tokens: Some(request.max_tokens as i64),
        temperature: request.temperature.map(|value| value.get()),
        top_p: request.top_p.map(|value| value.get()),
        top_k: request.top_k.map(i64::from),
        seed: None,
        presence_penalty: None,
        frequency_penalty: None,
        response_logprobs: None,
        logprobs: None,
        enable_enhanced_civic_answers: None,
        speech_config: None,
        thinking_config: None,
        image_config: None,
        media_resolution: None,
    };

    if let Some(thinking) = &request.thinking {
        let thinking_config = match thinking {
            claude::types::BetaThinkingConfigParam::Enabled { budget_tokens } => {
                gemini::types::ThinkingConfig {
                    include_thoughts: Some(true),
                    thinking_budget: Some(budget_tokens.get() as i64),
                    thinking_level: None,
                }
            }
            claude::types::BetaThinkingConfigParam::Disabled => gemini::types::ThinkingConfig {
                include_thoughts: Some(false),
                thinking_budget: None,
                thinking_level: None,
            },
        };
        config.thinking_config = Some(thinking_config);
    }

    Some(config)
}

pub fn request(
    request: claude::messages::MessageCreateRequest,
) -> Result<gemini::generate_content::GenerateContentRequest, TransformError> {
    let mut contents = Vec::new();
    let generation_config = generation_config_from_claude(&request);
    let system_instruction = request
        .system
        .clone()
        .map(system_prompt_to_parts)
        .map(|parts| gemini::types::SystemInstruction { parts });
    let declarations = build_function_declarations(&request.tools);
    let tool_config = request.tool_choice.as_ref().map(tool_choice_to_config);

    for message in request.messages.0 {
        let role = match message.role {
            claude::types::MessageRole::User => gemini::types::ContentRole::User,
            claude::types::MessageRole::Assistant => gemini::types::ContentRole::Model,
        };
        let parts = message_content_to_parts(message.content);
        if !parts.is_empty() {
            contents.push(gemini::types::Content {
                role: Some(role),
                parts,
            });
        }
    }

    if contents.is_empty() {
        contents.push(gemini::types::Content {
            role: Some(gemini::types::ContentRole::User),
            parts: vec![text_part("Please answer according to system instructions.".to_string())],
        });
    }

    let tools = if declarations.is_empty() {
        None
    } else {
        Some(vec![gemini::types::Tool {
            function_declarations: Some(declarations),
            google_search_retrieval: None,
            code_execution: None,
            google_search: None,
            computer_use: None,
            url_context: None,
            file_search: None,
            google_maps: None,
        }])
    };

    Ok(gemini::generate_content::GenerateContentRequest {
        contents,
        tools,
        tool_config,
        safety_settings: None,
        system_instruction,
        generation_config,
        cached_content: None,
    })
}
