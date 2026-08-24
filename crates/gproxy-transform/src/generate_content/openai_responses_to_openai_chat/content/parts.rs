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
            openai::ChatContentPart::ImageUrl(openai::ChatImageUrlPart {
                type_: openai::ChatImageUrlPartType::ImageUrl,
                image_url: openai::ImageUrl {
                    url: part.image_url.ok_or_else(|| {
                        TransformError::shape("Responses image", "URL is missing")
                    })?,
                    detail: None,
                    rest: Default::default(),
                },
                prompt_cache_breakpoint: part.prompt_cache_breakpoint,
                rest: part.rest,
            })
        }
        openai::ResponseInputContentPart::InputFile(part) => {
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
