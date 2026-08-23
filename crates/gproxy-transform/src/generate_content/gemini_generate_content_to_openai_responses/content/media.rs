use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::messages::MessagePart;

pub(super) fn media_message(
    blob: gemini::Blob,
    response: bool,
    mut rest: openai::Rest,
) -> Result<MessagePart, TransformError> {
    if response {
        let raw = gemini::Part {
            data: Some(gemini::PartData::InlineData {
                inline_data: blob,
                rest: Default::default(),
            }),
            rest,
            ..Default::default()
        };
        return Ok(MessagePart::Output(
            openai::ResponseMessageOutputContentPart::Unknown(serde_json::to_value(raw)?),
        ));
    }
    let mime = blob.mime_type;
    if mime.starts_with("image/") {
        return Ok(MessagePart::Input(
            openai::ResponseInputContentPart::InputImage(openai::ResponseInputImage {
                type_: openai::ResponseInputImageType::InputImage,
                detail: None,
                file_id: None,
                image_url: Some(format!("data:{mime};base64,{}", blob.data)),
                prompt_cache_breakpoint: None,
                rest,
            }),
        ));
    }
    if mime.starts_with("audio/") {
        let format = mime
            .strip_prefix("audio/")
            .expect("audio MIME checked above")
            .to_owned();
        return Ok(MessagePart::Input(
            openai::ResponseInputContentPart::InputAudio(openai::ResponseInputAudio {
                type_: openai::ResponseInputAudioType::InputAudio,
                input_audio: openai::InputAudioContent {
                    data: blob.data,
                    format: openai::InputAudioFormat::Unknown(format),
                    rest: blob.rest,
                },
                rest,
            }),
        ));
    }
    rest.insert("mime_type".into(), mime.into());
    Ok(MessagePart::Input(
        openai::ResponseInputContentPart::InputFile(openai::ResponseInputFile {
            type_: openai::ResponseInputFileType::InputFile,
            detail: None,
            file_data: Some(blob.data),
            file_id: None,
            file_url: None,
            filename: None,
            prompt_cache_breakpoint: None,
            rest,
        }),
    ))
}

pub(super) fn file_message(
    file: gemini::FileData,
    response: bool,
    mut rest: openai::Rest,
) -> Result<MessagePart, TransformError> {
    if response {
        let raw = gemini::Part {
            data: Some(gemini::PartData::FileData {
                file_data: file,
                rest: Default::default(),
            }),
            rest,
            ..Default::default()
        };
        return Ok(MessagePart::Output(
            openai::ResponseMessageOutputContentPart::Unknown(serde_json::to_value(raw)?),
        ));
    }
    if file
        .mime_type
        .as_deref()
        .is_some_and(|mime| mime.starts_with("image/"))
    {
        return Ok(MessagePart::Input(
            openai::ResponseInputContentPart::InputImage(openai::ResponseInputImage {
                type_: openai::ResponseInputImageType::InputImage,
                detail: None,
                file_id: None,
                image_url: Some(file.file_uri),
                prompt_cache_breakpoint: None,
                rest,
            }),
        ));
    }
    if let Some(mime) = file.mime_type {
        rest.insert("mime_type".into(), mime.into());
    }
    Ok(MessagePart::Input(
        openai::ResponseInputContentPart::InputFile(openai::ResponseInputFile {
            type_: openai::ResponseInputFileType::InputFile,
            detail: None,
            file_data: None,
            file_id: None,
            file_url: Some(file.file_uri),
            filename: None,
            prompt_cache_breakpoint: None,
            rest,
        }),
    ))
}
