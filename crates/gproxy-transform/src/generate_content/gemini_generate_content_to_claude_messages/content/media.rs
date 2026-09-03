use gproxy_protocol::{claude, gemini};

use crate::TransformError;

pub(super) fn inline(data: gemini::Blob) -> Result<claude::ContentBlockParam, TransformError> {
    if data.mime_type.starts_with("image/") {
        return Ok(claude::ContentBlockParam::Image(crate::wire!(
            claude::ImageBlock {
                source: claude::ImageSource::Base64(claude::Base64ImageSource {
                    data: data.data,
                    media_type: image_type(&data.mime_type)?,
                    type_: claude::Base64SourceType::Base64,
                    rest: Default::default(),
                }),
                type_: claude::ImageBlockType::Image,
                cache_control: None,
                rest: Default::default(),
            }
        )));
    }
    if data.mime_type == "application/pdf" {
        return Ok(claude::ContentBlockParam::Document(crate::wire!(
            claude::DocumentBlock {
                source: claude::DocumentSource::Base64(claude::Base64PdfSource {
                    data: data.data,
                    media_type: claude::PdfMediaType::ApplicationPdf,
                    type_: claude::Base64SourceType::Base64,
                    rest: Default::default(),
                }),
                type_: claude::DocumentBlockType::Document,
                cache_control: None,
                citations: None,
                context: None,
                title: None,
                rest: Default::default(),
            }
        )));
    }
    Err(TransformError::unsupported(
        "Gemini inline data",
        data.mime_type,
    ))
}

pub(super) fn file(data: gemini::FileData) -> Result<claude::ContentBlockParam, TransformError> {
    if data
        .mime_type
        .as_deref()
        .is_some_and(|mime| mime.starts_with("image/"))
    {
        return Ok(claude::ContentBlockParam::Image(crate::wire!(
            claude::ImageBlock {
                source: claude::ImageSource::Url(claude::UrlImageSource {
                    type_: claude::UrlSourceType::Url,
                    url: data.file_uri,
                    rest: Default::default(),
                }),
                type_: claude::ImageBlockType::Image,
                cache_control: None,
                rest: Default::default(),
            }
        )));
    }
    Ok(claude::ContentBlockParam::Document(crate::wire!(
        claude::DocumentBlock {
            source: claude::DocumentSource::Url(claude::UrlDocumentSource {
                type_: claude::UrlSourceType::Url,
                url: data.file_uri,
                rest: Default::default(),
            }),
            type_: claude::DocumentBlockType::Document,
            cache_control: None,
            citations: None,
            context: None,
            title: None,
            rest: Default::default(),
        }
    )))
}

fn image_type(mime: &str) -> Result<claude::ImageMediaType, TransformError> {
    match mime {
        "image/jpeg" | "image/jpg" => Ok(claude::ImageMediaType::Jpeg),
        "image/png" => Ok(claude::ImageMediaType::Png),
        "image/gif" => Ok(claude::ImageMediaType::Gif),
        "image/webp" => Ok(claude::ImageMediaType::Webp),
        _ => Err(TransformError::unsupported("Gemini image", mime)),
    }
}
