use crate::protocol::{claude, gemini};

pub(super) fn gemini_content_to_claude_system(
    content: gemini::Content,
) -> Option<claude::SystemPrompt> {
    let text = gemini_content_to_text(content);
    (!text.is_empty()).then_some(claude::StringOrArray::String(text))
}

pub(super) fn gemini_contents_to_claude_messages(
    contents: Vec<gemini::Content>,
) -> Vec<claude::MessageParam> {
    contents
        .into_iter()
        .filter_map(gemini_content_to_claude_message)
        .collect()
}

pub(super) fn gemini_content_to_claude_response_blocks(
    content: gemini::Content,
) -> Vec<claude::ContentBlock> {
    content
        .parts
        .into_iter()
        .filter_map(part_to_response_block)
        .collect()
}

fn gemini_content_to_claude_message(content: gemini::Content) -> Option<claude::MessageParam> {
    let role = match content.role {
        Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)) => {
            claude::MessageRole::Known(claude::MessageRoleKnown::Assistant)
        }
        Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::System)) => {
            claude::MessageRole::Known(claude::MessageRoleKnown::System)
        }
        Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Function)) => {
            claude::MessageRole::Known(claude::MessageRoleKnown::User)
        }
        Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::User))
        | Some(gemini::ContentRole::Unknown(_))
        | None => claude::MessageRole::Known(claude::MessageRoleKnown::User),
    };

    let blocks = content
        .parts
        .into_iter()
        .filter_map(part_to_request_block)
        .collect::<Vec<_>>();
    (!blocks.is_empty()).then_some(claude::MessageParam {
        role,
        content: claude::StringOrArray::Array(blocks),
        extra: Default::default(),
    })
}

fn part_to_request_block(part: gemini::Part) -> Option<claude::ContentBlockParam> {
    match part.data? {
        gemini::PartData::Text { text } if part.thought == Some(true) => {
            Some(claude::ContentBlockParam::Thinking(claude::ThinkingBlock {
                signature: part.thought_signature.unwrap_or_default(),
                thinking: text,
                type_: claude::ThinkingBlockType::Thinking,
            }))
        }
        gemini::PartData::Text { text } => Some(text_block(text)),
        gemini::PartData::InlineData { inline_data } => inline_data_to_request_block(inline_data),
        gemini::PartData::FileData { file_data } => file_data_to_request_block(file_data),
        gemini::PartData::FunctionCall { function_call } => {
            Some(claude::ContentBlockParam::ToolUse(claude::ToolUseBlock {
                id: function_call
                    .id
                    .unwrap_or_else(|| format!("toolu_{}", function_call.name)),
                input: function_call.args.unwrap_or_default(),
                name: function_call.name,
                type_: claude::ToolUseBlockType::ToolUse,
                cache_control: None,
                caller: None,
            }))
        }
        gemini::PartData::FunctionResponse { function_response } => {
            let content = function_response_to_text(&function_response);
            Some(claude::ContentBlockParam::ToolResult(
                claude::ToolResultBlock {
                    tool_use_id: function_response
                        .id
                        .unwrap_or_else(|| function_response.name.clone()),
                    type_: claude::ToolResultBlockType::ToolResult,
                    cache_control: None,
                    content: Some(claude::ToolResultContent::Text(content)),
                    is_error: None,
                },
            ))
        }
        gemini::PartData::ExecutableCode { executable_code } => {
            Some(text_block(executable_code.code))
        }
        gemini::PartData::CodeExecutionResult {
            code_execution_result,
        } => code_execution_result.output.map(text_block),
        gemini::PartData::ToolCall { tool_call } => {
            Some(claude::ContentBlockParam::ToolUse(claude::ToolUseBlock {
                id: tool_call
                    .id
                    .unwrap_or_else(|| "toolu_server_tool".to_owned()),
                input: tool_call.args.unwrap_or_default(),
                name: serde_json::to_value(tool_call.tool_type)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .unwrap_or_else(|| "server_tool".to_owned()),
                type_: claude::ToolUseBlockType::ToolUse,
                cache_control: None,
                caller: None,
            }))
        }
        gemini::PartData::ToolResponse { tool_response } => Some(
            claude::ContentBlockParam::ToolResult(claude::ToolResultBlock {
                tool_use_id: tool_response.id.unwrap_or_else(|| "server_tool".to_owned()),
                type_: claude::ToolResultBlockType::ToolResult,
                cache_control: None,
                content: Some(claude::ToolResultContent::Text(
                    tool_response
                        .response
                        .map(|response| serde_json::to_string(&response).unwrap_or_default())
                        .unwrap_or_default(),
                )),
                is_error: None,
            }),
        ),
    }
}

