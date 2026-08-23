use gproxy_protocol::{claude, gemini};

use crate::TransformError;

pub(super) fn image(source: claude::ImageSource) -> Result<gemini::Part, TransformError> {
    Ok(match source {
        claude::ImageSource::Base64(source) => inline(
            image_mime(source.media_type)?.into(),
            source.data,
            checked_rest(source.rest, "Claude image source")?,
        ),
        claude::ImageSource::Url(source) => file(
            None,
            source.url,
            checked_rest(source.rest, "Claude image source")?,
        ),
        claude::ImageSource::File(source) => file(
            None,
            source.file_id,
            checked_rest(source.rest, "Claude image source")?,
        ),
        claude::ImageSource::Raw(raw) => {
            return Err(TransformError::unsupported(
                "Claude image source",
                raw.to_string(),
            ));
        }
        _ => return Err(TransformError::unsupported("Claude image", "future source")),
    })
}

pub(super) fn document(source: claude::DocumentSource) -> Result<gemini::Part, TransformError> {
    Ok(match source {
        claude::DocumentSource::Base64(source) => inline(
            "application/pdf".into(),
            source.data,
            checked_rest(source.rest, "Claude document source")?,
        ),
        claude::DocumentSource::Text(source) => super::text_part(
            source.data,
            checked_rest(source.rest, "Claude document source")?,
        ),
        claude::DocumentSource::Url(source) => file(
            None,
            source.url,
            checked_rest(source.rest, "Claude document source")?,
        ),
        claude::DocumentSource::File(source) => file(
            None,
            source.file_id,
            checked_rest(source.rest, "Claude document source")?,
        ),
        claude::DocumentSource::Raw(raw) => {
            return Err(TransformError::unsupported(
                "Claude document source",
                raw.to_string(),
            ));
        }
        other => {
            return Err(TransformError::unsupported(
                "Claude document",
                serde_json::to_string(&other)?,
            ));
        }
    })
}

fn inline(
    mime_type: String,
    data: String,
    rest: serde_json::Map<String, serde_json::Value>,
) -> gemini::Part {
    part(
        gemini::PartData::InlineData {
            inline_data: gemini::Blob {
                mime_type,
                data,
                rest: Default::default(),
            },
            rest: Default::default(),
        },
        rest,
    )
}

fn file(
    mime_type: Option<String>,
    file_uri: String,
    rest: serde_json::Map<String, serde_json::Value>,
) -> gemini::Part {
    part(
        gemini::PartData::FileData {
            file_data: gemini::FileData {
                mime_type,
                file_uri,
                rest: Default::default(),
            },
            rest: Default::default(),
        },
        rest,
    )
}

fn checked_rest(
    rest: serde_json::Map<String, serde_json::Value>,
    wire: &'static str,
) -> Result<serde_json::Map<String, serde_json::Value>, TransformError> {
    if !rest.is_empty() {
        return Err(TransformError::unsupported(wire, "source rest"));
    }
    Ok(rest)
}

fn part(data: gemini::PartData, rest: serde_json::Map<String, serde_json::Value>) -> gemini::Part {
    gemini::Part {
        thought: None,
        thought_signature: None,
        part_metadata: None,
        media_resolution: None,
        data: Some(data),
        metadata: None,
        rest,
    }
}

fn image_mime(media_type: claude::ImageMediaType) -> Result<&'static str, TransformError> {
    match media_type {
        claude::ImageMediaType::Jpeg => Ok("image/jpeg"),
        claude::ImageMediaType::Png => Ok("image/png"),
        claude::ImageMediaType::Gif => Ok("image/gif"),
        claude::ImageMediaType::Webp => Ok("image/webp"),
        _ => Err(TransformError::unsupported(
            "Claude image",
            "future media type",
        )),
    }
}
