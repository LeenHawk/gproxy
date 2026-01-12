use std::collections::HashMap;

use serde_json::Value;

use crate::formats::{claude, openai};

use super::{RequestParts, TransformError};

const OPENAI_CHAT_PATH: &str = "/v1/chat/completions";
const CLAUDE_MESSAGES_PATH: &str = "/v1/messages";
const DEFAULT_MAX_TOKENS: u32 = 8192;

pub fn request(
    req: RequestParts<openai::chat_completions::CreateChatCompletionRequest>,
) -> Result<RequestParts<claude::messages::MessageCreateRequest>, TransformError> {
    if req.path.trim_end_matches('/') != OPENAI_CHAT_PATH {
        return Err(TransformError::Invalid("chat completions path"));
    }
    super::ensure_empty_query(&req.query)?;
    let openai::chat_completions::CreateChatCompletionRequest {
        messages,
        model,
        reasoning_effort,
        max_completion_tokens,
        max_tokens: legacy_max_tokens,
        temperature,
        top_p,
        stop,
        stream,
        tools,
        tool_choice,
        function_call,
        functions,
        parallel_tool_calls,
        user,
        metadata,
        ..
    } = req.body;
    let messages = sanitize_messages(messages);
    let (system, messages) = split_messages(messages)?;
    let max_tokens = extract_max_tokens(max_completion_tokens, legacy_max_tokens)?;
    let tools = build_tools(tools, functions)?;
    let tool_choice = build_tool_choice(tool_choice, function_call, parallel_tool_calls)?;
    let temperature = temperature
        .map(|value| unit_interval(value, "temperature"))
        .transpose()?;
    let top_p = top_p
        .map(|value| unit_interval(value, "top_p"))
        .transpose()?;
    let thinking = reasoning_effort
        .map(thinking_from_reasoning_effort)
        .transpose()?
        .flatten();
    let metadata = metadata_from_openai(user, metadata);
    let stop_sequences = stop.map(stop_sequences_from_openai);
    let body = claude::messages::MessageCreateRequest {
        max_tokens,
        messages: claude::types::MessageList(messages),
        model: claude::types::Model::Other(model),
        container: None,
        context_management: None,
        mcp_servers: None,
        metadata,
        output_config: None,
        output_format: None,
        service_tier: None,
        stop_sequences,
        stream,
        system,
        temperature,
        thinking,
        tool_choice,
        tools,
        top_k: None,
        top_p,
    };
    Ok(RequestParts {
        path: CLAUDE_MESSAGES_PATH.to_string(),
        query: req.query,
        headers: req.headers,
        body,
    })
}

fn extract_max_tokens(
    max_completion_tokens: Option<i64>,
    max_tokens: Option<i64>,
) -> Result<u32, TransformError> {
    let value = max_completion_tokens.or(max_tokens);
    match value {
        Some(value) => u32::try_from(value).map_err(|_| TransformError::Invalid("max_tokens")),
        None => Ok(DEFAULT_MAX_TOKENS),
    }
}

fn sanitize_messages(
    messages: Vec<openai::chat_completions::ChatCompletionRequestMessage>,
) -> Vec<openai::chat_completions::ChatCompletionRequestMessage> {
    messages
        .into_iter()
        .filter_map(|message| match message {
            openai::chat_completions::ChatCompletionRequestMessage::Developer { content, name } => {
                Some(
                    openai::chat_completions::ChatCompletionRequestMessage::Developer {
                        content: trim_text_content(content),
                        name,
                    },
                )
            }
            openai::chat_completions::ChatCompletionRequestMessage::System { content, name } => {
                Some(
                    openai::chat_completions::ChatCompletionRequestMessage::System {
                        content: trim_text_content(content),
                        name,
                    },
                )
            }
            openai::chat_completions::ChatCompletionRequestMessage::User { content, name } => {
                Some(
                    openai::chat_completions::ChatCompletionRequestMessage::User {
                        content: trim_user_content(content),
                        name,
                    },
                )
            }
            openai::chat_completions::ChatCompletionRequestMessage::Assistant {
                content,
                refusal,
                name,
                audio,
                tool_calls,
                function_call,
            } => {
                let content = content.and_then(trim_assistant_content);
                let refusal = trim_optional_string(refusal);
                let should_drop =
                    content.is_none()
                        && refusal.is_none()
                        && tool_calls.is_none()
                        && function_call.is_none()
                        && audio.is_none();
                if should_drop {
                    None
                } else {
                    Some(openai::chat_completions::ChatCompletionRequestMessage::Assistant {
                        content,
                        refusal,
                        name,
                        audio,
                        tool_calls,
                        function_call,
                    })
                }
            }
            openai::chat_completions::ChatCompletionRequestMessage::Tool {
                content,
                tool_call_id,
            } => Some(openai::chat_completions::ChatCompletionRequestMessage::Tool {
                content: trim_text_content(content),
                tool_call_id,
            }),
            openai::chat_completions::ChatCompletionRequestMessage::Function { content, name } => {
                let content = content.map(trim_string).filter(|value| !value.is_empty());
                Some(openai::chat_completions::ChatCompletionRequestMessage::Function {
                    content,
                    name,
                })
            }
        })
        .collect()
}

