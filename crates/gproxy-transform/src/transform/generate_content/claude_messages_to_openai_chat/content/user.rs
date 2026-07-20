use crate::protocol::{claude, openai};

use super::super::super::common::openai_breakpoint;
use super::super::tools::claude_tool_result_to_text;
use super::cache::{breakpoint_for_text, warn_unrepresentable_cache_control};
use super::system::{mid_conversation_system_content, push_developer_message};

pub(in super::super) fn claude_blocks_to_user_messages(
    blocks: Vec<claude::ContentBlockParam>,
) -> Vec<openai::ChatCompletionMessageParam> {
    let mut messages = Vec::new();
    let mut user_parts = Vec::new();

    for block in blocks {
        match block {
            claude::ContentBlockParam::Text(block) => {
                let prompt_cache_breakpoint = breakpoint_for_text(
                    &block.text,
                    block.cache_control,
                    "OpenAI Chat user message",
                );
                user_parts.push(openai::ChatContentPart::Text {
                    text: block.text,
                    prompt_cache_breakpoint,
                    extra: Default::default(),
                });
            }
            claude::ContentBlockParam::Image(block) => {
                if let Some(part) = claude_image_to_chat_part(block) {
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
                if let Some(content) = mid_conversation_system_content(block) {
                    push_developer_message(&mut messages, content);
                }
            }
            claude::ContentBlockParam::ToolResult(block) => {
                flush_user_parts(&mut messages, &mut user_parts);
                messages.push(openai::ChatCompletionMessageParam::Tool {
                    content: marked_chat_text_content(
                        claude_tool_result_to_text(block.content),
                        block.cache_control,
                    ),
                    tool_call_id: block.tool_use_id,
                    extra: Default::default(),
                });
            }
            claude::ContentBlockParam::McpToolResult(block) => {
                flush_user_parts(&mut messages, &mut user_parts);
                let cache_control = block.cache_control;
                messages.push(openai::ChatCompletionMessageParam::Tool {
                    content: marked_chat_text_content(
                        match block.content {
                            Some(claude::StringOrArray::String(text)) => text,
                            Some(claude::StringOrArray::Array(blocks)) => blocks
                                .into_iter()
                                .map(|block| block.text)
                                .collect::<Vec<_>>()
                                .join("\n"),
                            None => String::new(),
                        },
                        cache_control,
                    ),
                    tool_call_id: block.tool_use_id,
                    extra: Default::default(),
                });
            }
            other => warn_unrepresentable_cache_control(&other, "OpenAI Chat user message"),
        }
    }

    flush_user_parts(&mut messages, &mut user_parts);
    messages
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
            Some(openai::ChatContentPart::Text {
                text,
                prompt_cache_breakpoint: None,
                ..
            }) => openai::ChatContent::Text(text),
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

fn claude_image_to_chat_part(block: claude::ImageBlock) -> Option<openai::ChatContentPart> {
    let breakpoint = openai_breakpoint(block.cache_control);
    let url = match block.source {
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
                prompt_cache_breakpoint: breakpoint,
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
        prompt_cache_breakpoint: breakpoint,
        extra: Default::default(),
    })
}

fn claude_document_to_chat_part(block: claude::DocumentBlock) -> Option<openai::ChatContentPart> {
    let breakpoint = openai_breakpoint(block.cache_control);
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
        prompt_cache_breakpoint: breakpoint,
        extra: Default::default(),
    })
}

fn marked_chat_text_content(
    text: String,
    cache_control: Option<claude::CacheControl>,
) -> openai::ChatTextContent {
    match breakpoint_for_text(&text, cache_control, "OpenAI Chat tool result") {
        Some(prompt_cache_breakpoint) => {
            openai::ChatTextContent::Parts(vec![openai::ChatTextContentPart::Text {
                text,
                prompt_cache_breakpoint: Some(prompt_cache_breakpoint),
                extra: Default::default(),
            }])
        }
        None => openai::ChatTextContent::Text(text),
    }
}
