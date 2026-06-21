use crate::protocol::{claude, openai};

use super::DEFAULT_MODEL;

pub(super) fn claude_previous_message_id_to_openai(
    diagnostics: Option<claude::DiagnosticsParam>,
) -> Option<String> {
    diagnostics?.previous_message_id?
}

pub(super) fn image_source_to_input_part(
    source: claude::ImageSource,
) -> Option<openai::ResponseInputContentPart> {
    match source {
        claude::ImageSource::File(source) => Some(openai::ResponseInputContentPart::InputImage {
            detail: None,
            file_id: Some(source.file_id),
            image_url: None,
            extra: Default::default(),
        }),
        claude::ImageSource::Url(source) => Some(openai::ResponseInputContentPart::InputImage {
            detail: None,
            file_id: None,
            image_url: Some(source.url),
            extra: Default::default(),
        }),
        claude::ImageSource::Base64(source) => Some(openai::ResponseInputContentPart::InputImage {
            detail: None,
            file_id: None,
            image_url: Some(format!(
                "data:{};base64,{}",
                image_media_type(source.media_type),
                source.data
            )),
            extra: Default::default(),
        }),
        claude::ImageSource::Raw(_) => None,
    }
}

pub(super) fn document_source_to_input_part(
    source: claude::DocumentSource,
    filename: Option<String>,
) -> Option<openai::ResponseInputContentPart> {
    match source {
        claude::DocumentSource::File(source) => Some(openai::ResponseInputContentPart::InputFile {
            detail: None,
            file_data: None,
            file_id: Some(source.file_id),
            file_url: None,
            filename,
            extra: Default::default(),
        }),
        claude::DocumentSource::Url(source) => Some(openai::ResponseInputContentPart::InputFile {
            detail: None,
            file_data: None,
            file_id: None,
            file_url: Some(source.url),
            filename,
            extra: Default::default(),
        }),
        claude::DocumentSource::Text(source) => Some(openai::ResponseInputContentPart::InputFile {
            detail: None,
            file_data: Some(source.data),
            file_id: None,
            file_url: None,
            filename,
            extra: Default::default(),
        }),
        claude::DocumentSource::Base64(source) => {
            Some(openai::ResponseInputContentPart::InputFile {
                detail: None,
                file_data: Some(format!(
                    "data:{};base64,{}",
                    pdf_media_type(source.media_type),
                    source.data
                )),
                file_id: None,
                file_url: None,
                filename,
                extra: Default::default(),
            })
        }
        claude::DocumentSource::Content(source) => {
            content_source_text(source.content).map(|file_data| {
                openai::ResponseInputContentPart::InputFile {
                    detail: None,
                    file_data: Some(file_data),
                    file_id: None,
                    file_url: None,
                    filename,
                    extra: Default::default(),
                }
            })
        }
        claude::DocumentSource::Raw(_) => None,
    }
}

pub(super) fn json_object_to_string(object: &claude::JsonObject) -> String {
    serde_json::to_string(object).unwrap_or_else(|_| "{}".to_owned())
}

fn content_source_text(content: claude::ContentSourceContent) -> Option<String> {
    let text = match content {
        claude::ContentSourceContent::Text(text) => text,
        claude::ContentSourceContent::Blocks(blocks) => {
            join_text(blocks.into_iter().filter_map(|block| match block {
                claude::ContentSourceBlock::Text(block) => Some(block.text),
                claude::ContentSourceBlock::Image(_) | claude::ContentSourceBlock::Raw(_) => None,
            }))
        }
    };
    (!text.is_empty()).then_some(text)
}

fn image_media_type(media_type: claude::ImageMediaType) -> &'static str {
    match media_type {
        claude::ImageMediaType::Jpeg => "image/jpeg",
        claude::ImageMediaType::Png => "image/png",
        claude::ImageMediaType::Gif => "image/gif",
        claude::ImageMediaType::Webp => "image/webp",
    }
}

fn pdf_media_type(media_type: claude::PdfMediaType) -> &'static str {
    match media_type {
        claude::PdfMediaType::ApplicationPdf => "application/pdf",
    }
}

pub(super) fn server_tool_name_to_string(name: &claude::ServerToolUseName) -> String {
    let Ok(value) = serde_json::to_value(name) else {
        return DEFAULT_MODEL.to_owned();
    };
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| DEFAULT_MODEL.to_owned())
}

pub(super) fn claude_usage_to_openai(usage: claude::Usage) -> openai::ResponseUsage {
    let input_tokens = u64_to_u32(usage.input_tokens.unwrap_or_default());
    let output_tokens = u64_to_u32(usage.output_tokens.unwrap_or_default());
    let cached_tokens = usage.cache_read_input_tokens.map(u64_to_u32);
    let reasoning_tokens = usage
        .output_tokens_details
        .map(|details| u64_to_u32(details.thinking_tokens))
        .unwrap_or_default();

    openai::ResponseUsage {
        input_tokens,
        output_tokens,
        total_tokens: input_tokens.saturating_add(output_tokens),
        input_tokens_details: cached_tokens.map(|cached_tokens| {
            openai::ResponseInputTokensDetails {
                cached_tokens,
                extra: Default::default(),
            }
        }),
        output_tokens_details: openai::ResponseOutputTokensDetails {
            reasoning_tokens,
            extra: Default::default(),
        },
        extra: Default::default(),
    }
}

pub(super) fn claude_system_to_text(system: claude::SystemPrompt) -> Option<String> {
    match system {
        claude::SystemPrompt::String(text) => Some(text).filter(|text| !text.is_empty()),
        claude::SystemPrompt::Array(blocks) => {
            let text = join_text(blocks.into_iter().map(|block| block.text));
            (!text.is_empty()).then_some(text)
        }
    }
}

pub(super) fn claude_service_tier_to_compact(
    service_tier: Option<claude::RequestServiceTier>,
) -> Option<openai::CompactServiceTier> {
    let service_tier = match service_tier? {
        claude::RequestServiceTier::Known(claude::RequestServiceTierKnown::Auto) => {
            openai::CompactServiceTier::Auto
        }
        claude::RequestServiceTier::Known(claude::RequestServiceTierKnown::StandardOnly)
        | claude::RequestServiceTier::Unknown(_) => openai::CompactServiceTier::Default,
    };
    Some(service_tier)
}

pub(super) fn model_to_string<T: serde::Serialize>(model: &T) -> String {
    let Ok(value) = serde_json::to_value(model) else {
        return DEFAULT_MODEL.to_owned();
    };
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| DEFAULT_MODEL.to_owned())
}

fn u64_to_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

pub(super) fn join_text(parts: impl Iterator<Item = String>) -> String {
    parts
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