fn trim_string(value: String) -> String {
    value.trim().to_string()
}

fn trim_optional_string(value: Option<String>) -> Option<String> {
    value.map(trim_string).filter(|value| !value.is_empty())
}

fn trim_text_content(
    content: openai::chat_completions::ChatCompletionTextContent,
) -> openai::chat_completions::ChatCompletionTextContent {
    match content {
        openai::chat_completions::ChatCompletionTextContent::Text(text) => {
            openai::chat_completions::ChatCompletionTextContent::Text(trim_string(text))
        }
        openai::chat_completions::ChatCompletionTextContent::Parts(parts) => {
            let parts: Vec<_> = parts
                .into_iter()
                .filter_map(|mut part| {
                    let trimmed = part.text.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        part.text = trimmed.to_string();
                        Some(part)
                    }
                })
                .collect();
            if parts.is_empty() {
                openai::chat_completions::ChatCompletionTextContent::Text(String::new())
            } else {
                openai::chat_completions::ChatCompletionTextContent::Parts(parts)
            }
        }
    }
}

fn trim_user_content(
    content: openai::chat_completions::ChatCompletionUserContent,
) -> openai::chat_completions::ChatCompletionUserContent {
    match content {
        openai::chat_completions::ChatCompletionUserContent::Text(text) => {
            openai::chat_completions::ChatCompletionUserContent::Text(trim_string(text))
        }
        openai::chat_completions::ChatCompletionUserContent::Parts(parts) => {
            let mut output = Vec::new();
            for part in parts {
                match part {
                    openai::chat_completions::ChatCompletionRequestUserContentPart::Text {
                        text,
                    } => {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            output.push(
                                openai::chat_completions::ChatCompletionRequestUserContentPart::Text {
                                    text: trimmed.to_string(),
                                },
                            );
                        }
                    }
                    openai::chat_completions::ChatCompletionRequestUserContentPart::ImageUrl {
                        image_url,
                    } => {
                        output.push(
                            openai::chat_completions::ChatCompletionRequestUserContentPart::ImageUrl {
                                image_url,
                            },
                        );
                    }
                    openai::chat_completions::ChatCompletionRequestUserContentPart::InputAudio {
                        input_audio,
                    } => {
                        output.push(
                            openai::chat_completions::ChatCompletionRequestUserContentPart::InputAudio {
                                input_audio,
                            },
                        );
                    }
                    openai::chat_completions::ChatCompletionRequestUserContentPart::File { file } => {
                        output.push(
                            openai::chat_completions::ChatCompletionRequestUserContentPart::File {
                                file,
                            },
                        );
                    }
                }
            }
            if output.is_empty() {
                openai::chat_completions::ChatCompletionUserContent::Text(String::new())
            } else {
                openai::chat_completions::ChatCompletionUserContent::Parts(output)
            }
        }
    }
}

