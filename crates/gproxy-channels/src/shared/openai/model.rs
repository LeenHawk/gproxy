//! Shared OpenAI request model and multipart rewriting.

use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind};
use http::HeaderMap;
use serde_json::Value;

pub(crate) fn shape(
    key: OperationKey,
    stream: bool,
    model: &str,
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Bytes, ChannelError> {
    let include_usage = stream
        && matches!(
            key.kind,
            OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat)
        );
    if model.is_empty() && !include_usage {
        return Ok(body.clone());
    }
    if !carries_model(key.operation) && !include_usage {
        return Ok(body.clone());
    }
    if is_multipart(headers) {
        return rewrite_multipart_model(headers, body, model);
    }
    let mut value = serde_json::from_slice::<Value>(body)
        .map_err(|error| ChannelError::Prepare(format!("request body is not JSON: {error}")))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| ChannelError::Prepare("request body must be a JSON object".into()))?;
    if !model.is_empty() {
        object.insert("model".into(), Value::String(model.into()));
    }
    if include_usage {
        let options = object
            .entry("stream_options")
            .or_insert_with(|| Value::Object(Default::default()));
        let options = options
            .as_object_mut()
            .ok_or_else(|| ChannelError::Prepare("stream_options must be a JSON object".into()))?;
        options.insert("include_usage".into(), Value::Bool(true));
    }
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

fn carries_model(operation: Operation) -> bool {
    matches!(
        operation,
        Operation::GenerateContent
            | Operation::StreamGenerateContent
            | Operation::CompactContent
            | Operation::CreateEmbedding
            | Operation::Rerank
            | Operation::CreateImage
            | Operation::EditImage
            | Operation::CreateSpeech
            | Operation::CreateTranscription
            | Operation::CreateTranslation
            | Operation::CreateVideo
            | Operation::RemixVideo
            | Operation::CreateVideoCharacter
            | Operation::EditVideo
            | Operation::ExtendVideo
    )
}

fn is_multipart(headers: &HeaderMap) -> bool {
    headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .to_ascii_lowercase()
                .starts_with("multipart/form-data")
        })
}

fn rewrite_multipart_model(
    headers: &HeaderMap,
    body: &Bytes,
    model: &str,
) -> Result<Bytes, ChannelError> {
    if model.is_empty() {
        return Ok(body.clone());
    }
    let content_type = headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ChannelError::Prepare("multipart content type is invalid".into()))?;
    let boundary = content_type
        .split(';')
        .find_map(|part| part.trim().strip_prefix("boundary="))
        .map(|value| value.trim_matches('"'))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelError::Prepare("multipart boundary is missing".into()))?;
    let marker = b"name=\"model\"";
    let field = find(body, marker)
        .ok_or_else(|| ChannelError::Prepare("multipart model field is missing".into()))?;
    let value_start = find(&body[field + marker.len()..], b"\r\n\r\n")
        .map(|offset| field + marker.len() + offset + 4)
        .ok_or_else(|| ChannelError::Prepare("multipart model field is malformed".into()))?;
    let delimiter = format!("\r\n--{boundary}");
    let value_end = find(&body[value_start..], delimiter.as_bytes())
        .map(|offset| value_start + offset)
        .ok_or_else(|| ChannelError::Prepare("multipart model field is unterminated".into()))?;
    let mut output = Vec::with_capacity(body.len() + model.len());
    output.extend_from_slice(&body[..value_start]);
    output.extend_from_slice(model.as_bytes());
    output.extend_from_slice(&body[value_end..]);
    Ok(Bytes::from(output))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|candidate| candidate == needle)
}
