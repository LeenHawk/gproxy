use crate::protocol::{claude, openai};

use super::tools::{
    claude_response_tool_use_to_chat_tool_call, claude_tool_result_to_text,
    claude_tool_use_to_chat_tool_call,
};

pub(super) fn push_system_message(
    messages: &mut Vec<openai::ChatCompletionMessageParam>,
    text: String,
) {
    if text.is_empty() {
        return;
    }
    messages.push(openai::ChatCompletionMessageParam::System {
        content: openai::ChatTextContent::Text(text),
        name: None,
        extra: Default::default(),
    });
}

pub(super) fn push_developer_message(
    messages: &mut Vec<openai::ChatCompletionMessageParam>,
    text: String,
) {
    if text.is_empty() {
        return;
    }
    messages.push(openai::ChatCompletionMessageParam::Developer {
        content: openai::ChatTextContent::Text(text),
        name: None,
        extra: Default::default(),
    });
}

pub(super) fn claude_system_to_text(system: Option<claude::SystemPrompt>) -> Option<String> {
    let text = match system? {
        claude::StringOrArray::String(text) => text,
        claude::StringOrArray::Array(blocks) => blocks
            .into_iter()
            .map(|block| block.text)
            .collect::<Vec<_>>()
            .join(""),
    };
    (!text.is_empty()).then_some(text)
}

