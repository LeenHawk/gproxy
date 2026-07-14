use crate::protocol::{claude, openai};

use super::super::common::openai_breakpoint;
use super::tools::{
    claude_response_tool_use_to_chat_tool_call, claude_tool_result_to_text,
    claude_tool_use_to_chat_tool_call,
};

pub(super) fn push_system_message(
    messages: &mut Vec<openai::ChatCompletionMessageParam>,
    content: openai::ChatTextContent,
) {
    messages.push(openai::ChatCompletionMessageParam::System {
        content,
        name: None,
        extra: Default::default(),
    });
}

pub(super) fn push_developer_message(
    messages: &mut Vec<openai::ChatCompletionMessageParam>,
    content: openai::ChatTextContent,
) {
    messages.push(openai::ChatCompletionMessageParam::Developer {
        content,
        name: None,
        extra: Default::default(),
    });
}

pub(super) fn claude_system_to_chat_content(
    system: Option<claude::SystemPrompt>,
) -> Option<openai::ChatTextContent> {
    match system? {
        claude::StringOrArray::String(text) => {
            (!text.is_empty()).then_some(openai::ChatTextContent::Text(text))
        }
        claude::StringOrArray::Array(blocks) => {
            let parts = blocks
                .into_iter()
                .filter_map(|block| {
                    if block.text.trim().is_empty() {
                        if block.cache_control.is_some() {
                            warn_dropped_cache_breakpoint("text", "OpenAI Chat system");
                        }
                        return None;
                    }
                    Some(openai::ChatTextContentPart::Text {
                        prompt_cache_breakpoint: openai_breakpoint(block.cache_control),
                        text: block.text,
                        extra: Default::default(),
                    })
                })
                .collect::<Vec<_>>();
            (!parts.is_empty()).then_some(openai::ChatTextContent::Parts(parts))
        }
    }
}

pub(super) fn claude_content_to_chat_text_content(
    content: claude::MessageContent,
) -> Option<openai::ChatTextContent> {
    match content {
        claude::StringOrArray::String(text) => {
            (!text.is_empty()).then_some(openai::ChatTextContent::Text(text))
        }
        claude::StringOrArray::Array(blocks) => {
            let mut parts = Vec::new();
            for block in blocks {
                match block {
                    claude::ContentBlockParam::Text(block) => {
                        if !block.text.trim().is_empty() {
                            parts.push(openai::ChatTextContentPart::Text {
                                text: block.text,
                                prompt_cache_breakpoint: openai_breakpoint(block.cache_control),
                                extra: Default::default(),
                            });
                        } else if block.cache_control.is_some() {
                            warn_dropped_cache_breakpoint("text", "OpenAI Chat system message");
                        }
                    }
                    claude::ContentBlockParam::MidConversationSystem(block) => {
                        if let Some(content) = mid_conversation_system_content(block) {
                            match content {
                                openai::ChatTextContent::Text(text) => {
                                    parts.push(openai::ChatTextContentPart::Text {
                                        text,
                                        prompt_cache_breakpoint: None,
                                        extra: Default::default(),
                                    });
                                }
                                openai::ChatTextContent::Parts(nested) => parts.extend(nested),
                            }
                        }
                    }
                    other => warn_unrepresentable_cache_control(&other, "OpenAI Chat system"),
                }
            }
            (!parts.is_empty()).then_some(openai::ChatTextContent::Parts(parts))
        }
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

pub(super) fn claude_blocks_to_assistant_message(
    blocks: Vec<claude::ContentBlockParam>,
) -> openai::ChatCompletionMessageParam {
    let mut content_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in blocks {
        match block {
            claude::ContentBlockParam::Text(block) => {
                let prompt_cache_breakpoint = breakpoint_for_text(
                    &block.text,
                    block.cache_control,
                    "OpenAI Chat assistant message",
                );
                content_parts.push(openai::ChatAssistantContentPart::Text {
                    text: block.text,
                    prompt_cache_breakpoint,
                    extra: Default::default(),
                });
            }
            claude::ContentBlockParam::Thinking(block) => {
                content_parts.push(openai::ChatAssistantContentPart::Text {
                    text: block.thinking,
                    prompt_cache_breakpoint: None,
                    extra: Default::default(),
                });
            }
            claude::ContentBlockParam::ToolUse(block) => {
                if block.cache_control.is_some() {
                    tracing::warn!(
                        block_type = "tool_use",
                        target = "OpenAI Chat",
                        "cache breakpoint dropped during protocol conversion"
                    );
                }
                tool_calls.push(claude_tool_use_to_chat_tool_call(block));
            }
            claude::ContentBlockParam::ServerToolUse(block) => {
                if block.cache_control.is_some() {
                    tracing::warn!(
                        block_type = "server_tool_use",
                        target = "OpenAI Chat",
                        "cache breakpoint dropped during protocol conversion"
                    );
                }
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
                if block.cache_control.is_some() {
                    tracing::warn!(
                        block_type = "mcp_tool_use",
                        target = "OpenAI Chat",
                        "cache breakpoint dropped during protocol conversion"
                    );
                }
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
            other => warn_unrepresentable_cache_control(&other, "OpenAI Chat assistant message"),
        }
    }

    openai::ChatCompletionMessageParam::Assistant {
        content: (!content_parts.is_empty())
            .then_some(openai::ChatAssistantContent::Parts(content_parts)),
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

fn mid_conversation_system_content(
    block: claude::MidConversationSystemBlock,
) -> Option<openai::ChatTextContent> {
    let outer_breakpoint = openai_breakpoint(block.cache_control);
    let mut parts = block
        .content
        .into_iter()
        .filter_map(|block| {
            if block.text.trim().is_empty() {
                if block.cache_control.is_some() {
                    warn_dropped_cache_breakpoint("text", "OpenAI Chat mid-conversation system");
                }
                return None;
            }
            Some(openai::ChatTextContentPart::Text {
                text: block.text,
                prompt_cache_breakpoint: openai_breakpoint(block.cache_control),
                extra: Default::default(),
            })
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        if outer_breakpoint.is_some() {
            warn_dropped_cache_breakpoint("mid_conversation_system", "OpenAI Chat");
        }
        return None;
    }
    if let Some(breakpoint) = outer_breakpoint
        && let Some(openai::ChatTextContentPart::Text {
            prompt_cache_breakpoint,
            ..
        }) = parts.last_mut()
    {
        prompt_cache_breakpoint.get_or_insert(breakpoint);
    }
    Some(openai::ChatTextContent::Parts(parts))
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

fn breakpoint_for_text(
    text: &str,
    cache_control: Option<claude::CacheControl>,
    target: &str,
) -> Option<openai::PromptCacheBreakpoint> {
    if text.trim().is_empty() {
        if cache_control.is_some() {
            warn_dropped_cache_breakpoint("text", target);
        }
        None
    } else {
        openai_breakpoint(cache_control)
    }
}

fn warn_dropped_cache_breakpoint(block_type: &str, target: &str) {
    tracing::warn!(
        block_type,
        conversion_target = target,
        "cache breakpoint dropped during protocol conversion"
    );
}

fn warn_unrepresentable_cache_control(block: &claude::ContentBlockParam, target: &str) {
    let Ok(value) = serde_json::to_value(block) else {
        return;
    };
    if value.get("cache_control").is_some() {
        warn_dropped_cache_breakpoint(
            value
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown"),
            target,
        );
    }
}