fn trim_assistant_content(
    content: openai::chat_completions::ChatCompletionAssistantContent,
) -> Option<openai::chat_completions::ChatCompletionAssistantContent> {
    match content {
        openai::chat_completions::ChatCompletionAssistantContent::Text(text) => {
            let trimmed = trim_string(text);
            if trimmed.is_empty() {
                None
            } else {
                Some(openai::chat_completions::ChatCompletionAssistantContent::Text(trimmed))
            }
        }
        openai::chat_completions::ChatCompletionAssistantContent::Parts(parts) => {
            let mut output = Vec::new();
            for part in parts {
                match part {
                    openai::chat_completions::ChatCompletionRequestAssistantContentPart::Text {
                        text,
                    } => {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            output.push(
                                openai::chat_completions::ChatCompletionRequestAssistantContentPart::Text {
                                    text: trimmed.to_string(),
                                },
                            );
                        }
                    }
                    openai::chat_completions::ChatCompletionRequestAssistantContentPart::Refusal {
                        refusal,
                    } => {
                        let trimmed = refusal.trim();
                        if !trimmed.is_empty() {
                            output.push(
                                openai::chat_completions::ChatCompletionRequestAssistantContentPart::Refusal {
                                    refusal: trimmed.to_string(),
                                },
                            );
                        }
                    }
                }
            }
            if output.is_empty() {
                None
            } else {
                Some(openai::chat_completions::ChatCompletionAssistantContent::Parts(output))
            }
        }
    }
}

fn split_messages(
    messages: Vec<openai::chat_completions::ChatCompletionRequestMessage>,
) -> Result<
    (
        Option<claude::types::SystemPrompt>,
        Vec<claude::types::BetaMessageParam>,
    ),
    TransformError,
> {
    let mut system_blocks: Vec<claude::types::BetaTextBlockParam> = Vec::new();
    let mut output: Vec<claude::types::BetaMessageParam> = Vec::new();

    for message in messages {
        match message {
            openai::chat_completions::ChatCompletionRequestMessage::Developer { content, .. }
            | openai::chat_completions::ChatCompletionRequestMessage::System { content, .. } => {
                let text = text_from_text_content(&content);
                system_blocks.push(text_block(text));
            }
            openai::chat_completions::ChatCompletionRequestMessage::User { content, .. } => {
                let blocks = user_content_blocks(content)?;
                let content = message_content_from_blocks(blocks);
                output.push(claude::types::BetaMessageParam {
                    role: claude::types::MessageRole::User,
                    content,
                });
            }
            openai::chat_completions::ChatCompletionRequestMessage::Assistant {
                content,
                tool_calls,
                function_call,
                refusal,
                ..
            } => {
                let blocks = assistant_content_blocks(content, refusal)?;
                let mut blocks = blocks;
                if let Some(tool_calls) = tool_calls {
                    blocks.extend(tool_use_blocks(tool_calls)?);
                }
                if let Some(function_call) = function_call {
                    blocks.extend(function_call_blocks(function_call)?);
                }
                let content = message_content_from_blocks(blocks);
                output.push(claude::types::BetaMessageParam {
                    role: claude::types::MessageRole::Assistant,
                    content,
                });
            }
            openai::chat_completions::ChatCompletionRequestMessage::Tool {
                content,
                tool_call_id,
            } => {
                let tool_result = tool_result_block(tool_call_id, content)?;
                output.push(claude::types::BetaMessageParam {
                    role: claude::types::MessageRole::User,
                    content: claude::types::MessageContent::Blocks(vec![
                        claude::types::BetaContentBlockParam::ToolResult(tool_result),
                    ]),
                });
            }
            openai::chat_completions::ChatCompletionRequestMessage::Function { content, name } => {
                let tool_content = openai::chat_completions::ChatCompletionTextContent::Text(
                    content.unwrap_or_default(),
                );
                let tool_result = tool_result_block(name, tool_content)?;
                output.push(claude::types::BetaMessageParam {
                    role: claude::types::MessageRole::User,
                    content: claude::types::MessageContent::Blocks(vec![
                        claude::types::BetaContentBlockParam::ToolResult(tool_result),
                    ]),
                });
            }
        }
    }

    let system = if system_blocks.is_empty() {
        None
    } else if system_blocks.len() == 1 {
        Some(claude::types::SystemPrompt::Text(
            system_blocks
                .first()
                .map(|block| block.text.clone())
                .unwrap_or_default(),
        ))
    } else {
        Some(claude::types::SystemPrompt::Blocks(system_blocks))
    };

    Ok((system, output))
}

