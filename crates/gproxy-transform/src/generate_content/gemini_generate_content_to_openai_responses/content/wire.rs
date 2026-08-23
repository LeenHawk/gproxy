use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(super) fn part_rest(part: &mut gemini::Part) -> Result<openai::Rest, TransformError> {
    let mut rest = std::mem::take(&mut part.rest);
    preserve(&mut rest, "gemini_part_metadata", part.part_metadata.take())?;
    preserve(
        &mut rest,
        "gemini_media_resolution",
        part.media_resolution.take(),
    )?;
    preserve(&mut rest, "gemini_metadata", part.metadata.take())?;
    Ok(rest)
}

pub(super) fn arguments(args: Option<gemini::JsonMap>) -> Result<String, TransformError> {
    serde_json::to_string(
        &args.ok_or_else(|| TransformError::shape("Gemini function call", "args is missing"))?,
    )
    .map_err(Into::into)
}

pub(super) fn output(response: gemini::JsonMap) -> Result<openai::ResponseOutput, TransformError> {
    Ok(openai::ResponseOutput::Text(serde_json::to_string(
        &response,
    )?))
}

pub(super) fn function_output(
    response: gemini::JsonMap,
    parts: Option<Vec<gemini::FunctionResponsePart>>,
) -> Result<openai::ResponseOutput, TransformError> {
    let mut parts = parts.into_iter().flatten().peekable();
    if parts.peek().is_none() {
        return output(response);
    }
    let mut output = vec![openai::ResponseInputContentPart::InputText(
        openai::ResponseInputText {
            type_: openai::ResponseInputTextType::InputText,
            text: serde_json::to_string(&response)?,
            prompt_cache_breakpoint: None,
            rest: Default::default(),
        },
    )];
    for mut part in parts {
        let encoded = serde_json::to_value(&part)?;
        let mut part_rest = std::mem::take(&mut part.rest);
        output.push(match part.data.take() {
            Some(gemini::FunctionResponsePartData::InlineData { inline_data, rest }) => {
                part_rest.extend(rest);
                response_blob(inline_data, part_rest)
            }
            Some(gemini::FunctionResponsePartData::Raw(raw)) => {
                if part_rest.is_empty() {
                    openai::ResponseInputContentPart::Unknown(raw)
                } else {
                    openai::ResponseInputContentPart::Unknown(encoded)
                }
            }
            None => openai::ResponseInputContentPart::Unknown(encoded),
            Some(_) => openai::ResponseInputContentPart::Unknown(encoded),
        });
    }
    Ok(openai::ResponseOutput::Parts(output))
}

fn response_blob(
    blob: gemini::FunctionResponseBlob,
    mut rest: openai::Rest,
) -> openai::ResponseInputContentPart {
    rest.extend(blob.rest);
    if blob.mime_type.starts_with("image/") {
        return openai::ResponseInputContentPart::InputImage(openai::ResponseInputImage {
            type_: openai::ResponseInputImageType::InputImage,
            detail: None,
            file_id: None,
            image_url: Some(format!("data:{};base64,{}", blob.mime_type, blob.data)),
            prompt_cache_breakpoint: None,
            rest,
        });
    }
    if blob.mime_type.starts_with("audio/") {
        let format = blob
            .mime_type
            .strip_prefix("audio/")
            .expect("audio MIME checked above")
            .to_owned();
        return openai::ResponseInputContentPart::InputAudio(openai::ResponseInputAudio {
            type_: openai::ResponseInputAudioType::InputAudio,
            input_audio: openai::InputAudioContent {
                data: blob.data,
                format: openai::InputAudioFormat::Unknown(format),
                rest: Default::default(),
            },
            rest,
        });
    }
    rest.insert("mime_type".into(), blob.mime_type.into());
    openai::ResponseInputContentPart::InputFile(openai::ResponseInputFile {
        type_: openai::ResponseInputFileType::InputFile,
        detail: None,
        file_data: Some(blob.data),
        file_id: None,
        file_url: None,
        filename: None,
        prompt_cache_breakpoint: None,
        rest,
    })
}

pub(super) fn server_tool_name(value: &gemini::ServerToolType) -> Result<String, TransformError> {
    serde_json::to_value(value)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| TransformError::shape("Gemini server tool", "expected a string"))
}

fn preserve<T: serde::Serialize>(
    rest: &mut openai::Rest,
    key: &str,
    value: Option<T>,
) -> Result<(), TransformError> {
    if let Some(value) = value {
        rest.insert(key.into(), serde_json::to_value(value)?);
    }
    Ok(())
}