pub(super) fn claude_content_to_text(content: claude::MessageContent) -> String {
    match content {
        claude::StringOrArray::String(text) => text,
        claude::StringOrArray::Array(blocks) => blocks
            .into_iter()
            .filter_map(|block| match block {
                claude::ContentBlockParam::Text(block) => Some(block.text),
                claude::ContentBlockParam::MidConversationSystem(block) => {
                    Some(mid_conversation_system_text(block))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

pub(super) fn claude_blocks_to_user_messages(
    blocks: Vec<claude::ContentBlockParam>,
) -> Vec<openai::ChatCompletionMessageParam> {
    let mut messages = Vec::new();
    let mut user_parts = Vec::new();

    for block in blocks {
        match block {
            claude::ContentBlockParam::Text(block) => {
                user_parts.push(openai::ChatContentPart::Text {
                    text: block.text,
                    extra: Default::default(),
                });
            }
            claude::ContentBlockParam::Image(block) => {
                if let Some(part) = claude_image_to_chat_part(block.source) {
                    user_parts.push(part);
                }
            }
            claude::ContentBlockParam::Document(block) => {
                if let Some(part) = claude_document_to_chat_part(block) {
                    user_parts.push(part);
                }
            }
            claude::ContentBlockParam::MidConversationSystem(block) => {
                flush_user_parts(&mut messages, &mut user_parts);
                push_developer_message(&mut messages, mid_conversation_system_text(block));
            }
            claude::ContentBlockParam::ToolResult(block) => {
                flush_user_parts(&mut messages, &mut user_parts);
                messages.push(openai::ChatCompletionMessageParam::Tool {
                    content: openai::ChatTextContent::Text(claude_tool_result_to_text(
                        block.content,
                    )),
                    tool_call_id: block.tool_use_id,
                    extra: Default::default(),
                });
            }
            claude::ContentBlockParam::McpToolResult(block) => {
                flush_user_parts(&mut messages, &mut user_parts);
                messages.push(openai::ChatCompletionMessageParam::Tool {
                    content: openai::ChatTextContent::Text(match block.content {
                        Some(claude::StringOrArray::String(text)) => text,
                        Some(claude::StringOrArray::Array(blocks)) => blocks
                            .into_iter()
                            .map(|block| block.text)
                            .collect::<Vec<_>>()
                            .join("\n"),
                        None => String::new(),
                    }),
                    tool_call_id: block.tool_use_id,
                    extra: Default::default(),
                });
            }
            _ => {}
        }
    }

    flush_user_parts(&mut messages, &mut user_parts);
    messages
}

pub(super) fn claude_blocks_to_assistant_message(
    blocks: Vec<claude::ContentBlockParam>,
) -> openai::ChatCompletionMessageParam {
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in blocks {
        match block {
            claude::ContentBlockParam::Text(block) => text_parts.push(block.text),
            claude::ContentBlockParam::Thinking(block) => text_parts.push(block.thinking),
            claude::ContentBlockParam::ToolUse(block) => {
                tool_calls.push(claude_tool_use_to_chat_tool_call(block));
            }
            claude::ContentBlockParam::ServerToolUse(block) => {
                tool_calls.push(openai::ChatToolCall::Custom {
                    id: block.id,
                    custom: openai::CustomToolCall {
                        input: serde_json::to_string(&block.input)
                            .unwrap_or_else(|_| "{}".to_owned()),
                        name: serde_json::to_value(block.name)
                            .ok()
                            .and_then(|value| value.as_str().map(str::to_owned))
                            .unwrap_or_else(|| "server_tool".to_owned()),
                        extra: Default::default(),
                    },
                    extra: Default::default(),
                });
            }
            claude::ContentBlockParam::McpToolUse(block) => {
                tool_calls.push(openai::ChatToolCall::Custom {
                    id: block.id,
                    custom: openai::CustomToolCall {
                        input: serde_json::to_string(&block.input)
                            .unwrap_or_else(|_| "{}".to_owned()),
                        name: format!("mcp:{}:{}", block.server_name, block.name),
                        extra: Default::default(),
                    },
                    extra: Default::default(),
                });
            }
            _ => {}
        }
    }

    openai::ChatCompletionMessageParam::Assistant {
        content: (!text_parts.is_empty())
            .then(|| openai::ChatAssistantContent::Text(text_parts.join("\n"))),
        audio: None,
        function_call: None,
        name: None,
        reasoning_content: None,
        refusal: None,
        tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
        extra: Default::default(),
    }
}

pub(super) fn claude_response_blocks_to_chat_message(
    blocks: Vec<claude::ContentBlock>,
) -> openai::ChatMessage {
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in blocks {
        match block {
            claude::ContentBlock::Text(block) => text_parts.push(block.text),
            claude::ContentBlock::Thinking(block) => text_parts.push(block.thinking),
            claude::ContentBlock::ToolUse(block) => {
                tool_calls.push(claude_response_tool_use_to_chat_tool_call(block));
            }
            claude::ContentBlock::ServerToolUse(block) => {
                tool_calls.push(openai::ChatToolCall::Custom {
                    id: block.id,
                    custom: openai::CustomToolCall {
                        input: serde_json::to_string(&block.input)
                            .unwrap_or_else(|_| "{}".to_owned()),
                        name: serde_json::to_value(block.name)
                            .ok()
                            .and_then(|value| value.as_str().map(str::to_owned))
                            .unwrap_or_else(|| "server_tool".to_owned()),
                        extra: Default::default(),
                    },
                    extra: Default::default(),
                });
            }
            claude::ContentBlock::McpToolUse(block) => {
                tool_calls.push(openai::ChatToolCall::Custom {
                    id: block.id,
                    custom: openai::CustomToolCall {
                        input: serde_json::to_string(&block.input)
                            .unwrap_or_else(|_| "{}".to_owned()),
                        name: format!("mcp:{}:{}", block.server_name, block.name),
                        extra: Default::default(),
                    },
                    extra: Default::default(),
                });
            }
            _ => {}
        }
    }

    openai::ChatMessage {
        role: openai::ChatCompletionMessageRole::Assistant,
        content: (!text_parts.is_empty()).then(|| text_parts.join("\n")),
        refusal: None,
        annotations: None,
        audio: None,
        function_call: None,
        reasoning_content: None,
        tool_calls: (!tool_calls.is_empty()).then_some(tool_calls),
        extra: Default::default(),
    }
}

fn flush_user_parts(
    messages: &mut Vec<openai::ChatCompletionMessageParam>,
    parts: &mut Vec<openai::ChatContentPart>,
) {
    if parts.is_empty() {
        return;
    }
    let content = if parts.len() == 1 {
        match parts.pop() {
            Some(openai::ChatContentPart::Text { text, .. }) => openai::ChatContent::Text(text),
            Some(part) => openai::ChatContent::Parts(vec![part]),
            None => return,
        }
    } else {
        openai::ChatContent::Parts(std::mem::take(parts))
    };
    messages.push(openai::ChatCompletionMessageParam::User {
        content,
        name: None,
        extra: Default::default(),
    });
}

fn claude_image_to_chat_part(source: claude::ImageSource) -> Option<openai::ChatContentPart> {
    let url = match source {
        claude::ImageSource::Base64(source) => {
            let mime = match source.media_type {
                claude::ImageMediaType::Jpeg => "image/jpeg",
                claude::ImageMediaType::Png => "image/png",
                claude::ImageMediaType::Gif => "image/gif",
                claude::ImageMediaType::Webp => "image/webp",
            };
            format!("data:{mime};base64,{}", source.data)
        }
        claude::ImageSource::Url(source) => source.url,
        claude::ImageSource::File(source) => {
            return Some(openai::ChatContentPart::File {
                file: openai::ChatFileRef {
                    file_data: None,
                    file_id: Some(source.file_id),
                    filename: None,
                    extra: Default::default(),
                },
                extra: Default::default(),
            });
        }
        claude::ImageSource::Raw(_) => return None,
    };
    Some(openai::ChatContentPart::ImageUrl {
        image_url: openai::ImageUrl {
            url,
            detail: None,
            extra: Default::default(),
        },
        extra: Default::default(),
    })
}

fn claude_document_to_chat_part(block: claude::DocumentBlock) -> Option<openai::ChatContentPart> {
    let file = match block.source {
        claude::DocumentSource::File(source) => openai::ChatFileRef {
            file_data: None,
            file_id: Some(source.file_id),
            filename: block.title,
            extra: Default::default(),
        },
        claude::DocumentSource::Text(source) => openai::ChatFileRef {
            file_data: Some(source.data),
            file_id: None,
            filename: block.title,
            extra: Default::default(),
        },
        claude::DocumentSource::Base64(source) => openai::ChatFileRef {
            file_data: Some(source.data),
            file_id: None,
            filename: block.title,
            extra: Default::default(),
        },
        claude::DocumentSource::Content(_)
        | claude::DocumentSource::Url(_)
        | claude::DocumentSource::Raw(_) => {
            return None;
        }
    };
    Some(openai::ChatContentPart::File {
        file,
        extra: Default::default(),
    })
}

fn mid_conversation_system_text(block: claude::MidConversationSystemBlock) -> String {
    block
        .content
        .into_iter()
        .map(|block| block.text)
        .collect::<Vec<_>>()
        .join("")
}