fn text_from_text_content(
    content: &openai::chat_completions::ChatCompletionTextContent,
) -> String {
    match content {
        openai::chat_completions::ChatCompletionTextContent::Text(text) => text.clone(),
        openai::chat_completions::ChatCompletionTextContent::Parts(parts) => {
            parts.iter().map(|part| part.text.as_str()).collect()
        }
    }
}

fn text_block(text: String) -> claude::types::BetaTextBlockParam {
    claude::types::BetaTextBlockParam {
        text,
        block_type: claude::types::TextBlockType::Text,
        cache_control: None,
        citations: None,
    }
}

fn user_content_blocks(
    content: openai::chat_completions::ChatCompletionUserContent,
) -> Result<Vec<claude::types::BetaContentBlockParam>, TransformError> {
    match content {
        openai::chat_completions::ChatCompletionUserContent::Text(text) => {
            Ok(vec![claude::types::BetaContentBlockParam::Text(text_block(
                text,
            ))])
        }
        openai::chat_completions::ChatCompletionUserContent::Parts(parts) => {
            let mut blocks = Vec::with_capacity(parts.len());
            for part in parts {
                match part {
                    openai::chat_completions::ChatCompletionRequestUserContentPart::Text {
                        text,
                    } => {
                        blocks.push(claude::types::BetaContentBlockParam::Text(text_block(
                            text,
                        )));
                    }
                    openai::chat_completions::ChatCompletionRequestUserContentPart::ImageUrl {
                        image_url,
                    } => {
                        blocks.push(claude::types::BetaContentBlockParam::Image(
                            image_block_from_url(image_url.url),
                        ));
                    }
                    openai::chat_completions::ChatCompletionRequestUserContentPart::InputAudio {
                        ..
                    } => {
                        return Err(TransformError::Unsupported("input_audio"));
                    }
                    openai::chat_completions::ChatCompletionRequestUserContentPart::File {
                        file,
                    } => {
                        blocks.push(claude::types::BetaContentBlockParam::Document(
                            document_block_from_file(file)?,
                        ));
                    }
                }
            }
            Ok(blocks)
        }
    }
}

fn assistant_content_blocks(
    content: Option<openai::chat_completions::ChatCompletionAssistantContent>,
    refusal: Option<String>,
) -> Result<Vec<claude::types::BetaContentBlockParam>, TransformError> {
    let mut blocks = Vec::new();
    match content {
        Some(openai::chat_completions::ChatCompletionAssistantContent::Text(text)) => {
            blocks.push(claude::types::BetaContentBlockParam::Text(text_block(text)));
        }
        Some(openai::chat_completions::ChatCompletionAssistantContent::Parts(parts)) => {
            for part in parts {
                match part {
                    openai::chat_completions::ChatCompletionRequestAssistantContentPart::Text {
                        text,
                    } => {
                        blocks.push(claude::types::BetaContentBlockParam::Text(text_block(
                            text,
                        )));
                    }
                    openai::chat_completions::ChatCompletionRequestAssistantContentPart::Refusal {
                        refusal,
                    } => {
                        blocks.push(claude::types::BetaContentBlockParam::Text(text_block(
                            refusal,
                        )));
                    }
                }
            }
        }
        None => {}
    }
    if let Some(refusal) = refusal {
        blocks.push(claude::types::BetaContentBlockParam::Text(text_block(refusal)));
    }
    Ok(blocks)
}

