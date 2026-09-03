use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::parts::text_part;

pub(super) fn user_part(
    part: openai::ChatContentPart,
) -> Result<Option<gemini::Part>, TransformError> {
    Ok(Some(match part {
        openai::ChatContentPart::Text(part) => text_part(part.text, false),
        openai::ChatContentPart::ImageUrl(part) => uri_part(part.image_url.url),
        openai::ChatContentPart::InputAudio(part) => audio_part(part)?,
        openai::ChatContentPart::File(part) => file_part(part)?,
        openai::ChatContentPart::Unknown(_) => return Ok(None),
    }))
}

fn audio_part(part: openai::ChatInputAudioPart) -> Result<gemini::Part, TransformError> {
    let mime_type = match part.input_audio.format {
        openai::InputAudioFormat::Wav => "audio/wav",
        openai::InputAudioFormat::Mp3 => "audio/mpeg",
        openai::InputAudioFormat::Unknown(value) => {
            return Err(TransformError::unsupported(
                "Chat input audio format",
                value,
            ));
        }
    };
    Ok(gemini::Part {
        data: Some(gemini::PartData::InlineData {
            inline_data: gemini::Blob {
                mime_type: mime_type.into(),
                data: part.input_audio.data,
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        rest: Default::default(),
        ..Default::default()
    })
}

fn file_part(part: openai::ChatFilePart) -> Result<gemini::Part, TransformError> {
    if let Some(data) = part.file.file_data {
        let (mime_type, data) = data_url(&data).ok_or_else(|| {
            TransformError::shape("Chat file", "file_data has no MIME-bearing data URL")
        })?;
        return Ok(inline_part(mime_type, data));
    }
    let uri = part
        .file
        .file_id
        .ok_or_else(|| TransformError::shape("Chat file", "file_id is missing"))?;
    Ok(file_uri_part(uri, None))
}

fn uri_part(uri: String) -> gemini::Part {
    match data_url(&uri) {
        Some((mime, data)) => inline_part(mime, data),
        None => file_uri_part(uri, None),
    }
}

fn inline_part(mime_type: String, data: String) -> gemini::Part {
    gemini::Part {
        data: Some(gemini::PartData::InlineData {
            inline_data: gemini::Blob {
                mime_type,
                data,
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        rest: Default::default(),
        ..Default::default()
    }
}

fn file_uri_part(uri: String, mime_type: Option<String>) -> gemini::Part {
    gemini::Part {
        data: Some(gemini::PartData::FileData {
            file_data: gemini::FileData {
                mime_type,
                file_uri: uri,
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        rest: Default::default(),
        ..Default::default()
    }
}

fn data_url(value: &str) -> Option<(String, String)> {
    let value = value.strip_prefix("data:")?;
    let (mime, data) = value.split_once(";base64,")?;
    Some((mime.into(), data.into()))
}
