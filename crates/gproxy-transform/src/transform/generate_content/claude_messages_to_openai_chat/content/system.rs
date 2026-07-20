use crate::protocol::{claude, openai};

use super::super::super::common::openai_breakpoint;
use super::cache::{warn_dropped_cache_breakpoint, warn_unrepresentable_cache_control};

pub(in super::super) fn push_system_message(
    messages: &mut Vec<openai::ChatCompletionMessageParam>,
    content: openai::ChatTextContent,
) {
    messages.push(openai::ChatCompletionMessageParam::System {
        content,
        name: None,
        extra: Default::default(),
    });
}

pub(in super::super) fn push_developer_message(
    messages: &mut Vec<openai::ChatCompletionMessageParam>,
    content: openai::ChatTextContent,
) {
    messages.push(openai::ChatCompletionMessageParam::Developer {
        content,
        name: None,
        extra: Default::default(),
    });
}

pub(in super::super) fn claude_system_to_chat_content(
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

pub(in super::super) fn claude_content_to_chat_text_content(
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

pub(super) fn mid_conversation_system_content(
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
