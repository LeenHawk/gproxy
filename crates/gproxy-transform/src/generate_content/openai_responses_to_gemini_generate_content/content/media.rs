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
            rest: Default::default(),
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
                inline_part(mime, data)
            } else {
                file_part(uri, None)
            }
        }
        openai::ResponseInputContentPart::InputFile(part) => {
            if let Some(data) = part.file_data {
                let (mime, data) = data_uri(&data).ok_or_else(|| {
                    TransformError::unsupported(
                        "Responses inline input file",
                        "file_data without a media type",
                    )
                })?;
                inline_part(mime, data)
            } else {
                let uri = part.file_url.or(part.file_id).ok_or_else(|| {
                    TransformError::shape(
                        "Responses input file",
                        "file_data, file_url, and file_id are all missing",
                    )
                })?;
                file_part(uri, None)
            }
        }
        openai::ResponseInputContentPart::InputAudio(part) => {
            let mime = match part.input_audio.format {
                openai::InputAudioFormat::Wav => "audio/wav",
                openai::InputAudioFormat::Mp3 => "audio/mpeg",
                openai::InputAudioFormat::Unknown(value) => {
                    return Err(TransformError::unsupported(
                        "Responses input audio format",
                        value,
                    ));
                }
            };
            inline_part(mime.into(), part.input_audio.data)
        }
    })
}

fn inline_part(mime: String, data: String) -> gemini::Part {
    gemini::Part {
        data: Some(gemini::PartData::InlineData {
            inline_data: gemini::Blob {
                mime_type: mime,
                data,
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        rest: Default::default(),
        ..Default::default()
    }
}

fn file_part(uri: String, mime: Option<String>) -> gemini::Part {
    gemini::Part {
        data: Some(gemini::PartData::FileData {
            file_data: gemini::FileData {
                mime_type: mime,
                file_uri: uri,
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        rest: Default::default(),
        ..Default::default()
    }
}

fn data_uri(value: &str) -> Option<(String, String)> {
    let value = value.strip_prefix("data:")?;
    let (metadata, data) = value.split_once(',')?;
    let mime = metadata.strip_suffix(";base64")?;
    Some((mime.to_owned(), data.to_owned()))
}