fn tool_use_blocks(
    tool_calls: openai::chat_completions::ChatCompletionMessageToolCalls,
) -> Result<Vec<claude::types::BetaContentBlockParam>, TransformError> {
    let mut blocks = Vec::with_capacity(tool_calls.len());
    for call in tool_calls {
        match call {
            openai::chat_completions::ChatCompletionMessageToolCall::Function { id, function } => {
                blocks.push(claude::types::BetaContentBlockParam::ToolUse(
                    claude::types::BetaToolUseBlockParam {
                        id,
                        input: parse_arguments(&function.arguments)?,
                        name: function.name,
                        block_type: claude::types::ToolUseBlockType::ToolUse,
                        cache_control: None,
                        caller: None,
                    },
                ));
            }
            openai::chat_completions::ChatCompletionMessageToolCall::Custom { id, custom } => {
                let mut input = HashMap::new();
                input.insert("input".to_string(), Value::String(custom.input));
                blocks.push(claude::types::BetaContentBlockParam::ToolUse(
                    claude::types::BetaToolUseBlockParam {
                        id,
                        input,
                        name: custom.name,
                        block_type: claude::types::ToolUseBlockType::ToolUse,
                        cache_control: None,
                        caller: None,
                    },
                ));
            }
        }
    }
    Ok(blocks)
}

fn function_call_blocks(
    function_call: openai::chat_completions::ChatCompletionFunctionCall,
) -> Result<Vec<claude::types::BetaContentBlockParam>, TransformError> {
    Ok(vec![claude::types::BetaContentBlockParam::ToolUse(
        claude::types::BetaToolUseBlockParam {
            id: function_call.name.clone(),
            input: parse_arguments(&function_call.arguments)?,
            name: function_call.name,
            block_type: claude::types::ToolUseBlockType::ToolUse,
            cache_control: None,
            caller: None,
        },
    )])
}

fn tool_result_block(
    tool_use_id: String,
    content: openai::chat_completions::ChatCompletionTextContent,
) -> Result<claude::types::BetaToolResultBlockParam, TransformError> {
    let text = text_from_text_content(&content);
    let content = if text.is_empty() {
        None
    } else {
        Some(claude::types::ToolResultContentParam::Text(text))
    };
    Ok(claude::types::BetaToolResultBlockParam {
        tool_use_id,
        block_type: claude::types::ToolResultBlockType::ToolResult,
        cache_control: None,
        content,
        is_error: None,
    })
}

fn message_content_from_blocks(
    blocks: Vec<claude::types::BetaContentBlockParam>,
) -> claude::types::MessageContent {
    if blocks.len() == 1
        && let claude::types::BetaContentBlockParam::Text(text) = &blocks[0] {
            return claude::types::MessageContent::Text(text.text.clone());
        }
    claude::types::MessageContent::Blocks(blocks)
}

fn image_block_from_url(url: String) -> claude::types::BetaImageBlockParam {
    claude::types::BetaImageBlockParam {
        source: claude::types::BetaImageSource::Url(claude::types::BetaURLImageSource {
            source_type: claude::types::ImageSourceTypeUrl::Url,
            url,
        }),
        block_type: claude::types::ImageBlockType::Image,
        cache_control: None,
    }
}

fn document_block_from_file(
    file: openai::chat_completions::ChatCompletionFileContent,
) -> Result<claude::types::BetaRequestDocumentBlock, TransformError> {
    let file_id = file
        .file_id
        .ok_or(TransformError::Unsupported("file_data"))?;
    Ok(claude::types::BetaRequestDocumentBlock {
        source: claude::types::BetaDocumentSource::File(claude::types::BetaFileDocumentSource {
            file_id,
            source_type: claude::types::DocumentSourceTypeFile::File,
        }),
        block_type: claude::types::DocumentBlockType::Document,
        cache_control: None,
        citations: None,
        context: None,
        title: file.filename,
    })
}

fn parse_arguments(value: &str) -> Result<HashMap<String, Value>, TransformError> {
    let parsed: Value =
        serde_json::from_str(value).map_err(|_| TransformError::Invalid("tool arguments"))?;
    match parsed {
        Value::Object(map) => Ok(map.into_iter().collect()),
        _ => Err(TransformError::Invalid("tool arguments")),
    }
}

