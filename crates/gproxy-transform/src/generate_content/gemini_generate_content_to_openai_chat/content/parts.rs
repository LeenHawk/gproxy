use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(super) fn user_part(
    part: gemini::Part,
) -> Result<Option<openai::ChatContentPart>, TransformError> {
    let Some(data) = part.data else {
        return Ok(None);
    };
    Ok(Some(match data {
        gemini::PartData::Text { text, rest } => {
            openai::ChatContentPart::Text(openai::ChatTextPart {
                type_: openai::ChatTextPartType::Text,
                text,
                prompt_cache_breakpoint: None,
                rest: merge(part.rest, rest),
            })
        }
        gemini::PartData::InlineData { inline_data, rest } => {
            inline_data_part(inline_data, merge(part.rest, rest))?
        }
        gemini::PartData::FileData { file_data, rest } => {
            file_data_part(file_data, merge(part.rest, rest))
        }
        gemini::PartData::Raw(raw) => openai::ChatContentPart::Unknown(raw),
        other => {
            return Err(TransformError::unsupported(
                "Gemini user part",
                serde_json::to_string(&other)?,
            ));
        }
    }))
}

pub(super) fn text_content(parts: Vec<openai::ChatContentPart>) -> openai::ChatContent {
    if parts.len() == 1
        && let openai::ChatContentPart::Text(part) = &parts[0]
    {
        return openai::ChatContent::Text(part.text.clone());
    }
    openai::ChatContent::Parts(parts)
}

pub(super) fn text_part(text: String, rest: openai::Rest) -> openai::ChatTextContentPart {
    openai::ChatTextContentPart::Text(openai::ChatTextPart {
        type_: openai::ChatTextPartType::Text,
        text,
        prompt_cache_breakpoint: None,
        rest,
    })
}

fn inline_data_part(
    data: gemini::Blob,
    rest: openai::Rest,
) -> Result<openai::ChatContentPart, TransformError> {
    if data.mime_type.starts_with("image/") {
        return Ok(openai::ChatContentPart::ImageUrl(
            openai::ChatImageUrlPart {
                type_: openai::ChatImageUrlPartType::ImageUrl,
                image_url: openai::ImageUrl {
                    url: format!("data:{};base64,{}", data.mime_type, data.data),
                    detail: None,
                    rest: data.rest,
                },
                prompt_cache_breakpoint: None,
                rest,
            },
        ));
    }
    let format = match data.mime_type.as_str() {
        "audio/wav" | "audio/x-wav" => Some(openai::InputAudioFormat::Wav),
        "audio/mpeg" | "audio/mp3" => Some(openai::InputAudioFormat::Mp3),
        _ => None,
    };
    if let Some(format) = format {
        return Ok(openai::ChatContentPart::InputAudio(
            openai::ChatInputAudioPart {
                type_: openai::ChatInputAudioPartType::InputAudio,
                input_audio: openai::InputAudio {
                    data: data.data,
                    format,
                    rest: data.rest,
                },
                prompt_cache_breakpoint: None,
                rest,
            },
        ));
    }
    Ok(openai::ChatContentPart::File(openai::ChatFilePart {
        type_: openai::ChatFilePartType::File,
        file: openai::ChatFileRef {
            file_data: Some(format!("data:{};base64,{}", data.mime_type, data.data)),
            file_id: None,
            filename: None,
            rest: data.rest,
        },
        prompt_cache_breakpoint: None,
        rest,
    }))
}

fn file_data_part(data: gemini::FileData, rest: openai::Rest) -> openai::ChatContentPart {
    if data
        .mime_type
        .as_ref()
        .is_some_and(|mime| mime.starts_with("image/"))
    {
        return openai::ChatContentPart::ImageUrl(openai::ChatImageUrlPart {
            type_: openai::ChatImageUrlPartType::ImageUrl,
            image_url: openai::ImageUrl {
                url: data.file_uri,
                detail: None,
                rest: data.rest,
            },
            prompt_cache_breakpoint: None,
            rest,
        });
    }
    openai::ChatContentPart::File(openai::ChatFilePart {
        type_: openai::ChatFilePartType::File,
        file: openai::ChatFileRef {
            file_data: None,
            file_id: Some(data.file_uri),
            filename: None,
            rest: data.rest,
        },
        prompt_cache_breakpoint: None,
        rest,
    })
}

pub(super) fn merge(mut left: openai::Rest, right: openai::Rest) -> openai::Rest {
    left.extend(right);
    left
}
