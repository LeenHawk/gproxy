use gproxy_protocol::openai;

use crate::TransformError;

pub(super) fn response_part_to_chat(
    part: openai::ResponseInputContentPart,
) -> Result<openai::ChatContentPart, TransformError> {
    Ok(match part {
        openai::ResponseInputContentPart::InputText(part) => {
            openai::ChatContentPart::Text(openai::ChatTextPart {
                type_: openai::ChatTextPartType::Text,
                text: part.text,
                prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                rest: part.rest,
            })
        }
        openai::ResponseInputContentPart::InputImage(part) => {
            if let Some(url) = part.image_url {
                openai::ChatContentPart::ImageUrl(openai::ChatImageUrlPart {
                    type_: openai::ChatImageUrlPartType::ImageUrl,
                    image_url: openai::ImageUrl {
                        url,
                        detail: part.detail.and_then(image_detail),
                        rest: Default::default(),
                    },
                    prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                    rest: part.rest,
                })
            } else if let Some(file_id) = part.file_id {
                openai::ChatContentPart::File(openai::ChatFilePart {
                    type_: openai::ChatFilePartType::File,
                    file: openai::ChatFileRef {
                        file_data: None,
                        file_id: Some(file_id),
                        filename: None,
                        rest: Default::default(),
                    },
                    prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                    rest: part.rest,
                })
            } else {
                return Err(TransformError::shape(
                    "Responses image",
                    "URL and file id are missing",
                ));
            }
        }
        openai::ResponseInputContentPart::InputFile(part) => {
            if let Some(url) = part.file_url {
                openai::ChatContentPart::Text(openai::ChatTextPart {
                    type_: openai::ChatTextPartType::Text,
                    text: format!("Attachment URL: {url}"),
                    prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                    rest: part.rest,
                })
            } else {
                openai::ChatContentPart::File(openai::ChatFilePart {
                    type_: openai::ChatFilePartType::File,
                    file: openai::ChatFileRef {
                        file_data: part.file_data,
                        file_id: part.file_id,
                        filename: part.filename,
                        rest: Default::default(),
                    },
                    prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                    rest: part.rest,
                })
            }
        }
        openai::ResponseInputContentPart::InputAudio(part) => {
            openai::ChatContentPart::InputAudio(openai::ChatInputAudioPart {
                type_: openai::ChatInputAudioPartType::InputAudio,
                input_audio: openai::InputAudio {
                    data: part.input_audio.data,
                    format: part.input_audio.format,
                    rest: part.input_audio.rest,
                },
                prompt_cache_breakpoint: None,
                rest: part.rest,
            })
        }
    })
}

fn image_detail(detail: openai::DetailLevel) -> Option<openai::ChatImageDetailLevel> {
    match detail {
        openai::DetailLevel::Auto => Some(openai::ChatImageDetailLevel::Auto),
        openai::DetailLevel::Low => Some(openai::ChatImageDetailLevel::Low),
        openai::DetailLevel::High => Some(openai::ChatImageDetailLevel::High),
        openai::DetailLevel::Original | openai::DetailLevel::Unknown(_) => None,
    }
}
