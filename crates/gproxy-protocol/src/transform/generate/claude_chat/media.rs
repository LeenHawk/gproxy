use super::content::text_block;
use crate::{
    Rest,
    transform::{Report, TransformError},
    wire::{claude::content as c, openai::chat},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::borrow::Cow;

pub(super) fn image_url(source: &c::ImageSource) -> Result<chat::ImageUrl, TransformError> {
    let url = match source {
        c::ImageSource::Url(source) => source.url.clone(),
        c::ImageSource::Base64(source) => {
            format!(
                "data:{};base64,{}",
                media_type(source.media_type),
                source.data
            )
        }
        c::ImageSource::File(_) => {
            return Err(TransformError::unsupported(
                "messages.image",
                "file image requires ResourceAccess",
            ));
        }
    };
    Ok(chat::ImageUrl {
        url,
        detail: None,
        rest: Rest::new(),
    })
}

fn media_type(media: c::ImageMediaType) -> &'static str {
    match media {
        c::ImageMediaType::Jpeg => "image/jpeg",
        c::ImageMediaType::Png => "image/png",
        c::ImageMediaType::Gif => "image/gif",
        c::ImageMediaType::Webp => "image/webp",
    }
}

pub(super) fn openai_user_blocks(
    content: &chat::UserContent,
    report: &mut Report,
) -> Result<Vec<c::ContentBlock>, TransformError> {
    let _ = report;
    let parts = match content {
        chat::UserContent::Text(text) => {
            Cow::Owned(vec![chat::UserContentPart::Text(chat::TextPart {
                type_: chat::TextPartType::Text,
                text: text.clone(),
                prompt_cache_breakpoint: None,
                rest: Rest::new(),
            })])
        }
        chat::UserContent::Parts(parts) => Cow::Borrowed(parts.as_slice()),
    };
    let mut output = Vec::new();
    for part in parts.iter() {
        match part {
            chat::UserContentPart::Text(text) => output.push(text_block(text.text.clone())),
            chat::UserContentPart::Image(image) => {
                let Some(source) =
                    crate::transform::optional(claude_image_source(image.image_url.url.clone()))?
                else {
                    continue;
                };
                output.push(c::ContentBlock::Image(c::ImageBlock {
                    type_: c::ImageBlockType::Tag,
                    source,
                    cache_control: None,
                    rest: Rest::new(),
                }))
            }
            chat::UserContentPart::File(file) => {
                if let Some(document) = crate::transform::optional(file_document(file))? {
                    output.push(c::ContentBlock::Document(document));
                }
            }
            chat::UserContentPart::InputAudio(_) => {
                report.omitted(
                    "messages.user.content",
                    "audio has no target representation",
                );
            }
        }
    }
    Ok(output)
}

fn claude_image_source(url: String) -> Result<c::ImageSource, TransformError> {
    if let Some(rest) = url.strip_prefix("data:") {
        let Some((media, data)) = rest.split_once(";base64,") else {
            return Err(TransformError::shape("messages.image", "invalid data URL"));
        };
        let media_type = match media {
            "image/jpeg" => c::ImageMediaType::Jpeg,
            "image/png" => c::ImageMediaType::Png,
            "image/gif" => c::ImageMediaType::Gif,
            "image/webp" => c::ImageMediaType::Webp,
            _ => {
                return Err(TransformError::unsupported(
                    "messages.image",
                    "unsupported image media type",
                ));
            }
        };

        Ok(c::ImageSource::Base64(c::Base64Source {
            data: data.into(),
            media_type,
            rest: Rest::new(),
        }))
    } else {
        Ok(c::ImageSource::Url(c::UrlSource {
            url,
            rest: Rest::new(),
        }))
    }
}

pub(super) fn document_file(block: &c::DocumentBlock) -> Result<chat::FileRef, TransformError> {
    let file_data = match &block.source {
        c::DocumentSource::Base64(source) => {
            Some(format!("data:application/pdf;base64,{}", source.data))
        }
        c::DocumentSource::Text(source) => Some(format!(
            "data:text/plain;base64,{}",
            STANDARD.encode(source.data.as_bytes())
        )),
        c::DocumentSource::Content(source) => {
            let text = match &source.content {
                c::DocumentContent::Text(text) => text.clone(),
                c::DocumentContent::Blocks(blocks) => blocks
                    .iter()
                    .map(|block| match block {
                        c::DocumentContentBlock::Text(text) => text.text.as_str(),
                        c::DocumentContentBlock::Image(_) => "",
                    })
                    .collect::<Vec<_>>()
                    .join(""),
            };
            Some(format!(
                "data:text/plain;base64,{}",
                STANDARD.encode(text.as_bytes())
            ))
        }
        c::DocumentSource::Url(_) | c::DocumentSource::File(_) => {
            return Err(TransformError::unsupported(
                "document.source",
                "source needs resource materialization",
            ));
        }
    };
    Ok(chat::FileRef {
        file_data,
        file_id: None,
        filename: block.title.clone(),
        rest: Rest::new(),
    })
}

fn file_document(file: &chat::FilePart) -> Result<c::DocumentBlock, TransformError> {
    let source = file.file.file_data.as_ref().ok_or_else(|| {
        TransformError::unsupported("file.file_id", "file needs resource materialization")
    })?;
    let source = if let Some(data) = source.strip_prefix("data:application/pdf;base64,") {
        c::DocumentSource::Base64(c::PdfBase64Source {
            data: data.into(),
            media_type: c::PdfMediaType::Pdf,
            rest: Rest::new(),
        })
    } else if let Some(data) = source.strip_prefix("data:text/plain;base64,") {
        let data = STANDARD.decode(data).map_err(|error| {
            TransformError::shape("file.file_data", format!("invalid text data URL: {error}"))
        })?;
        let data = String::from_utf8(data).map_err(|error| {
            TransformError::shape(
                "file.file_data",
                format!("text data URL is not UTF-8: {error}"),
            )
        })?;
        c::DocumentSource::Text(c::PlainTextSource {
            data,
            media_type: c::TextMediaType::Plain,
            rest: Rest::new(),
        })
    } else {
        return Err(TransformError::unsupported(
            "file.file_data",
            "no matching document source",
        ));
    };
    Ok(c::DocumentBlock {
        type_: c::DocumentBlockType::Tag,
        source,
        cache_control: None,
        citations: None,
        context: None,
        title: file.file.filename.clone(),
        rest: Rest::new(),
    })
}
