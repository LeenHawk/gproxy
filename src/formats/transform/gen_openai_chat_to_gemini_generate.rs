use crate::formats::{gemini, openai};
use crate::formats::openai::types::JsonValue as OpenAiJsonValue;
use serde_json::Value;
use std::collections::HashMap;

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

fn parse_data_url(url: &str) -> Option<(String, String)> {
    if !url.starts_with("data:") {
        return None;
    }
    let (meta, data) = url.split_once(',')?;
    let meta = meta.trim_start_matches("data:");
    let mime_type = meta.strip_suffix(";base64")?.to_string();
    Some((mime_type, data.to_string()))
}

fn text_content_to_string(content: &openai::chat_completions::ChatCompletionTextContent) -> String {
    match content {
        openai::chat_completions::ChatCompletionTextContent::Text(text) => text.clone(),
        openai::chat_completions::ChatCompletionTextContent::Parts(parts) => parts
            .iter()
            .map(|part| part.text.clone())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn user_content_to_parts(
    content: openai::chat_completions::ChatCompletionUserContent,
) -> Vec<gemini::types::Part> {
    match content {
        openai::chat_completions::ChatCompletionUserContent::Text(text) => {
            vec![text_part(text)]
        }
        openai::chat_completions::ChatCompletionUserContent::Parts(parts) => parts
            .into_iter()
            .filter_map(|part| match part {
                openai::chat_completions::ChatCompletionRequestUserContentPart::Text { text } => {
                    Some(text_part(text))
                }
                openai::chat_completions::ChatCompletionRequestUserContentPart::ImageUrl {
                    image_url,
                } => parse_data_url(&image_url.url)
                    .map(|(mime, data)| inline_part(mime, data))
                    .or_else(|| Some(text_part(image_url.url))),
                _ => None,
            })
            .collect(),
    }
}

fn assistant_content_to_parts(
    content: openai::chat_completions::ChatCompletionAssistantContent,
) -> Vec<gemini::types::Part> {
    match content {
        openai::chat_completions::ChatCompletionAssistantContent::Text(text) => {
            vec![text_part(text)]
        }
        openai::chat_completions::ChatCompletionAssistantContent::Parts(parts) => parts
            .into_iter()
            .map(|part| match part {
                openai::chat_completions::ChatCompletionRequestAssistantContentPart::Text { text } => {
                    text_part(text)
                }
                openai::chat_completions::ChatCompletionRequestAssistantContentPart::Refusal {
                    refusal,
                } => text_part(refusal),
            })
            .collect(),
    }
}

fn json_struct_from_value(value: Value) -> gemini::types::JsonStruct {
    match value {
        Value::Object(map) => map,
        other => {
            let mut map = gemini::types::JsonStruct::new();
            map.insert("result".to_string(), other);
            map
        }
    }
}

fn parse_args_to_struct(args: &str) -> gemini::types::JsonStruct {
    match serde_json::from_str::<Value>(args) {
        Ok(value) => json_struct_from_value(value),
        Err(_) => gemini::types::JsonStruct::new(),
    }
}

fn build_function_declarations(
    tools: &Option<Vec<openai::chat_completions::ChatCompletionTool>>,
    functions: &Option<Vec<openai::chat_completions::ChatCompletionFunction>>,
) -> Vec<gemini::types::FunctionDeclaration> {
    let mut declarations = Vec::new();

    if let Some(tools) = tools {
        for tool in tools {
            if let openai::chat_completions::ChatCompletionTool::Function { function } = tool {
                let parameters_json_schema = function
                    .parameters
                    .as_ref()
                    .and_then(|schema| serde_json::to_value(schema).ok());
                declarations.push(gemini::types::FunctionDeclaration {
                    name: function.name.clone(),
                    description: function.description.clone().unwrap_or_default(),
                    behavior: None,
                    parameters: None,
                    parameters_json_schema,
                    response: None,
                    response_json_schema: None,
                });
            }
        }
    }

    if let Some(functions) = functions {
        for function in functions {
            let parameters_json_schema = function
                .parameters
                .as_ref()
                .and_then(|schema| serde_json::to_value(schema).ok());
            declarations.push(gemini::types::FunctionDeclaration {
                name: function.name.clone(),
                description: function.description.clone().unwrap_or_default(),
                behavior: None,
                parameters: None,
                parameters_json_schema,
                response: None,
                response_json_schema: None,
            });
        }
    }

    declarations
}

fn tool_choice_to_config(
    choice: &openai::chat_completions::ChatCompletionToolChoiceOption,
) -> gemini::types::ToolConfig {
    use openai::chat_completions::ChatCompletionToolChoiceMode;

    let mut config = gemini::types::ToolConfig {
        function_calling_config: None,
        retrieval_config: None,
    };

    let (mode, allowed) = match choice {
        openai::chat_completions::ChatCompletionToolChoiceOption::Mode(mode_value) => (
            Some(match mode_value {
                ChatCompletionToolChoiceMode::Auto => gemini::types::FunctionCallingMode::Auto,
                ChatCompletionToolChoiceMode::None => gemini::types::FunctionCallingMode::None,
                ChatCompletionToolChoiceMode::Required => gemini::types::FunctionCallingMode::Any,
            }),
            None,
        ),
        openai::chat_completions::ChatCompletionToolChoiceOption::NamedTool(named) => (
            Some(gemini::types::FunctionCallingMode::Any),
            Some(vec![named.function.name.clone()]),
        ),
        openai::chat_completions::ChatCompletionToolChoiceOption::NamedCustom(custom) => (
            Some(gemini::types::FunctionCallingMode::Any),
            Some(vec![custom.custom.name.clone()]),
        ),
        openai::chat_completions::ChatCompletionToolChoiceOption::AllowedTools(allowed_tools) => {
            let mode = match allowed_tools.allowed_tools.mode {
                openai::chat_completions::ChatCompletionAllowedToolsMode::Auto => {
                    gemini::types::FunctionCallingMode::Auto
                }
                openai::chat_completions::ChatCompletionAllowedToolsMode::Required => {
                    gemini::types::FunctionCallingMode::Any
                }
            };
            let mut names = Vec::new();
            for entry in &allowed_tools.allowed_tools.tools {
                if let Some(OpenAiJsonValue::String(tool_type)) = entry.get("type")
                    && tool_type == "function"
                    && let Some(OpenAiJsonValue::Object(function)) = entry.get("function")
                    && let Some(OpenAiJsonValue::String(name)) = function.get("name")
                {
                    names.push(name.clone());
                }
            }
            let allowed = if names.is_empty() { None } else { Some(names) };
            (Some(mode), allowed)
        }
    };

    config.function_calling_config = Some(gemini::types::FunctionCallingConfig {
        mode,
        allowed_function_names: allowed,
    });

    config
}

fn generation_config_from_openai(
    request: &openai::chat_completions::CreateChatCompletionRequest,
) -> Option<gemini::types::GenerationConfig> {
    if request.temperature.is_none()
        && request.top_p.is_none()
        && request.max_tokens.is_none()
        && request.max_completion_tokens.is_none()
        && request.stop.is_none()
        && request.presence_penalty.is_none()
        && request.frequency_penalty.is_none()
        && request.n.is_none()
        && request.seed.is_none()
        && request.response_format.is_none()
    {
        return None;
    }

    let mut config = gemini::types::GenerationConfig {
        stop_sequences: None,
        response_mime_type: None,
        response_schema: None,
        response_json_schema_internal: None,
        response_json_schema: None,
        response_modalities: None,
        candidate_count: None,
        max_output_tokens: None,
        temperature: request.temperature,
        top_p: request.top_p,
        top_k: None,
        seed: request.seed,
        presence_penalty: request.presence_penalty,
        frequency_penalty: request.frequency_penalty,
        response_logprobs: None,
        logprobs: request.top_logprobs,
        enable_enhanced_civic_answers: None,
        speech_config: None,
        thinking_config: None,
        image_config: None,
        media_resolution: None,
    };

    if let Some(stop) = &request.stop {
        let sequences = match stop {
            openai::chat_completions::StopConfiguration::Single(value) => vec![value.clone()],
            openai::chat_completions::StopConfiguration::Multiple(values) => values.clone(),
        };
        config.stop_sequences = Some(sequences);
    }

    if let Some(count) = request.n {
        config.candidate_count = Some(count);
    }

    let max_tokens = request.max_completion_tokens.or(request.max_tokens);
    if let Some(tokens) = max_tokens {
        config.max_output_tokens = Some(tokens);
    }

    if let Some(format) = &request.response_format {
        match format {
            openai::chat_completions::ResponseFormat::JsonSchema { json_schema } => {
                config.response_mime_type =
                    Some(gemini::types::ResponseMimeType::ApplicationJson);
                if let Some(schema) = &json_schema.schema {
                    config.response_json_schema = serde_json::to_value(schema).ok();
                }
            }
            openai::chat_completions::ResponseFormat::JsonObject => {
                config.response_mime_type =
                    Some(gemini::types::ResponseMimeType::ApplicationJson);
            }
            openai::chat_completions::ResponseFormat::Text => {
                config.response_mime_type = Some(gemini::types::ResponseMimeType::TextPlain);
            }
        }
    }

    Some(config)
}

pub fn request(
    request: openai::chat_completions::CreateChatCompletionRequest,
) -> Result<gemini::generate_content::GenerateContentRequest, TransformError> {
    let mut system_parts = Vec::new();
    let mut contents: Vec<gemini::types::Content> = Vec::new();
    let mut tool_name_map: HashMap<String, String> = HashMap::new();

    let generation_config = generation_config_from_openai(&request);
    let declarations = build_function_declarations(&request.tools, &request.functions);
    let tool_config = request
        .tool_choice
        .as_ref()
        .map(tool_choice_to_config);

    let messages = request.messages;

    for message in &messages {
        if let openai::chat_completions::ChatCompletionRequestMessage::Assistant {
            tool_calls: Some(tool_calls),
            ..
        } = message
        {
            for tool_call in tool_calls {
                if let openai::chat_completions::ChatCompletionMessageToolCall::Function {
                    id,
                    function,
                } = tool_call
                {
                    tool_name_map.insert(id.clone(), function.name.clone());
                }
            }
        }
    }

    for message in messages {
        match message {
            openai::chat_completions::ChatCompletionRequestMessage::Developer { content, .. }
            | openai::chat_completions::ChatCompletionRequestMessage::System { content, .. } => {
                let text = text_content_to_string(&content);
                if !text.trim().is_empty() {
                    system_parts.push(gemini::types::TextPart { text });
                }
            }
            openai::chat_completions::ChatCompletionRequestMessage::User { content, .. } => {
                let parts = user_content_to_parts(content);
                if !parts.is_empty() {
                    contents.push(gemini::types::Content {
                        role: Some(gemini::types::ContentRole::User),
                        parts,
                    });
                }
            }
            openai::chat_completions::ChatCompletionRequestMessage::Assistant {
                content,
                tool_calls,
                function_call,
                ..
            } => {
                let mut parts = Vec::new();
                if let Some(content) = content {
                    parts.extend(assistant_content_to_parts(content));
                }
                if let Some(tool_calls) = tool_calls {
                    for tool_call in tool_calls {
                        if let openai::chat_completions::ChatCompletionMessageToolCall::Function {
                            id,
                            function,
                        } = tool_call
                        {
                            let args = parse_args_to_struct(&function.arguments);
                            parts.push(function_call_part(
                                Some(id),
                                function.name,
                                Some(args),
                            ));
                        }
                    }
                }
                if let Some(function_call) = function_call {
                    let args = parse_args_to_struct(&function_call.arguments);
                    parts.push(function_call_part(
                        None,
                        function_call.name,
                        Some(args),
                    ));
                }
                if !parts.is_empty() {
                    contents.push(gemini::types::Content {
                        role: Some(gemini::types::ContentRole::Model),
                        parts,
                    });
                }
            }
            openai::chat_completions::ChatCompletionRequestMessage::Tool {
                content,
                tool_call_id,
            } => {
                let name = tool_name_map
                    .get(&tool_call_id)
                    .cloned()
                    .unwrap_or_else(|| "unknown_function".to_string());
                let text = text_content_to_string(&content);
                let response = match serde_json::from_str::<Value>(&text) {
                    Ok(value) => json_struct_from_value(value),
                    Err(_) => {
                        let mut map = gemini::types::JsonStruct::new();
                        map.insert("result".to_string(), Value::String(text));
                        map
                    }
                };
                contents.push(gemini::types::Content {
                    role: Some(gemini::types::ContentRole::User),
                    parts: vec![function_response_part(
                        Some(tool_call_id),
                        name,
                        response,
                    )],
                });
            }
            openai::chat_completions::ChatCompletionRequestMessage::Function { name, content } => {
                let text = content.unwrap_or_default();
                let response_value =
                    serde_json::from_str::<Value>(&text).unwrap_or(Value::String(text));
                let response = json_struct_from_value(response_value);
                contents.push(gemini::types::Content {
                    role: Some(gemini::types::ContentRole::User),
                    parts: vec![function_response_part(None, name, response)],
                });
            }
        }
    }

    if contents.is_empty() {
        contents.push(gemini::types::Content {
            role: Some(gemini::types::ContentRole::User),
            parts: vec![text_part("Please answer according to system instructions.".to_string())],
        });
    }

    let system_instruction = if system_parts.is_empty() {
        None
    } else {
        Some(gemini::types::SystemInstruction {
            parts: system_parts,
        })
    };

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
