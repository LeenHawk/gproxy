use gproxy_protocol::{claude, openai};

use crate::TransformError;

pub(crate) fn chat_text_blocks(
    content: openai::ChatTextContent,
) -> Result<Vec<claude::ContentBlockParam>, TransformError> {
    match content {
        openai::ChatTextContent::Text(text) => Ok(text_block(text, None, Default::default())
            .into_iter()
            .collect()),
        openai::ChatTextContent::Parts(parts) => parts
            .into_iter()
            .map(|part| match part {
                openai::ChatTextContentPart::Text(part) => Ok(text_block(
                    part.text,
                    part.prompt_cache_breakpoint,
                    part.rest,
                )),
                openai::ChatTextContentPart::Unknown(raw) => {
                    Ok(Some(claude::ContentBlockParam::Raw(raw)))
                }
            })
            .filter_map(Result::transpose)
            .collect(),
        openai::ChatTextContent::Unknown(raw) => Ok(vec![claude::ContentBlockParam::Raw(raw)]),
    }
}

pub(crate) fn chat_user_blocks(
    content: openai::ChatContent,
) -> Result<Vec<claude::ContentBlockParam>, TransformError> {
    match content {
        openai::ChatContent::Text(text) => Ok(text_block(text, None, Default::default())
            .into_iter()
            .collect()),
        openai::ChatContent::Parts(parts) => parts.into_iter().map(chat_part_to_claude).collect(),
        openai::ChatContent::Unknown(raw) => Ok(vec![claude::ContentBlockParam::Raw(raw)]),
    }
}

pub(crate) fn chat_assistant_blocks(
    content: openai::ChatAssistantContent,
) -> Result<Vec<claude::ContentBlockParam>, TransformError> {
    match content {
        openai::ChatAssistantContent::Text(text) => Ok(text_block(text, None, Default::default())
            .into_iter()
            .collect()),
        openai::ChatAssistantContent::Parts(parts) => parts
            .into_iter()
            .map(|part| match part {
                openai::ChatAssistantContentPart::Text(part) => Ok(text_block(
                    part.text,
                    part.prompt_cache_breakpoint,
                    part.rest,
                )),
                openai::ChatAssistantContentPart::Refusal(part) => Ok(text_block(
                    part.refusal,
                    part.prompt_cache_breakpoint,
                    part.rest,
                )),
                openai::ChatAssistantContentPart::Unknown(raw) => {
                    Ok(Some(claude::ContentBlockParam::Raw(raw)))
                }
            })
            .filter_map(Result::transpose)
            .collect(),
        openai::ChatAssistantContent::Unknown(raw) => Ok(vec![claude::ContentBlockParam::Raw(raw)]),
    }
}

pub(crate) fn claude_system_to_chat(
    system: claude::SystemPrompt,
) -> Result<openai::ChatTextContent, TransformError> {
    match system {
        claude::StringOrArray::String(text) => Ok(openai::ChatTextContent::Text(text)),
        claude::StringOrArray::Array(blocks) => Ok(openai::ChatTextContent::Parts(
            blocks
                .into_iter()
                .map(|block| {
                    Ok(openai::ChatTextContentPart::Text(openai::ChatTextPart {
                        type_: openai::ChatTextPartType::Text,
                        text: block.text,
                        prompt_cache_breakpoint: cache_breakpoint(block.cache_control),
                        rest: merge_rest(block.rest, block.citations, "citations")?,
                    }))
                })
                .collect::<Result<_, TransformError>>()?,
        )),
        claude::StringOrArray::Raw(raw) => Ok(openai::ChatTextContent::Unknown(raw)),
        _ => Err(TransformError::unsupported(
            "Claude system prompt",
            "future system shape",
        )),
    }
}

