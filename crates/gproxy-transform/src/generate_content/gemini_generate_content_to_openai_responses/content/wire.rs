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
    serde_json::to_string(&args.unwrap_or_default()).map_err(Into::into)
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
    let mut output = vec![openai::ResponseToolOutputContentPart::InputText(
        openai::ResponseInputText {
            text: serde_json::to_string(&response)?,
            prompt_cache_breakpoint: None,
            rest: Default::default(),
        },
    )];
    for mut part in parts {
        let mut part_rest = std::mem::take(&mut part.rest);
        output.push(match part.data.take() {
            Some(gemini::FunctionResponsePartData::InlineData { inline_data, rest }) => {
                part_rest.extend(rest);
                response_blob(inline_data, part_rest)?
            }
            Some(gemini::FunctionResponsePartData::Raw(_)) => {
                return Err(TransformError::unsupported(
                    "Gemini function response part",
                    "unknown part data",
                ));
            }
            None => {
                return Err(TransformError::shape(
                    "Gemini function response part",
                    "data is missing",
                ));
            }
            Some(_) => {
                return Err(TransformError::unsupported(
                    "Gemini function response part",
                    "future part data",
                ));
            }
        });
    }
    Ok(openai::ResponseOutput::Parts(output))
}

fn response_blob(
    blob: gemini::FunctionResponseBlob,
    mut rest: openai::Rest,
) -> Result<openai::ResponseToolOutputContentPart, TransformError> {
    rest.extend(blob.rest);
    if blob.mime_type.starts_with("image/") {
        return Ok(openai::ResponseToolOutputContentPart::InputImage(
            openai::ResponseInputImage {
                detail: None,
                file_id: None,
                image_url: Some(format!("data:{};base64,{}", blob.mime_type, blob.data)),
                prompt_cache_breakpoint: None,
                rest,
            },
        ));
    }
    if blob.mime_type.starts_with("audio/") {
        return Err(TransformError::unsupported(
            "Gemini function response part",
            "audio content",
        ));
    }
    rest.insert("mime_type".into(), blob.mime_type.into());
    Ok(openai::ResponseToolOutputContentPart::InputFile(
        openai::ResponseInputFile {
            detail: None,
            file_data: Some(blob.data),
            file_id: None,
            file_url: None,
            filename: None,
            prompt_cache_breakpoint: None,
            rest,
        },
    ))
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