fn build_tools(
    tools: Option<Vec<openai::chat_completions::ChatCompletionTool>>,
    functions: Option<Vec<openai::chat_completions::ChatCompletionFunction>>,
) -> Result<Option<Vec<claude::types::BetaToolUnion>>, TransformError> {
    let mut output = Vec::new();
    if let Some(tools) = tools {
        for tool in tools {
            match tool {
                openai::chat_completions::ChatCompletionTool::Function { function } => {
                    output.push(claude::types::BetaToolUnion::Tool(build_tool_from_function(
                        function.name,
                        function.description,
                        function.parameters,
                        function.strict,
                        None,
                    )?));
                }
                openai::chat_completions::ChatCompletionTool::Custom { custom } => {
                    output.push(claude::types::BetaToolUnion::Tool(build_tool_from_function(
                        custom.name,
                        custom.description,
                        None,
                        None,
                        Some(claude::types::CustomToolType::Custom),
                    )?));
                }
            }
        }
    }
    if let Some(functions) = functions {
        for function in functions {
            output.push(claude::types::BetaToolUnion::Tool(build_tool_from_function(
                function.name,
                function.description,
                function.parameters,
                None,
                None,
            )?));
        }
    }
    if output.is_empty() {
        Ok(None)
    } else {
        Ok(Some(output))
    }
}

fn build_tool_from_function(
    name: String,
    description: Option<String>,
    parameters: Option<openai::types::JsonSchema>,
    strict: Option<bool>,
    tool_type: Option<claude::types::CustomToolType>,
) -> Result<claude::types::BetaTool, TransformError> {
    let input_schema = map_tool_schema(parameters)?;
    Ok(claude::types::BetaTool {
        input_schema,
        name,
        allowed_callers: None,
        cache_control: None,
        defer_loading: None,
        description,
        input_examples: None,
        strict,
        tool_type,
    })
}

fn map_tool_schema(
    schema: Option<openai::types::JsonSchema>,
) -> Result<claude::types::JsonSchema, TransformError> {
    let schema = match schema {
        Some(schema) => schema,
        None => {
            return Ok(claude::types::JsonSchema {
                schema_type: claude::types::JsonSchemaType::Object,
                properties: None,
                required: None,
            });
        }
    };
    let schema_type = match schema.get("type") {
        Some(openai::types::JsonValue::String(value)) if value == "object" => {
            claude::types::JsonSchemaType::Object
        }
        Some(_) => return Err(TransformError::Unsupported("tool schema type")),
        None => claude::types::JsonSchemaType::Object,
    };
    let properties = match schema.get("properties") {
        Some(openai::types::JsonValue::Object(map)) => Some(
            map.iter()
                .map(|(key, value)| (key.clone(), json_value_to_serde(value)))
                .collect(),
        ),
        Some(_) => return Err(TransformError::Invalid("tool schema properties")),
        None => None,
    };
    let required = match schema.get("required") {
        Some(openai::types::JsonValue::Array(values)) => {
            let mut output = Vec::new();
            for value in values {
                match value {
                    openai::types::JsonValue::String(value) => output.push(value.clone()),
                    _ => return Err(TransformError::Invalid("tool schema required")),
                }
            }
            Some(output)
        }
        Some(_) => return Err(TransformError::Invalid("tool schema required")),
        None => None,
    };
    Ok(claude::types::JsonSchema {
        schema_type,
        properties,
        required,
    })
}

fn json_value_to_serde(value: &openai::types::JsonValue) -> Value {
    match value {
        openai::types::JsonValue::Null(_) => Value::Null,
        openai::types::JsonValue::Bool(value) => Value::Bool(*value),
        openai::types::JsonValue::Number(value) => Value::Number(value.clone()),
        openai::types::JsonValue::String(value) => Value::String(value.clone()),
        openai::types::JsonValue::Array(values) => {
            Value::Array(values.iter().map(json_value_to_serde).collect())
        }
        openai::types::JsonValue::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), json_value_to_serde(value)))
                .collect(),
        ),
    }
}

