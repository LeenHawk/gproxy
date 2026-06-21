use crate::protocol::{claude, openai};

pub(super) fn chat_text_content_to_text(content: openai::ChatTextContent) -> String {
    match content {
        openai::ChatTextContent::Text(text) => text,
        openai::ChatTextContent::Parts(parts) => parts
            .into_iter()
            .map(|part| match part {
                openai::ChatTextContentPart::Text { text, .. } => text,
            })
            .collect::<Vec<_>>()
            .join(""),
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
                openai::ChatAssistantContentPart::Text { text, .. } => non_empty_text_block(text),
                openai::ChatAssistantContentPart::Refusal { refusal, .. } => {
                    non_empty_text_block(refusal)
                }
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
        openai::ChatContentPart::Text { text, .. } => non_empty_text_block(text),
        openai::ChatContentPart::ImageUrl { image_url, .. } => {
            Some(claude::ContentBlockParam::Image(claude::ImageBlock {
                source: image_url_to_claude_source(image_url.url),
                type_: claude::ImageBlockType::Image,
                cache_control: None,
            }))
        }
        openai::ChatContentPart::File { file, .. } => chat_file_to_claude_block(file),
        openai::ChatContentPart::InputAudio { .. } => None,
    }
}

fn chat_file_to_claude_block(file: openai::ChatFileRef) -> Option<claude::ContentBlockParam> {
    if let Some(file_id) = file.file_id {
        return Some(claude::ContentBlockParam::Document(claude::DocumentBlock {
            source: claude::DocumentSource::File(claude::FileDocumentSource {
                file_id,
                type_: claude::FileSourceType::File,
                extra: Default::default(),
            }),
            type_: claude::DocumentBlockType::Document,
            cache_control: None,
            citations: None,
            context: None,
            title: file.filename,
        }));
    }
    file.file_data.filter(|data| !data.is_empty()).map(|data| {
        claude::ContentBlockParam::Document(claude::DocumentBlock {
            source: claude::DocumentSource::Text(claude::PlainTextSource {
                data,
                media_type: claude::PlainTextMediaType::TextPlain,
                type_: claude::TextSourceType::Text,
                extra: Default::default(),
            }),
            type_: claude::DocumentBlockType::Document,
            cache_control: None,
            citations: None,
            context: None,
            title: file.filename,
        })
    })
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
    if text.is_empty() {
        None
    } else {
        Some(text_block(text))
    }
}

pub(super) fn text_block(text: String) -> claude::ContentBlockParam {
    claude::ContentBlockParam::Text(claude::TextBlock {
        text,
        type_: claude::TextBlockType::Text,
        cache_control: None,
        citations: None,
        extra: Default::default(),
    })
}

pub(super) fn mid_conversation_system_block(text: String) -> claude::ContentBlockParam {
    claude::ContentBlockParam::MidConversationSystem(claude::MidConversationSystemBlock {
        content: vec![claude::TextBlock {
            text,
            type_: claude::TextBlockType::Text,
            cache_control: None,
            citations: None,
            extra: Default::default(),
        }],
        type_: claude::MidConversationSystemBlockType::MidConversationSystem,
        cache_control: None,
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
        1 => Some(claude::StringOrArray::String(text_blocks.remove(0).text)),
        _ => Some(claude::StringOrArray::Array(text_blocks)),
    }
}
