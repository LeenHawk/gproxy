use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(super) fn json_map(value: &str) -> Result<gemini::JsonMap, TransformError> {
    let value: serde_json::Value =
        serde_json::from_str(value).unwrap_or_else(|_| serde_json::Value::String(value.into()));
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
        openai::ResponseOutput::Parts(parts) => multipart_result(parts),
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    }
}

fn multipart_result(
    parts: Vec<openai::ResponseToolOutputContentPart>,
) -> Result<(gemini::JsonMap, Option<Vec<gemini::FunctionResponsePart>>), TransformError> {
    let mut values = Vec::new();
    let mut media = Vec::new();
    for part in parts {
        match part {
            openai::ResponseToolOutputContentPart::InputText(part) => {
                values.push(text_value(part.text));
            }
            openai::ResponseToolOutputContentPart::InputImage(part) => {
                if let Some(uri) = part.image_url {
                    if let Ok((mime_type, data)) = data_uri(uri.clone()) {
                        media.push(response_part(mime_type, data));
                    } else {
                        values.push(serde_json::Value::String(uri));
                    }
                } else if let Some(id) = part.file_id {
                    values.push(serde_json::Value::String(id));
                }
            }
            openai::ResponseToolOutputContentPart::InputFile(part) => {
                if let Some(data) = part.file_data {
                    if let Ok((mime_type, data)) = data_uri(data.clone()) {
                        media.push(response_part(mime_type, data));
                    } else {
                        values.push(serde_json::Value::String(data));
                    }
                } else if let Some(reference) = part.file_url.or(part.file_id).or(part.filename) {
                    values.push(serde_json::Value::String(reference));
                }
            }
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
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
    crate::wire!(gemini::FunctionResponsePart {
        data: Some(gemini::FunctionResponsePartData::InlineData {
            inline_data: gemini::FunctionResponseBlob {
                mime_type,
                data,
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        rest: Default::default(),
    })
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