fn build_tool_choice(
    choice: Option<openai::chat_completions::ChatCompletionToolChoiceOption>,
    function_call: Option<openai::chat_completions::ChatCompletionFunctionCallOption>,
    parallel_tool_calls: Option<bool>,
) -> Result<Option<claude::types::BetaToolChoice>, TransformError> {
    let mut choice = if let Some(choice) = choice {
        Some(match choice {
            openai::chat_completions::ChatCompletionToolChoiceOption::Mode(mode) => match mode {
                openai::chat_completions::ChatCompletionToolChoiceMode::None => {
                    claude::types::BetaToolChoice::None
                }
                openai::chat_completions::ChatCompletionToolChoiceMode::Auto => {
                    claude::types::BetaToolChoice::Auto {
                        disable_parallel_tool_use: None,
                    }
                }
                openai::chat_completions::ChatCompletionToolChoiceMode::Required => {
                    claude::types::BetaToolChoice::Any {
                        disable_parallel_tool_use: None,
                    }
                }
            },
            openai::chat_completions::ChatCompletionToolChoiceOption::AllowedTools(_) => {
                return Err(TransformError::Unsupported("allowed tools"));
            }
            openai::chat_completions::ChatCompletionToolChoiceOption::NamedTool(named) => {
                claude::types::BetaToolChoice::Tool {
                    name: named.function.name,
                    disable_parallel_tool_use: None,
                }
            }
            openai::chat_completions::ChatCompletionToolChoiceOption::NamedCustom(named) => {
                claude::types::BetaToolChoice::Tool {
                    name: named.custom.name,
                    disable_parallel_tool_use: None,
                }
            }
        })
    } else { function_call.map(|function_call| match function_call {
            openai::chat_completions::ChatCompletionFunctionCallOption::Mode(mode) => match mode {
                openai::chat_completions::ChatCompletionFunctionCallMode::None => {
                    claude::types::BetaToolChoice::None
                }
                openai::chat_completions::ChatCompletionFunctionCallMode::Auto => {
                    claude::types::BetaToolChoice::Auto {
                        disable_parallel_tool_use: None,
                    }
                }
            },
            openai::chat_completions::ChatCompletionFunctionCallOption::Named(named) => {
                claude::types::BetaToolChoice::Tool {
                    name: named.name,
                    disable_parallel_tool_use: None,
                }
            }
        }) };

    if let Some(choice) = choice.as_mut()
        && matches!(parallel_tool_calls, Some(false)) {
            match choice {
                claude::types::BetaToolChoice::Auto {
                    disable_parallel_tool_use,
                }
                | claude::types::BetaToolChoice::Any {
                    disable_parallel_tool_use,
                }
                | claude::types::BetaToolChoice::Tool {
                    disable_parallel_tool_use,
                    ..
                } => {
                    *disable_parallel_tool_use = Some(true);
                }
                claude::types::BetaToolChoice::None => {}
            }
        }

    Ok(choice)
}

fn unit_interval(
    value: f64,
    name: &'static str,
) -> Result<claude::messages::UnitIntervalF64, TransformError> {
    claude::messages::UnitIntervalF64::new(value).map_err(|_| TransformError::Invalid(name))
}

fn thinking_from_reasoning_effort(
    effort: openai::types::ReasoningEffort,
) -> Result<Option<claude::types::BetaThinkingConfigParam>, TransformError> {
    use claude::types::{BetaThinkingConfigParam, BudgetTokens};
    let tokens = match effort {
        openai::types::ReasoningEffort::None => {
            return Ok(Some(BetaThinkingConfigParam::Disabled));
        }
        openai::types::ReasoningEffort::Minimal => 1024,
        openai::types::ReasoningEffort::Low => 2048,
        openai::types::ReasoningEffort::Medium => 4096,
        openai::types::ReasoningEffort::High => 6144,
        openai::types::ReasoningEffort::Xhigh => 8192,
    };
    let budget =
        BudgetTokens::new(tokens).map_err(|_| TransformError::Invalid("reasoning_effort"))?;
    Ok(Some(BetaThinkingConfigParam::Enabled {
        budget_tokens: budget,
    }))
}

fn metadata_from_openai(
    user: Option<String>,
    metadata: Option<HashMap<String, String>>,
) -> Option<claude::messages::BetaMetadata> {
    let user_id = user.or_else(|| metadata.and_then(|mut map| map.remove("user_id")));
    user_id.map(|user_id| claude::messages::BetaMetadata {
        user_id: Some(user_id),
    })
}

fn stop_sequences_from_openai(
    stop: openai::chat_completions::StopConfiguration,
) -> Vec<String> {
    match stop {
        openai::chat_completions::StopConfiguration::Single(value) => vec![value],
        openai::chat_completions::StopConfiguration::Multiple(values) => values,
    }
}
