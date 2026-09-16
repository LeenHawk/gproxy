use crate::{
    transform::{Report, TransformError},
    wire::{claude::content as c, gemini as g},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::collections::BTreeMap;
/// MIME and public resource URI resolved by an invocation. The key is the exact
/// source URL/file ID; opaque native IDs cannot simply become target URLs.
#[derive(Debug, Default)]
pub struct MediaFacts {
    pub resources: BTreeMap<String, g::FileData>,
}
pub(super) fn image(
    block: c::ImageBlock,
    facts: &MediaFacts,
    report: &mut Report,
) -> Result<g::Part, TransformError> {
    if block.cache_control.is_some() {
        report.omitted("image.cache_control", "Gemini has no per-part cache field");
    }
    match block.source {
        c::ImageSource::Base64(source) => blob(source.data, mime(source.media_type).into()),
        c::ImageSource::Url(source) => resource(&source.url, facts, true),
        c::ImageSource::File(source) => resource(&source.file_id, facts, false),
    }
}
fn resource(key: &str, facts: &MediaFacts, is_url: bool) -> Result<g::Part, TransformError> {
    if let Some(value) = facts.resources.get(key) {
        if value.mime_type.as_deref().is_none_or(str::is_empty)
            || !(value.file_uri.starts_with("https://")
                || value.file_uri.starts_with("http://")
                || value.file_uri.starts_with("gs://"))
        {
            return Err(TransformError::missing_metadata(
                "media resolved URI and MIME",
            ));
        }
        return Ok(g::Part::builder()
            .file_data(g::FileData {
                file_uri: value.file_uri.clone(),
                mime_type: value.mime_type.clone(),
                rest: Default::default(),
            })
            .build());
    }
    if is_url && key.starts_with("data:") {
        let (header, data) = key
            .split_once(',')
            .ok_or_else(|| TransformError::shape("data_url", "missing separator"))?;
        let mime = header
            .strip_prefix("data:")
            .and_then(|v| v.strip_suffix(";base64"))
            .ok_or_else(|| TransformError::unsupported("data_url", "base64 data URL required"))?;
        return blob(data.into(), mime.into());
    }
    Err(TransformError::missing_metadata(format!(
        "media[{key}] resolved URI and MIME"
    )))
}
fn blob(data: String, mime: String) -> Result<g::Part, TransformError> {
    STANDARD
        .decode(&data)
        .map_err(|e| TransformError::shape("media.base64", e.to_string()))?;
    if mime.is_empty() {
        return Err(TransformError::missing_metadata("media.mime"));
    }
    Ok(g::Part::builder()
        .inline_data(g::Blob::builder(mime, data).build())
        .build())
}
fn mime(value: c::ImageMediaType) -> &'static str {
    match value {
        c::ImageMediaType::Jpeg => "image/jpeg",
        c::ImageMediaType::Png => "image/png",
        c::ImageMediaType::Gif => "image/gif",
        c::ImageMediaType::Webp => "image/webp",
    }
}
fn image_mime(value: &str) -> Option<c::ImageMediaType> {
    match value {
        "image/jpeg" => Some(c::ImageMediaType::Jpeg),
        "image/png" => Some(c::ImageMediaType::Png),
        "image/gif" => Some(c::ImageMediaType::Gif),
        "image/webp" => Some(c::ImageMediaType::Webp),
        _ => None,
    }
}
pub(super) fn document(
    block: c::DocumentBlock,
    facts: &MediaFacts,
    report: &mut Report,
) -> Result<Vec<g::Part>, TransformError> {
    if block.cache_control.is_some() {
        report.omitted(
            "document.cache_control",
            "Gemini has no per-part cache field",
        );
    }
    Ok(match block.source {
        c::DocumentSource::Base64(source) => vec![blob(source.data, "application/pdf".into())?],
        c::DocumentSource::Text(source) => vec![blob(
            STANDARD.encode(source.data.as_bytes()),
            "text/plain".into(),
        )?],
        c::DocumentSource::Url(source) => vec![resource(&source.url, facts, true)?],
        c::DocumentSource::File(source) => vec![resource(&source.file_id, facts, false)?],
        c::DocumentSource::Content(source) => match source.content {
            c::DocumentContent::Text(text) => vec![g::Part::builder().text(text).build()],
            c::DocumentContent::Blocks(blocks) => {
                let mut out = Vec::new();
                for block in blocks {
                    out.push(match block {
                        c::DocumentContentBlock::Text(text) => {
                            g::Part::builder().text(text.text).build()
                        }
                        c::DocumentContentBlock::Image(block) => image(block, facts, report)?,
                    });
                }
                out
            }
        },
    })
}
pub(super) fn inline(source: g::Blob) -> Result<c::ContentBlock, TransformError> {
    let bytes = STANDARD
        .decode(&source.data)
        .map_err(|e| TransformError::shape("inline_data", e.to_string()))?;
    if let Some(media_type) = image_mime(&source.mime_type) {
        return Ok(c::ContentBlock::Image(
            c::ImageBlock::builder(
                c::ImageBlockType::Tag,
                c::ImageSource::Base64(c::Base64Source::builder(source.data, media_type).build()),
            )
            .build(),
        ));
    }
    let source = match source.mime_type.as_str() {
        "application/pdf" => c::DocumentSource::Base64(
            c::PdfBase64Source::builder(source.data, c::PdfMediaType::Pdf).build(),
        ),
        "text/plain" => c::DocumentSource::Text(
            c::PlainTextSource::builder(
                String::from_utf8(bytes)
                    .map_err(|e| TransformError::shape("inline_data.text", e.to_string()))?,
                c::TextMediaType::Plain,
            )
            .build(),
        ),
        _ => {
            return Err(TransformError::unsupported(
                "inline_data.mime_type",
                "Claude supports only image, PDF or text document media in this contract",
            ));
        }
    };
    Ok(c::ContentBlock::Document(
        c::DocumentBlock::builder(c::DocumentBlockType::Tag, source).build(),
    ))
}
pub(super) fn file(source: g::FileData) -> Result<c::ContentBlock, TransformError> {
    if !source.file_uri.starts_with("https://") && !source.file_uri.starts_with("http://") {
        return Err(TransformError::missing_metadata(
            "file_data public URL or transferred Claude file ID",
        ));
    }
    let mime = source
        .mime_type
        .ok_or_else(|| TransformError::missing_metadata("file_data.mime_type"))?;
    if image_mime(&mime).is_some() {
        return Ok(c::ContentBlock::Image(
            c::ImageBlock::builder(
                c::ImageBlockType::Tag,
                c::ImageSource::Url(c::UrlSource::builder(source.file_uri).build()),
            )
            .build(),
        ));
    }
    if mime == "application/pdf" {
        return Ok(c::ContentBlock::Document(
            c::DocumentBlock::builder(
                c::DocumentBlockType::Tag,
                c::DocumentSource::Url(c::UrlSource::builder(source.file_uri).build()),
            )
            .build(),
        ));
    }
    Err(TransformError::unsupported(
        "file_data.mime_type",
        "requested file MIME needs a resource conversion",
    ))
}