pub(crate) fn claude_user_parts(
    blocks: Vec<claude::ContentBlockParam>,
) -> Result<Vec<openai::ChatContentPart>, TransformError> {
    blocks
        .into_iter()
        .map(|block| match block {
            claude::ContentBlockParam::Text(block) => {
                Ok(openai::ChatContentPart::Text(openai::ChatTextPart {
                    type_: openai::ChatTextPartType::Text,
                    text: block.text,
                    prompt_cache_breakpoint: cache_breakpoint(block.cache_control),
                    rest: merge_rest(block.rest, block.citations, "citations")?,
                }))
            }
            claude::ContentBlockParam::Image(block) => image_to_chat(block),
            claude::ContentBlockParam::Document(block) => document_to_chat(block),
            claude::ContentBlockParam::Raw(raw) => Ok(openai::ChatContentPart::Unknown(raw)),
            other => Err(TransformError::unsupported(
                "Claude user block",
                variant_name(&other),
            )),
        })
        .collect()
}

fn chat_part_to_claude(
    part: openai::ChatContentPart,
) -> Result<claude::ContentBlockParam, TransformError> {
    match part {
        openai::ChatContentPart::Text(part) => {
            text_block(part.text, part.prompt_cache_breakpoint, part.rest)
                .ok_or_else(|| TransformError::shape("OpenAI Chat", "empty text part"))
        }
        openai::ChatContentPart::ImageUrl(part) => {
            Ok(claude::ContentBlockParam::Image(claude::ImageBlock {
                source: image_source(part.image_url.url)?,
                type_: claude::ImageBlockType::Image,
                cache_control: cache_control(part.prompt_cache_breakpoint),
                rest: part.rest,
            }))
        }
        openai::ChatContentPart::File(part) => {
            Ok(claude::ContentBlockParam::Document(claude::DocumentBlock {
                source: document_source(&part.file)?,
                type_: claude::DocumentBlockType::Document,
                cache_control: cache_control(part.prompt_cache_breakpoint),
                citations: None,
                context: None,
                title: part.file.filename,
                rest: part.rest,
            }))
        }
        openai::ChatContentPart::InputAudio(_) => Err(TransformError::unsupported(
            "OpenAI Chat content",
            "input_audio",
        )),
        openai::ChatContentPart::Unknown(raw) => Ok(claude::ContentBlockParam::Raw(raw)),
    }
}

fn text_block(
    text: String,
    breakpoint: Option<openai::PromptCacheBreakpoint>,
    rest: openai::Rest,
) -> Option<claude::ContentBlockParam> {
    (!text.is_empty()).then(|| {
        claude::ContentBlockParam::Text(claude::TextBlock {
            text,
            type_: claude::TextBlockType::Text,
            cache_control: cache_control(breakpoint),
            citations: None,
            rest,
        })
    })
}

fn cache_control(
    breakpoint: Option<openai::PromptCacheBreakpoint>,
) -> Option<claude::CacheControl> {
    breakpoint.map(|breakpoint| claude::CacheControl {
        type_: claude::CacheControlType::Ephemeral,
        ttl: None,
        rest: breakpoint.rest,
    })
}

fn cache_breakpoint(
    control: Option<claude::CacheControl>,
) -> Option<openai::PromptCacheBreakpoint> {
    control.map(|control| openai::PromptCacheBreakpoint {
        mode: openai::PromptCacheBreakpointMode::Explicit,
        rest: control.rest,
    })
}

fn image_source(url: String) -> Result<claude::ImageSource, TransformError> {
    let Some(data) = url.strip_prefix("data:") else {
        return Ok(claude::ImageSource::Url(claude::UrlImageSource {
            type_: claude::UrlSourceType::Url,
            url,
            rest: Default::default(),
        }));
    };
    let (media_type, data) = data
        .split_once(";base64,")
        .ok_or_else(|| TransformError::shape("image URL", "invalid data URL"))?;
    let media_type = match media_type {
        "image/jpeg" => claude::ImageMediaType::Jpeg,
        "image/png" => claude::ImageMediaType::Png,
        "image/gif" => claude::ImageMediaType::Gif,
        "image/webp" => claude::ImageMediaType::Webp,
        other => return Err(TransformError::unsupported("image media type", other)),
    };
    Ok(claude::ImageSource::Base64(claude::Base64ImageSource {
        data: data.into(),
        media_type,
        type_: claude::Base64SourceType::Base64,
        rest: Default::default(),
    }))
}

