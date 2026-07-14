use crate::protocol::{claude, openai};

use super::super::common::claude_cache_control;

pub(super) fn chat_text_content_to_text_and_cache(
    content: openai::ChatTextContent,
) -> (String, Option<claude::CacheControl>) {
    match content {
        openai::ChatTextContent::Text(text) => (text, None),
        openai::ChatTextContent::Parts(parts) => {
            let mut text = Vec::new();
            let mut cache_control = None;
            for part in parts {
                let openai::ChatTextContentPart::Text {
                    text: part_text,
                    prompt_cache_breakpoint,
                    ..
                } = part;
                text.push(part_text);
                if prompt_cache_breakpoint.is_some() {
                    cache_control = claude_cache_control(prompt_cache_breakpoint);
                }
            }
            (text.join(""), cache_control)
        }
    }
}

pub(super) fn chat_text_content_to_claude_blocks(
    content: openai::ChatTextContent,
) -> Vec<claude::ContentBlockParam> {
    match content {
        openai::ChatTextContent::Text(text) => non_empty_text_block(text).into_iter().collect(),
        openai::ChatTextContent::Parts(parts) => parts
            .into_iter()
            .filter_map(|part| {
                let openai::ChatTextContentPart::Text {
                    text,
                    prompt_cache_breakpoint,
                    ..
                } = part;
                non_empty_marked_text_block(text, prompt_cache_breakpoint)
            })
            .collect(),
    }
}

pub(super) fn chat_assistant_content_to_claude_blocks(
    content: openai::ChatAssistantContent,
) -> Vec<claude::ContentBlockParam> {
    match content {
        openai::ChatAssistantContent::Text(text) => {
            non_empty_text_block(text).into_iter().collect()
        }
        openai::ChatAssistantContent::Parts(parts) => parts
            .into_iter()
            .filter_map(|part| match part {
                openai::ChatAssistantContentPart::Text {
                    text,
                    prompt_cache_breakpoint,
                    ..
                } => non_empty_marked_text_block(text, prompt_cache_breakpoint),
                openai::ChatAssistantContentPart::Refusal {
                    refusal,
                    prompt_cache_breakpoint,
                    ..
                } => non_empty_marked_text_block(refusal, prompt_cache_breakpoint),
            })
            .collect(),
    }
}

pub(super) fn chat_content_to_claude_blocks(
    content: openai::ChatContent,
) -> Vec<claude::ContentBlockParam> {
    match content {
        openai::ChatContent::Text(text) => non_empty_text_block(text).into_iter().collect(),
        openai::ChatContent::Parts(parts) => parts
            .into_iter()
            .filter_map(chat_content_part_to_claude_block)
            .collect(),
    }
}

fn chat_content_part_to_claude_block(
    part: openai::ChatContentPart,
) -> Option<claude::ContentBlockParam> {
    match part {
        openai::ChatContentPart::Text {
            text,
            prompt_cache_breakpoint,
            ..
        } => non_empty_marked_text_block(text, prompt_cache_breakpoint),
        openai::ChatContentPart::ImageUrl {
            image_url,
            prompt_cache_breakpoint,
            ..
        } => Some(claude::ContentBlockParam::Image(claude::ImageBlock {
            source: image_url_to_claude_source(image_url.url),
            type_: claude::ImageBlockType::Image,
            cache_control: claude_cache_control(prompt_cache_breakpoint),
        })),
        openai::ChatContentPart::File {
            file,
            prompt_cache_breakpoint,
            ..
        } => chat_file_to_claude_block(file, prompt_cache_breakpoint),
        openai::ChatContentPart::InputAudio {
            prompt_cache_breakpoint,
            ..
        } => {
            warn_dropped_openai_breakpoint(
                prompt_cache_breakpoint.as_ref(),
                "input_audio",
                "Claude message",
            );
            None
        }
    }
}

fn chat_file_to_claude_block(
    file: openai::ChatFileRef,
    breakpoint: Option<openai::PromptCacheBreakpoint>,
) -> Option<claude::ContentBlockParam> {
    let had_breakpoint = breakpoint.is_some();
    let cache_control = claude_cache_control(breakpoint);
    if let Some(file_id) = file.file_id {
        return Some(claude::ContentBlockParam::Document(claude::DocumentBlock {
            source: claude::DocumentSource::File(claude::FileDocumentSource {
                file_id,
                type_: claude::FileSourceType::File,
                extra: Default::default(),
            }),
            type_: claude::DocumentBlockType::Document,
            cache_control,
            citations: None,
            context: None,
            title: file.filename,
        }));
    }
    let block = file.file_data.filter(|data| !data.is_empty()).map(|data| {
        claude::ContentBlockParam::Document(claude::DocumentBlock {
            source: claude::DocumentSource::Text(claude::PlainTextSource {
                data,
                media_type: claude::PlainTextMediaType::TextPlain,
                type_: claude::TextSourceType::Text,
                extra: Default::default(),
            }),
            type_: claude::DocumentBlockType::Document,
            cache_control,
            citations: None,
            context: None,
            title: file.filename,
        })
    });
    if block.is_none() && had_breakpoint {
        tracing::warn!(
            block_type = "file",
            conversion_target = "Claude message",
            "cache breakpoint dropped during protocol conversion"
        );
    }
    block
}

