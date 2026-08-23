use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(super) fn input_part(
    part: openai::ResponseInputContentPart,
) -> Result<gemini::Part, TransformError> {
    Ok(match part {
        openai::ResponseInputContentPart::InputText(part) => gemini::Part {
            data: Some(gemini::PartData::Text {
                text: part.text,
                rest: Default::default(),
            }),
            rest: part.rest,
            ..Default::default()
        },
        openai::ResponseInputContentPart::InputImage(part) => {
            let uri = part.image_url.or(part.file_id).ok_or_else(|| {
                TransformError::shape(
                    "Responses input image",
                    "both image_url and file_id are missing",
                )
            })?;
            if let Some((mime, data)) = data_uri(&uri) {
                inline_part(mime, data, part.rest)
            } else {
                file_part(uri, None, part.rest)
            }
        }
        openai::ResponseInputContentPart::InputFile(mut part) => {
            let mime = part
                .rest
                .remove("mime_type")
                .and_then(|value| value.as_str().map(str::to_owned));
            if let Some(data) = part.file_data {
                let mime = mime.ok_or_else(|| {
                    TransformError::shape("Responses inline input file", "MIME type is missing")
                })?;
                inline_part(mime, data, part.rest)
            } else {
                let uri = part.file_url.or(part.file_id).ok_or_else(|| {
                    TransformError::shape(
                        "Responses input file",
                        "file_data, file_url, and file_id are all missing",
                    )
                })?;
                file_part(uri, mime, part.rest)
            }
        }
        openai::ResponseInputContentPart::InputAudio(part) => {
            let mime = format!("audio/{}", part.input_audio.format.as_str());
            inline_part(mime, part.input_audio.data, part.rest)
        }
        openai::ResponseInputContentPart::Unknown(raw) => gemini::Part {
            data: Some(gemini::PartData::Raw(raw)),
            ..Default::default()
        },
    })
}

fn inline_part(mime: String, data: String, rest: gemini::JsonMap) -> gemini::Part {
    gemini::Part {
        data: Some(gemini::PartData::InlineData {
            inline_data: gemini::Blob {
                mime_type: mime,
                data,
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        rest,
        ..Default::default()
    }
}

fn file_part(uri: String, mime: Option<String>, rest: gemini::JsonMap) -> gemini::Part {
    gemini::Part {
        data: Some(gemini::PartData::FileData {
            file_data: gemini::FileData {
                mime_type: mime,
                file_uri: uri,
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        rest,
        ..Default::default()
    }
}

fn data_uri(value: &str) -> Option<(String, String)> {
    let value = value.strip_prefix("data:")?;
    let (metadata, data) = value.split_once(',')?;
    let mime = metadata.strip_suffix(";base64")?;
    Some((mime.to_owned(), data.to_owned()))
}
