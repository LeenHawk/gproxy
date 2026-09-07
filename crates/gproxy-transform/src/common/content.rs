use gproxy_protocol::{claude, openai};

use crate::TransformError;

pub(crate) fn claude_fallback_model(
    block: &claude::ResponseContentBlock,
) -> Option<claude::ClaudeModel> {
    match block {
        claude::ResponseContentBlock::Fallback(block) => Some(block.to.model.clone()),
        // The fallback guide also shows boundary blocks without the API schema's trigger.
        claude::ResponseContentBlock::Raw(raw) if raw.get("type")?.as_str()? == "fallback" => raw
            .pointer("/to/model")?
            .as_str()
            .map(|model| model.to_owned().into()),
        _ => None,
    }
}

pub(crate) fn chat_text_blocks(
    content: openai::ChatTextContent,
) -> Result<Vec<claude::ContentBlockParam>, TransformError> {
    match content {
        openai::ChatTextContent::Text(text) => Ok(text_block(text, None).into_iter().collect()),
        openai::ChatTextContent::Parts(parts) => parts
            .into_iter()
            .map(|part| match part {
                openai::ChatTextContentPart::Text(part) => {
                    Ok(text_block(part.text, part.prompt_cache_breakpoint))
                }
                openai::ChatTextContentPart::Unknown(_) => Ok(None),
                #[cfg(not(feature = "exhaustive"))]
                _ => {
                    return Err(crate::TransformError::unsupported(
                        "protocol enum",
                        "unrecognized external variant",
                    ));
                }
            })
            .filter_map(Result::transpose)
            .collect(),
        openai::ChatTextContent::Unknown(raw) => Err(TransformError::unsupported(
            "OpenAI Chat text content",
            raw.to_string(),
        )),
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    }
}

pub(crate) fn chat_user_blocks(
    content: openai::ChatContent,
) -> Result<Vec<claude::ContentBlockParam>, TransformError> {
    match content {
        openai::ChatContent::Text(text) => Ok(text_block(text, None).into_iter().collect()),
        openai::ChatContent::Parts(parts) => parts
            .into_iter()
            .filter_map(|part| chat_part_to_claude(part).transpose())
            .collect(),
        openai::ChatContent::Unknown(raw) => Err(TransformError::unsupported(
            "OpenAI Chat content",
            raw.to_string(),
        )),
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    }
}

pub(crate) fn chat_assistant_blocks(
    content: openai::ChatAssistantContent,
) -> Result<Vec<claude::ContentBlockParam>, TransformError> {
    match content {
        openai::ChatAssistantContent::Text(text) => {
            Ok(text_block(text, None).into_iter().collect())
        }
        openai::ChatAssistantContent::Parts(parts) => parts
            .into_iter()
            .map(|part| match part {
                openai::ChatAssistantContentPart::Text(part) => {
                    Ok(text_block(part.text, part.prompt_cache_breakpoint))
                }
                openai::ChatAssistantContentPart::Refusal(part) => {
                    Ok(text_block(part.refusal, part.prompt_cache_breakpoint))
                }
                openai::ChatAssistantContentPart::Unknown(_) => Ok(None),
                #[cfg(not(feature = "exhaustive"))]
                _ => {
                    return Err(crate::TransformError::unsupported(
                        "protocol enum",
                        "unrecognized external variant",
                    ));
                }
            })
            .filter_map(Result::transpose)
            .collect(),
        openai::ChatAssistantContent::Unknown(raw) => Err(TransformError::unsupported(
            "OpenAI Chat assistant content",
            raw.to_string(),
        )),
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
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
                    Ok(openai::ChatTextContentPart::Text(crate::wire!(
                        openai::ChatTextPart {
                            type_: openai::ChatTextPartType::Text,
                            text: block.text,
                            prompt_cache_breakpoint: cache_breakpoint(block.cache_control),
                            rest: Default::default(),
                        }
                    )))
                })
                .collect::<Result<_, TransformError>>()?,
        )),
        claude::StringOrArray::Raw(raw) => Err(TransformError::unsupported(
            "Claude system prompt",
            raw.to_string(),
        )),
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
        .filter_map(|block| match block {
            claude::ContentBlockParam::Text(block) => Some(Ok(openai::ChatContentPart::Text(
                crate::wire!(openai::ChatTextPart {
                    type_: openai::ChatTextPartType::Text,
                    text: block.text,
                    prompt_cache_breakpoint: cache_breakpoint(block.cache_control),
                    rest: Default::default(),
                }),
            ))),
            claude::ContentBlockParam::Image(block) => Some(image_to_chat(block)),
            claude::ContentBlockParam::Document(block) => Some(document_to_chat(block)),
            claude::ContentBlockParam::Raw(_) => None,
            other => Some(Err(TransformError::unsupported(
                "Claude user block",
                variant_name(&other),
            ))),
        })
        .collect()
}

