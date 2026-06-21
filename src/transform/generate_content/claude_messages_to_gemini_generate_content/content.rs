use crate::protocol::{claude, gemini};

pub(super) fn claude_system_to_gemini(
    system: Option<claude::SystemPrompt>,
) -> Option<gemini::Content> {
    let text = match system? {
        claude::StringOrArray::String(text) => text,
        claude::StringOrArray::Array(blocks) => blocks
            .into_iter()
            .map(|block| block.text)
            .collect::<Vec<_>>()
            .join(""),
    };
    (!text.is_empty()).then(|| text_content(text, gemini::ContentRoleKnown::System))
}

pub(super) fn claude_messages_to_gemini_contents(
    messages: Vec<claude::MessageParam>,
) -> Vec<gemini::Content> {
    let mut contents = Vec::new();
    for message in messages {
        match message.content {
            claude::StringOrArray::String(text) => {
                if !text.is_empty() {
                    contents.push(text_content(text, message_role_to_gemini(message.role)));
                }
            }
            claude::StringOrArray::Array(blocks) => {
                contents.extend(blocks_to_contents(
                    blocks,
                    message_role_to_gemini(message.role),
                ));
            }
        }
    }
    contents
}

pub(super) fn claude_response_blocks_to_gemini_content(
    blocks: Vec<claude::ContentBlock>,
) -> gemini::Content {
    let parts = blocks
        .into_iter()
        .filter_map(response_block_to_part)
        .collect::<Vec<_>>();
    gemini::Content {
        parts,
        role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
        extra: Default::default(),
    }
}

fn blocks_to_contents(
    blocks: Vec<claude::ContentBlockParam>,
    role: gemini::ContentRoleKnown,
) -> Vec<gemini::Content> {
    let mut contents = Vec::new();
    let mut current_parts = Vec::new();

    for block in blocks {
        match block {
            claude::ContentBlockParam::MidConversationSystem(block) => {
                flush_parts(&mut contents, &mut current_parts, role.clone());
                let text = block
                    .content
                    .into_iter()
                    .map(|block| block.text)
                    .collect::<Vec<_>>()
                    .join("");
                if !text.is_empty() {
                    contents.push(text_content(text, gemini::ContentRoleKnown::System));
                }
            }
            block => {
                if let Some(part) = request_block_to_part(block) {
                    current_parts.push(part);
                }
            }
        }
    }

    flush_parts(&mut contents, &mut current_parts, role);
    contents
}

fn flush_parts(
    contents: &mut Vec<gemini::Content>,
    parts: &mut Vec<gemini::Part>,
    role: gemini::ContentRoleKnown,
) {
    if parts.is_empty() {
        return;
    }
    contents.push(gemini::Content {
        parts: std::mem::take(parts),
        role: Some(gemini::ContentRole::Known(role)),
        extra: Default::default(),
    });
}

fn request_block_to_part(block: claude::ContentBlockParam) -> Option<gemini::Part> {
    match block {
        claude::ContentBlockParam::Text(block) => Some(text_part(block.text)),
        claude::ContentBlockParam::Thinking(block) => Some(gemini::Part {
            thought: Some(true),
            thought_signature: Some(block.signature),
            data: Some(gemini::PartData::Text {
                text: block.thinking,
            }),
            ..Default::default()
        }),
        claude::ContentBlockParam::Image(block) => image_source_to_part(block.source),
        claude::ContentBlockParam::Document(block) => document_source_to_part(block.source),
        claude::ContentBlockParam::ToolUse(block) => Some(gemini::Part {
            data: Some(gemini::PartData::FunctionCall {
                function_call: gemini::FunctionCall {
                    id: Some(block.id),
                    name: block.name,
                    args: Some(block.input),
                    extra: Default::default(),
                },
            }),
            ..Default::default()
        }),
        claude::ContentBlockParam::ToolResult(block) => Some(tool_result_part(
            Some(block.tool_use_id.clone()),
            block.tool_use_id,
            block
                .content
                .map(tool_result_content_to_text)
                .unwrap_or_default(),
        )),
        _ => None,
    }
}