fn part_to_response_block(part: gemini::Part) -> Option<claude::ContentBlock> {
    match part.data? {
        gemini::PartData::Text { text } if part.thought == Some(true) => {
            Some(claude::ContentBlock::Thinking(claude::ThinkingBlock {
                signature: part.thought_signature.unwrap_or_default(),
                thinking: text,
                type_: claude::ThinkingBlockType::Thinking,
            }))
        }
        gemini::PartData::Text { text } => {
            Some(claude::ContentBlock::Text(claude::ResponseTextBlock {
                citations: None,
                text,
                type_: claude::TextBlockType::Text,
                extra: Default::default(),
            }))
        }
        gemini::PartData::FunctionCall { function_call } => Some(claude::ContentBlock::ToolUse(
            claude::ResponseToolUseBlock {
                id: function_call
                    .id
                    .unwrap_or_else(|| format!("toolu_{}", function_call.name)),
                input: function_call.args.unwrap_or_default(),
                name: function_call.name,
                type_: claude::ToolUseBlockType::ToolUse,
                caller: None,
                extra: Default::default(),
            },
        )),
        gemini::PartData::ExecutableCode { executable_code } => {
            Some(response_text_block(executable_code.code))
        }
        gemini::PartData::CodeExecutionResult {
            code_execution_result,
        } => code_execution_result.output.map(response_text_block),
        _ => None,
    }
}

fn inline_data_to_request_block(data: gemini::Blob) -> Option<claude::ContentBlockParam> {
    if data.mime_type.starts_with("image/") {
        return Some(claude::ContentBlockParam::Image(claude::ImageBlock {
            source: claude::ImageSource::Base64(claude::Base64ImageSource {
                data: data.data,
                media_type: image_media_type(&data.mime_type)?,
                type_: claude::Base64SourceType::Base64,
                extra: Default::default(),
            }),
            type_: claude::ImageBlockType::Image,
            cache_control: None,
        }));
    }
    if data.mime_type == "application/pdf" {
        return Some(claude::ContentBlockParam::Document(claude::DocumentBlock {
            source: claude::DocumentSource::Base64(claude::Base64PdfSource {
                data: data.data,
                media_type: claude::PdfMediaType::ApplicationPdf,
                type_: claude::Base64SourceType::Base64,
                extra: Default::default(),
            }),
            type_: claude::DocumentBlockType::Document,
            cache_control: None,
            citations: None,
            context: None,
            title: None,
        }));
    }
    Some(text_block(data.data))
}

fn file_data_to_request_block(file_data: gemini::FileData) -> Option<claude::ContentBlockParam> {
    if file_data
        .mime_type
        .as_ref()
        .is_some_and(|mime| mime.starts_with("image/"))
    {
        return Some(claude::ContentBlockParam::Image(claude::ImageBlock {
            source: claude::ImageSource::Url(claude::UrlImageSource {
                type_: claude::UrlSourceType::Url,
                url: file_data.file_uri,
                extra: Default::default(),
            }),
            type_: claude::ImageBlockType::Image,
            cache_control: None,
        }));
    }
    Some(claude::ContentBlockParam::Document(claude::DocumentBlock {
        source: claude::DocumentSource::Url(claude::UrlDocumentSource {
            type_: claude::UrlSourceType::Url,
            url: file_data.file_uri,
            extra: Default::default(),
        }),
        type_: claude::DocumentBlockType::Document,
        cache_control: None,
        citations: None,
        context: None,
        title: None,
    }))
}

fn gemini_content_to_text(content: gemini::Content) -> String {
    content
        .parts
        .into_iter()
        .filter_map(|part| match part.data {
            Some(gemini::PartData::Text { text }) => Some(text),
            Some(gemini::PartData::ExecutableCode { executable_code }) => {
                Some(executable_code.code)
            }
            Some(gemini::PartData::CodeExecutionResult {
                code_execution_result,
            }) => code_execution_result.output,
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn function_response_to_text(response: &gemini::FunctionResponse) -> String {
    response
        .response
        .get("output")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| serde_json::to_string(&response.response).unwrap_or_default())
}

fn text_block(text: String) -> claude::ContentBlockParam {
    claude::ContentBlockParam::Text(claude::TextBlock {
        text,
        type_: claude::TextBlockType::Text,
        cache_control: None,
        citations: None,
        extra: Default::default(),
    })
}

fn response_text_block(text: String) -> claude::ContentBlock {
    claude::ContentBlock::Text(claude::ResponseTextBlock {
        citations: None,
        text,
        type_: claude::TextBlockType::Text,
        extra: Default::default(),
    })
}

fn image_media_type(mime: &str) -> Option<claude::ImageMediaType> {
    match mime {
        "image/jpeg" => Some(claude::ImageMediaType::Jpeg),
        "image/png" => Some(claude::ImageMediaType::Png),
        "image/gif" => Some(claude::ImageMediaType::Gif),
        "image/webp" => Some(claude::ImageMediaType::Webp),
        _ => None,
    }
}