fn chat_part_to_claude(
    part: openai::ChatContentPart,
) -> Result<Option<claude::ContentBlockParam>, TransformError> {
    match part {
        openai::ChatContentPart::Text(part) => {
            Ok(text_block(part.text, part.prompt_cache_breakpoint))
        }
        openai::ChatContentPart::ImageUrl(part) => Ok(Some(claude::ContentBlockParam::Image(
            crate::wire!(claude::ImageBlock {
                source: image_source(part.image_url.url)?,
                type_: claude::ImageBlockType::Image,
                cache_control: cache_control(part.prompt_cache_breakpoint),
                rest: Default::default(),
            }),
        ))),
        openai::ChatContentPart::File(part) => Ok(Some(claude::ContentBlockParam::Document(
            crate::wire!(claude::DocumentBlock {
                source: document_source(&part.file)?,
                type_: claude::DocumentBlockType::Document,
                cache_control: cache_control(part.prompt_cache_breakpoint),
                citations: None,
                context: None,
                title: part.file.filename,
                rest: Default::default(),
            }),
        ))),
        openai::ChatContentPart::InputAudio(_) => Err(TransformError::unsupported(
            "OpenAI Chat content",
            "input_audio",
        )),
        openai::ChatContentPart::Unknown(_) => Ok(None),
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    }
}

fn text_block(
    text: String,
    breakpoint: Option<openai::PromptCacheBreakpoint>,
) -> Option<claude::ContentBlockParam> {
    (!text.is_empty()).then(|| {
        claude::ContentBlockParam::Text(crate::wire!(claude::TextBlock {
            text,
            type_: claude::TextBlockType::Text,
            cache_control: cache_control(breakpoint),
            citations: None,
            rest: Default::default(),
        }))
    })
}

fn cache_control(
    breakpoint: Option<openai::PromptCacheBreakpoint>,
) -> Option<claude::CacheControl> {
    breakpoint.map(|_| {
        crate::wire!(claude::CacheControl {
            type_: claude::CacheControlType::Ephemeral,
            ttl: None,
            rest: Default::default(),
        })
    })
}

fn cache_breakpoint(
    control: Option<claude::CacheControl>,
) -> Option<openai::PromptCacheBreakpoint> {
    control.map(|_| {
        crate::wire!(openai::PromptCacheBreakpoint {
            mode: openai::PromptCacheBreakpointMode::Explicit,
            rest: Default::default(),
        })
    })
}

fn image_source(url: String) -> Result<claude::ImageSource, TransformError> {
    let Some(data) = url.strip_prefix("data:") else {
        return Ok(claude::ImageSource::Url(crate::wire!(
            claude::UrlImageSource {
                type_: claude::UrlSourceType::Url,
                url,
                rest: Default::default(),
            }
        )));
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
    Ok(claude::ImageSource::Base64(crate::wire!(
        claude::Base64ImageSource {
            data: data.into(),
            media_type,
            type_: claude::Base64SourceType::Base64,
            rest: Default::default(),
        }
    )))
}

fn document_source(file: &openai::ChatFileRef) -> Result<claude::DocumentSource, TransformError> {
    if let Some(file_id) = &file.file_id {
        return Ok(claude::DocumentSource::File(crate::wire!(
            claude::FileDocumentSource {
                file_id: file_id.clone(),
                type_: claude::FileSourceType::File,
                rest: Default::default(),
            }
        )));
    }
    let data = file
        .file_data
        .clone()
        .ok_or_else(|| TransformError::shape("OpenAI Chat file", "file data is missing"))?;
    Ok(claude::DocumentSource::Text(crate::wire!(
        claude::PlainTextSource {
            data,
            media_type: claude::PlainTextMediaType::TextPlain,
            type_: claude::TextSourceType::Text,
            rest: Default::default(),
        }
    )))
}

fn image_to_chat(block: claude::ImageBlock) -> Result<openai::ChatContentPart, TransformError> {
    let url = match block.source {
        claude::ImageSource::Url(source) => source.url,
        claude::ImageSource::Base64(source) => {
            let media_type = serde_json::to_value(&source.media_type)?
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| TransformError::shape("Claude image", "media type is not text"))?;
            format!("data:{media_type};base64,{}", source.data)
        }
        claude::ImageSource::File(source) => {
            return Ok(openai::ChatContentPart::File(openai::ChatFilePart {
                type_: openai::ChatFilePartType::File,
                file: crate::wire!(openai::ChatFileRef {
                    file_data: None,
                    file_id: Some(source.file_id),
                    filename: None,
                    rest: Default::default(),
                }),
                prompt_cache_breakpoint: cache_breakpoint(block.cache_control),
                rest: Default::default(),
            }));
        }
        claude::ImageSource::Raw(raw) => {
            return Err(TransformError::unsupported(
                "Claude image source",
                raw.to_string(),
            ));
        }
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
            image_url: crate::wire!(openai::ImageUrl {
                url,
                detail: None,
                rest: Default::default(),
            }),
            prompt_cache_breakpoint: cache_breakpoint(block.cache_control),
            rest: Default::default(),
        },
    ))
}

fn document_to_chat(
    block: claude::DocumentBlock,
) -> Result<openai::ChatContentPart, TransformError> {
    let file = match block.source {
        claude::DocumentSource::File(source) => crate::wire!(openai::ChatFileRef {
            file_data: None,
            file_id: Some(source.file_id),
            filename: block.title,
            rest: Default::default(),
        }),
        claude::DocumentSource::Text(source) => crate::wire!(openai::ChatFileRef {
            file_data: Some(source.data),
            file_id: None,
            filename: block.title,
            rest: Default::default(),
        }),
        claude::DocumentSource::Raw(raw) => {
            return Err(TransformError::unsupported(
                "Claude document source",
                raw.to_string(),
            ));
        }
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
        rest: Default::default(),
    }))
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
