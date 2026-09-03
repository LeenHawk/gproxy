use gproxy_protocol::{gemini, openai};

use crate::TransformError;

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
    for part in parts {
        let converted = match part.data {
            Some(gemini::FunctionResponsePartData::InlineData { inline_data, .. }) => {
                Some(response_blob(inline_data)?)
            }
            Some(gemini::FunctionResponsePartData::Raw(_)) => None,
            None => {
                return Err(TransformError::shape(
                    "Gemini function response part",
                    "data is missing",
                ));
            }
            Some(_) => None,
        };
        output.extend(converted);
    }
    Ok(openai::ResponseOutput::Parts(output))
}

fn response_blob(
    blob: gemini::FunctionResponseBlob,
) -> Result<openai::ResponseToolOutputContentPart, TransformError> {
    if blob.mime_type.starts_with("image/") {
        return Ok(openai::ResponseToolOutputContentPart::InputImage(
            openai::ResponseInputImage {
                detail: None,
                file_id: None,
                image_url: Some(format!("data:{};base64,{}", blob.mime_type, blob.data)),
                prompt_cache_breakpoint: None,
                rest: Default::default(),
            },
        ));
    }
    if blob.mime_type.starts_with("audio/") {
        return Err(TransformError::unsupported(
            "Gemini function response part",
            "audio content",
        ));
    }
    let file_data = format!("data:{};base64,{}", blob.mime_type, blob.data);
    Ok(openai::ResponseToolOutputContentPart::InputFile(
        openai::ResponseInputFile {
            detail: None,
            file_data: Some(file_data),
            file_id: None,
            file_url: None,
            filename: None,
            prompt_cache_breakpoint: None,
            rest: Default::default(),
        },
    ))
}

pub(super) fn server_tool_name(value: &gemini::ServerToolType) -> Result<String, TransformError> {
    match value {
        gemini::ServerToolType::Known(value) => Ok(match value {
            gemini::ServerToolTypeKnown::ToolTypeUnspecified => "TOOL_TYPE_UNSPECIFIED",
            gemini::ServerToolTypeKnown::GoogleSearchWeb => "GOOGLE_SEARCH_WEB",
            gemini::ServerToolTypeKnown::GoogleSearchImage => "GOOGLE_SEARCH_IMAGE",
            gemini::ServerToolTypeKnown::UrlContext => "URL_CONTEXT",
            gemini::ServerToolTypeKnown::GoogleMaps => "GOOGLE_MAPS",
            gemini::ServerToolTypeKnown::FileSearch => "FILE_SEARCH",
            _ => {
                return Err(TransformError::unsupported(
                    "Gemini server tool type",
                    "future type",
                ));
            }
        }
        .to_owned()),
        gemini::ServerToolType::Unknown(value) => Err(TransformError::unsupported(
            "Gemini server tool type",
            value,
        )),
        _ => Err(TransformError::unsupported(
            "Gemini server tool type",
            "future type",
        )),
    }
}