fn document_source(file: &openai::ChatFileRef) -> Result<claude::DocumentSource, TransformError> {
    if let Some(file_id) = &file.file_id {
        return Ok(claude::DocumentSource::File(claude::FileDocumentSource {
            file_id: file_id.clone(),
            type_: claude::FileSourceType::File,
            rest: file.rest.clone(),
        }));
    }
    let data = file
        .file_data
        .clone()
        .ok_or_else(|| TransformError::shape("OpenAI Chat file", "file data is missing"))?;
    Ok(claude::DocumentSource::Text(claude::PlainTextSource {
        data,
        media_type: claude::PlainTextMediaType::TextPlain,
        type_: claude::TextSourceType::Text,
        rest: file.rest.clone(),
    }))
}

fn image_to_chat(block: claude::ImageBlock) -> Result<openai::ChatContentPart, TransformError> {
    let (url, source_rest) = match block.source {
        claude::ImageSource::Url(source) => (source.url, source.rest),
        claude::ImageSource::Base64(source) => {
            let media_type = serde_json::to_value(&source.media_type)?
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| TransformError::shape("Claude image", "media type is not text"))?;
            (
                format!("data:{media_type};base64,{}", source.data),
                source.rest,
            )
        }
        claude::ImageSource::File(source) => {
            return Ok(openai::ChatContentPart::File(openai::ChatFilePart {
                type_: openai::ChatFilePartType::File,
                file: openai::ChatFileRef {
                    file_data: None,
                    file_id: Some(source.file_id),
                    filename: None,
                    rest: source.rest,
                },
                prompt_cache_breakpoint: cache_breakpoint(block.cache_control),
                rest: block.rest,
            }));
        }
        claude::ImageSource::Raw(raw) => return Ok(openai::ChatContentPart::Unknown(raw)),
        _ => {
            return Err(TransformError::unsupported(
                "Claude image source",
                "future image source",
            ));
        }
    };
    Ok(openai::ChatContentPart::ImageUrl(
        openai::ChatImageUrlPart {
            type_: openai::ChatImageUrlPartType::ImageUrl,
            image_url: openai::ImageUrl {
                url,
                detail: None,
                rest: source_rest,
            },
            prompt_cache_breakpoint: cache_breakpoint(block.cache_control),
            rest: block.rest,
        },
    ))
}

fn document_to_chat(
    block: claude::DocumentBlock,
) -> Result<openai::ChatContentPart, TransformError> {
    let file = match block.source {
        claude::DocumentSource::File(source) => openai::ChatFileRef {
            file_data: None,
            file_id: Some(source.file_id),
            filename: block.title,
            rest: source.rest,
        },
        claude::DocumentSource::Text(source) => openai::ChatFileRef {
            file_data: Some(source.data),
            file_id: None,
            filename: block.title,
            rest: source.rest,
        },
        claude::DocumentSource::Raw(raw) => return Ok(openai::ChatContentPart::Unknown(raw)),
        _ => {
            return Err(TransformError::unsupported(
                "Claude document source",
                "non-file document",
            ));
        }
    };
    Ok(openai::ChatContentPart::File(openai::ChatFilePart {
        type_: openai::ChatFilePartType::File,
        file,
        prompt_cache_breakpoint: cache_breakpoint(block.cache_control),
        rest: block.rest,
    }))
}

fn merge_rest<T: serde::Serialize>(
    mut rest: serde_json::Map<String, serde_json::Value>,
    value: Option<T>,
    name: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, TransformError> {
    if let Some(value) = value {
        rest.insert(name.into(), serde_json::to_value(value)?);
    }
    Ok(rest)
}

fn variant_name(block: &claude::ContentBlockParam) -> &'static str {
    match block {
        claude::ContentBlockParam::Text(_) => "text",
        claude::ContentBlockParam::Image(_) => "image",
        claude::ContentBlockParam::Document(_) => "document",
        claude::ContentBlockParam::ToolUse(_) => "tool_use",
        claude::ContentBlockParam::ToolResult(_) => "tool_result",
        claude::ContentBlockParam::Thinking(_) => "thinking",
        claude::ContentBlockParam::RedactedThinking(_) => "redacted_thinking",
        claude::ContentBlockParam::Raw(_) => "raw",
        _ => "provider-specific block",
    }
}