fn image_url_to_claude_source(url: String) -> claude::ImageSource {
    parse_data_url_to_image_source(&url).unwrap_or_else(|| {
        claude::ImageSource::Url(claude::UrlImageSource {
            type_: claude::UrlSourceType::Url,
            url,
            extra: Default::default(),
        })
    })
}

fn parse_data_url_to_image_source(url: &str) -> Option<claude::ImageSource> {
    let data = url.strip_prefix("data:")?;
    let (mime, payload) = data.split_once(";base64,")?;
    let media_type = match mime {
        "image/jpeg" => claude::ImageMediaType::Jpeg,
        "image/png" => claude::ImageMediaType::Png,
        "image/gif" => claude::ImageMediaType::Gif,
        "image/webp" => claude::ImageMediaType::Webp,
        _ => return None,
    };
    Some(claude::ImageSource::Base64(claude::Base64ImageSource {
        data: payload.to_owned(),
        media_type,
        type_: claude::Base64SourceType::Base64,
        extra: Default::default(),
    }))
}

fn non_empty_text_block(text: String) -> Option<claude::ContentBlockParam> {
    non_empty_marked_text_block(text, None)
}

fn non_empty_marked_text_block(
    text: String,
    breakpoint: Option<openai::PromptCacheBreakpoint>,
) -> Option<claude::ContentBlockParam> {
    if text.trim().is_empty() {
        warn_dropped_openai_breakpoint(breakpoint.as_ref(), "text", "Claude message");
        None
    } else {
        Some(text_block_with_cache(
            text,
            claude_cache_control(breakpoint),
        ))
    }
}

fn warn_dropped_openai_breakpoint(
    breakpoint: Option<&openai::PromptCacheBreakpoint>,
    block_type: &str,
    target: &str,
) {
    if breakpoint.is_some() {
        tracing::warn!(
            block_type,
            conversion_target = target,
            "cache breakpoint dropped during protocol conversion"
        );
    }
}

pub(super) fn text_block(text: String) -> claude::ContentBlockParam {
    text_block_with_cache(text, None)
}

fn text_block_with_cache(
    text: String,
    cache_control: Option<claude::CacheControl>,
) -> claude::ContentBlockParam {
    claude::ContentBlockParam::Text(claude::TextBlock {
        text,
        type_: claude::TextBlockType::Text,
        cache_control,
        citations: None,
        extra: Default::default(),
    })
}

pub(super) fn mid_conversation_system_text_block(
    mut block: claude::TextBlock,
) -> claude::ContentBlockParam {
    let cache_control = block.cache_control.take();
    claude::ContentBlockParam::MidConversationSystem(claude::MidConversationSystemBlock {
        content: vec![block],
        type_: claude::MidConversationSystemBlockType::MidConversationSystem,
        cache_control,
    })
}

pub(super) fn push_claude_blocks(
    messages: &mut Vec<claude::MessageParam>,
    role: claude::MessageRole,
    blocks: Vec<claude::ContentBlockParam>,
) {
    for block in blocks {
        push_claude_block(messages, role.clone(), block);
    }
}

pub(super) fn push_claude_block(
    messages: &mut Vec<claude::MessageParam>,
    role: claude::MessageRole,
    block: claude::ContentBlockParam,
) {
    if let Some(last) = messages.last_mut()
        && last.role == role
    {
        match &mut last.content {
            claude::StringOrArray::String(text) => {
                let first = text_block(std::mem::take(text));
                last.content = claude::StringOrArray::Array(vec![first, block]);
            }
            claude::StringOrArray::Array(blocks) => blocks.push(block),
        }
        return;
    }
    messages.push(claude::MessageParam {
        role,
        content: claude::StringOrArray::Array(vec![block]),
        extra: Default::default(),
    });
}

pub(super) fn system_prompt(
    blocks: Vec<claude::ContentBlockParam>,
) -> Option<claude::SystemPrompt> {
    let mut text_blocks = Vec::new();
    for block in blocks {
        if let claude::ContentBlockParam::Text(block) = block {
            text_blocks.push(block);
        }
    }
    match text_blocks.len() {
        0 => None,
        1 if text_blocks[0].cache_control.is_none() => {
            Some(claude::StringOrArray::String(text_blocks.remove(0).text))
        }
        _ => Some(claude::StringOrArray::Array(text_blocks)),
    }
}
