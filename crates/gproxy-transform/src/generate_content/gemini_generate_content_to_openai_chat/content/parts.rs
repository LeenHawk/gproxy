use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(super) fn user_part(
    part: gemini::Part,
) -> Result<Option<openai::ChatContentPart>, TransformError> {
    let Some(data) = part.data else {
        return Ok(None);
    };
    Ok(match data {
        gemini::PartData::Text { text, .. } => Some(openai::ChatContentPart::Text(crate::wire!(
            openai::ChatTextPart {
                type_: openai::ChatTextPartType::Text,
                text,
                prompt_cache_breakpoint: None,
                rest: Default::default(),
            }
        ))),
        gemini::PartData::InlineData { inline_data, .. } => Some(inline_data_part(inline_data)?),
        gemini::PartData::FileData { file_data, .. } => Some(file_data_part(file_data)),
        gemini::PartData::Raw(_) => None,
        other => {
            return Err(TransformError::unsupported(
                "Gemini user part",
                serde_json::to_string(&other)?,
            ));
        }
    })
}

pub(super) fn text_content(parts: Vec<openai::ChatContentPart>) -> openai::ChatContent {
    if parts.len() == 1
        && let openai::ChatContentPart::Text(part) = &parts[0]
    {
        return openai::ChatContent::Text(part.text.clone());
    }
    openai::ChatContent::Parts(parts)
}

pub(super) fn text_part(text: String) -> openai::ChatTextContentPart {
    openai::ChatTextContentPart::Text(crate::wire!(openai::ChatTextPart {
        type_: openai::ChatTextPartType::Text,
        text,
        prompt_cache_breakpoint: None,
        rest: Default::default(),
    }))
}

fn inline_data_part(data: gemini::Blob) -> Result<openai::ChatContentPart, TransformError> {
    if data.mime_type.starts_with("image/") {
        return Ok(openai::ChatContentPart::ImageUrl(
            openai::ChatImageUrlPart {
                type_: openai::ChatImageUrlPartType::ImageUrl,
                image_url: crate::wire!(openai::ImageUrl {
                    url: format!("data:{};base64,{}", data.mime_type, data.data),
                    detail: None,
                    rest: Default::default(),
                }),
                prompt_cache_breakpoint: None,
                rest: Default::default(),
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
                input_audio: crate::wire!(openai::InputAudio {
                    data: data.data,
                    format,
                    rest: Default::default(),
                }),
                prompt_cache_breakpoint: None,
                rest: Default::default(),
            },
        ));
    }
    Ok(openai::ChatContentPart::File(openai::ChatFilePart {
        type_: openai::ChatFilePartType::File,
        file: crate::wire!(openai::ChatFileRef {
            file_data: Some(format!("data:{};base64,{}", data.mime_type, data.data)),
            file_id: None,
            filename: None,
            rest: Default::default(),
        }),
        prompt_cache_breakpoint: None,
        rest: Default::default(),
    }))
}

fn file_data_part(data: gemini::FileData) -> openai::ChatContentPart {
    if data
        .mime_type
        .as_ref()
        .is_some_and(|mime| mime.starts_with("image/"))
    {
        return openai::ChatContentPart::ImageUrl(openai::ChatImageUrlPart {
            type_: openai::ChatImageUrlPartType::ImageUrl,
            image_url: crate::wire!(openai::ImageUrl {
                url: data.file_uri,
                detail: None,
                rest: Default::default(),
            }),
            prompt_cache_breakpoint: None,
            rest: Default::default(),
        });
    }
    openai::ChatContentPart::File(openai::ChatFilePart {
        type_: openai::ChatFilePartType::File,
        file: crate::wire!(openai::ChatFileRef {
            file_data: None,
            file_id: Some(data.file_uri),
            filename: None,
            rest: Default::default(),
        }),
        prompt_cache_breakpoint: None,
        rest: Default::default(),
    })
}