fn response_block_to_part(block: claude::ContentBlock) -> Option<gemini::Part> {
    match block {
        claude::ContentBlock::Text(block) => Some(text_part(block.text)),
        claude::ContentBlock::Thinking(block) => Some(gemini::Part {
            thought: Some(true),
            thought_signature: Some(block.signature),
            data: Some(gemini::PartData::Text {
                text: block.thinking,
            }),
            ..Default::default()
        }),
        claude::ContentBlock::ToolUse(block) => Some(gemini::Part {
            data: Some(gemini::PartData::FunctionCall {
                function_call: gemini::FunctionCall {
                    id: Some(block.id),
                    name: block.name,
                    args: Some(block.input),
                    extra: Default::default(),
                },
            }),
            ..Default::default()
        }),
        _ => None,
    }
}

fn image_source_to_part(source: claude::ImageSource) -> Option<gemini::Part> {
    match source {
        claude::ImageSource::Base64(source) => Some(gemini::Part {
            data: Some(gemini::PartData::InlineData {
                inline_data: gemini::Blob {
                    mime_type: image_media_type(source.media_type).to_owned(),
                    data: source.data,
                    extra: Default::default(),
                },
            }),
            ..Default::default()
        }),
        claude::ImageSource::Url(source) => Some(file_part(None, source.url)),
        claude::ImageSource::File(source) => Some(file_part(None, source.file_id)),
        claude::ImageSource::Raw(_) => None,
    }
}

fn document_source_to_part(source: claude::DocumentSource) -> Option<gemini::Part> {
    match source {
        claude::DocumentSource::Base64(source) => Some(gemini::Part {
            data: Some(gemini::PartData::InlineData {
                inline_data: gemini::Blob {
                    mime_type: "application/pdf".to_owned(),
                    data: source.data,
                    extra: Default::default(),
                },
            }),
            ..Default::default()
        }),
        claude::DocumentSource::Text(source) => Some(text_part(source.data)),
        claude::DocumentSource::Url(source) => Some(file_part(None, source.url)),
        claude::DocumentSource::File(source) => Some(file_part(None, source.file_id)),
        claude::DocumentSource::Content(_) | claude::DocumentSource::Raw(_) => None,
    }
}

fn tool_result_part(id: Option<String>, name: String, output: String) -> gemini::Part {
    let mut response = gemini::JsonMap::new();
    response.insert("output".to_owned(), serde_json::Value::String(output));
    gemini::Part {
        data: Some(gemini::PartData::FunctionResponse {
            function_response: gemini::FunctionResponse {
                id,
                name,
                response,
                parts: Vec::new(),
                will_continue: None,
                scheduling: None,
                extra: Default::default(),
            },
        }),
        ..Default::default()
    }
}

fn tool_result_content_to_text(content: claude::ToolResultContent) -> String {
    match content {
        claude::ToolResultContent::Text(text) => text,
        claude::ToolResultContent::Blocks(blocks) => blocks
            .into_iter()
            .filter_map(|block| match block {
                claude::ToolResultContentBlock::Text(block) => Some(block.text),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        claude::ToolResultContent::Raw(value) => value.to_string(),
    }
}

fn text_content(text: String, role: gemini::ContentRoleKnown) -> gemini::Content {
    gemini::Content {
        parts: vec![text_part(text)],
        role: Some(gemini::ContentRole::Known(role)),
        extra: Default::default(),
    }
}

fn text_part(text: String) -> gemini::Part {
    gemini::Part {
        data: Some(gemini::PartData::Text { text }),
        ..Default::default()
    }
}

fn file_part(mime_type: Option<String>, file_uri: String) -> gemini::Part {
    gemini::Part {
        data: Some(gemini::PartData::FileData {
            file_data: gemini::FileData {
                mime_type,
                file_uri,
                extra: Default::default(),
            },
        }),
        ..Default::default()
    }
}

fn image_media_type(media_type: claude::ImageMediaType) -> &'static str {
    match media_type {
        claude::ImageMediaType::Jpeg => "image/jpeg",
        claude::ImageMediaType::Png => "image/png",
        claude::ImageMediaType::Gif => "image/gif",
        claude::ImageMediaType::Webp => "image/webp",
    }
}

fn message_role_to_gemini(role: claude::MessageRole) -> gemini::ContentRoleKnown {
    match role {
        claude::MessageRole::Known(claude::MessageRoleKnown::Assistant) => {
            gemini::ContentRoleKnown::Model
        }
        claude::MessageRole::Known(claude::MessageRoleKnown::System) => {
            gemini::ContentRoleKnown::System
        }
        claude::MessageRole::Known(claude::MessageRoleKnown::User)
        | claude::MessageRole::Unknown(_) => gemini::ContentRoleKnown::User,
    }
}
