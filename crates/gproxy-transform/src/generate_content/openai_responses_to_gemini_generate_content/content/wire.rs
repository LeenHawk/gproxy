use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(super) fn json_map(value: &str) -> Result<gemini::JsonMap, TransformError> {
    let value: serde_json::Value = serde_json::from_str(value)?;
    Ok(match value {
        serde_json::Value::Object(object) => object,
        value => [("value".into(), value)].into_iter().collect(),
    })
}

pub(super) fn function_result(
    output: openai::ResponseOutput,
) -> Result<(gemini::JsonMap, Option<Vec<gemini::FunctionResponsePart>>), TransformError> {
    match output {
        openai::ResponseOutput::Text(text) => Ok((response_map(text_value(text)), None)),
        openai::ResponseOutput::Unknown(raw) => Ok((response_map(raw), None)),
        openai::ResponseOutput::Parts(parts) => multipart_result(parts),
    }
}

pub(super) fn openai_item_rest(mut rest: gemini::JsonMap, id: Option<String>) -> gemini::JsonMap {
    if let Some(id) = id {
        rest.insert("openai_item_id".into(), id.into());
    }
    rest
}

fn multipart_result(
    parts: Vec<openai::ResponseInputContentPart>,
) -> Result<(gemini::JsonMap, Option<Vec<gemini::FunctionResponsePart>>), TransformError> {
    let mut values = Vec::new();
    let mut media = Vec::new();
    for part in parts {
        match part {
            openai::ResponseInputContentPart::InputText(part) => {
                if part.prompt_cache_breakpoint.is_some() || !part.rest.is_empty() {
                    return Err(TransformError::unsupported(
                        "Responses function output text",
                        "cache breakpoint or extension fields",
                    ));
                }
                values.push(text_value(part.text));
            }
            openai::ResponseInputContentPart::InputImage(part) => {
                if part.detail.is_some()
                    || part.file_id.is_some()
                    || part.prompt_cache_breakpoint.is_some()
                    || !part.rest.is_empty()
                {
                    return Err(TransformError::unsupported(
                        "Responses function output image",
                        "detail, file id, cache breakpoint, or extension fields",
                    ));
                }
                let uri = part.image_url.ok_or_else(|| {
                    TransformError::shape("Responses function output image", "image_url missing")
                })?;
                let (mime_type, data) = data_uri(uri)?;
                media.push(response_part(mime_type, data));
            }
            openai::ResponseInputContentPart::InputFile(mut part) => {
                if part.detail.is_some()
                    || part.file_id.is_some()
                    || part.file_url.is_some()
                    || part.filename.is_some()
                    || part.prompt_cache_breakpoint.is_some()
                {
                    return Err(TransformError::unsupported(
                        "Responses function output file",
                        "file reference, filename, detail, or cache breakpoint",
                    ));
                }
                let mime_type = part
                    .rest
                    .remove("mime_type")
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .ok_or_else(|| {
                        TransformError::shape("Responses function output file", "MIME type missing")
                    })?;
                if !part.rest.is_empty() {
                    return Err(TransformError::unsupported(
                        "Responses function output file",
                        "extension fields",
                    ));
                }
                let data = part.file_data.ok_or_else(|| {
                    TransformError::shape("Responses function output file", "file_data missing")
                })?;
                media.push(response_part(mime_type, data));
            }
            openai::ResponseInputContentPart::InputAudio(part) => {
                if !part.rest.is_empty() || !part.input_audio.rest.is_empty() {
                    return Err(TransformError::unsupported(
                        "Responses function output audio",
                        "extension fields",
                    ));
                }
                media.push(response_part(
                    format!("audio/{}", part.input_audio.format.as_str()),
                    part.input_audio.data,
                ));
            }
            openai::ResponseInputContentPart::Unknown(raw) => {
                return Err(TransformError::unsupported(
                    "Responses function output part",
                    raw.to_string(),
                ));
            }
        }
    }
    Ok((
        response_values(values),
        (!media.is_empty()).then_some(media),
    ))
}

fn response_part(mime_type: String, data: String) -> gemini::FunctionResponsePart {
    gemini::FunctionResponsePart {
        data: Some(gemini::FunctionResponsePartData::InlineData {
            inline_data: gemini::FunctionResponseBlob {
                mime_type,
                data,
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        rest: Default::default(),
    }
}

fn data_uri(uri: String) -> Result<(String, String), TransformError> {
    let value = uri.strip_prefix("data:").ok_or_else(|| {
        TransformError::unsupported("Responses function output image", "non-inline image URL")
    })?;
    let (metadata, data) = value.split_once(',').ok_or_else(|| {
        TransformError::shape("Responses function output image", "malformed data URL")
    })?;
    let mime_type = metadata.strip_suffix(";base64").ok_or_else(|| {
        TransformError::unsupported("Responses function output image", "non-base64 data URL")
    })?;
    Ok((mime_type.to_owned(), data.to_owned()))
}

fn text_value(text: String) -> serde_json::Value {
    serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text))
}

fn response_values(mut values: Vec<serde_json::Value>) -> gemini::JsonMap {
    match values.len() {
        0 => Default::default(),
        1 => response_map(values.pop().expect("one value")),
        _ => [("output".into(), serde_json::Value::Array(values))]
            .into_iter()
            .collect(),
    }
}

fn response_map(value: serde_json::Value) -> gemini::JsonMap {
    match value {
        serde_json::Value::Object(object) => object,
        value => [("output".into(), value)].into_iter().collect(),
    }
}
