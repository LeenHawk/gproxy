use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::messages::MessagePart;

pub(super) fn media_message(
    blob: gemini::Blob,
    response: bool,
) -> Result<Option<MessagePart>, TransformError> {
    if response {
        return Ok(None);
    }
    let mime = blob.mime_type;
    if mime.starts_with("image/") {
        return Ok(Some(MessagePart::Input(
            openai::ResponseInputContentPart::InputImage(crate::wire!(
                openai::ResponseInputImage {
                    detail: None,
                    file_id: None,
                    image_url: Some(format!("data:{mime};base64,{}", blob.data)),
                    prompt_cache_breakpoint: None,
                    rest: Default::default(),
                }
            )),
        )));
    }
    if mime.starts_with("audio/") {
        let format = match mime.as_str() {
            "audio/wav" | "audio/x-wav" => openai::InputAudioFormat::Wav,
            "audio/mpeg" | "audio/mp3" => openai::InputAudioFormat::Mp3,
            _ => return Ok(None),
        };
        return Ok(Some(MessagePart::Input(
            openai::ResponseInputContentPart::InputAudio(crate::wire!(
                openai::ResponseInputAudio {
                    input_audio: openai::InputAudioContent {
                        data: blob.data,
                        format,
                        rest: Default::default(),
                    },
                    rest: Default::default(),
                }
            )),
        )));
    }
    Ok(Some(MessagePart::Input(
        openai::ResponseInputContentPart::InputFile(crate::wire!(openai::ResponseInputFile {
            detail: None,
            file_data: Some(format!("data:{mime};base64,{}", blob.data)),
            file_id: None,
            file_url: None,
            filename: None,
            prompt_cache_breakpoint: None,
            rest: Default::default(),
        })),
    )))
}

pub(super) fn file_message(
    file: gemini::FileData,
    response: bool,
) -> Result<Option<MessagePart>, TransformError> {
    if response {
        return Ok(None);
    }
    if file
        .mime_type
        .as_deref()
        .is_some_and(|mime| mime.starts_with("image/"))
    {
        return Ok(Some(MessagePart::Input(
            openai::ResponseInputContentPart::InputImage(crate::wire!(
                openai::ResponseInputImage {
                    detail: None,
                    file_id: None,
                    image_url: Some(file.file_uri),
                    prompt_cache_breakpoint: None,
                    rest: Default::default(),
                }
            )),
        )));
    }
    Ok(Some(MessagePart::Input(
        openai::ResponseInputContentPart::InputFile(crate::wire!(openai::ResponseInputFile {
            detail: None,
            file_data: None,
            file_id: None,
            file_url: Some(file.file_uri),
            filename: None,
            prompt_cache_breakpoint: None,
            rest: Default::default(),
        })),
    )))
}
