use base64::Engine as _;
use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use http::HeaderValue;
use serde_json::Value;

use super::Part;

const DROP: &[&str] = &[
    "model",
    "prompt",
    "response_format",
    "stream",
    "temperature",
    "timestamp_granularities",
];

pub(super) fn from_json(body: &[u8]) -> Result<Vec<Part>, ChannelError> {
    let object = serde_json::from_slice::<Value>(body)
        .map_err(|error| ChannelError::Prepare(format!("transcription body JSON: {error}")))?
        .as_object()
        .cloned()
        .ok_or_else(|| ChannelError::Prepare("transcription body must be an object".into()))?;
    object
        .into_iter()
        .map(|(name, value)| value_part(name, value))
        .collect()
}

pub(super) fn stt(mut parts: Vec<Part>) -> Result<(Bytes, HeaderValue), ChannelError> {
    parts.retain(|part| !DROP.contains(&part.name.trim_end_matches("[]")));
    parts.sort_by_key(|part| part.name.trim_end_matches("[]") == "file");
    if !parts
        .iter()
        .any(|part| part.name.trim_end_matches("[]") == "file")
    {
        return Err(ChannelError::Prepare("transcription file missing".into()));
    }
    let boundary = boundary(&parts);
    let mut output = Vec::new();
    for part in parts {
        safe_name(&part.name)?;
        output.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        if part.file {
            let mime = part.mime.as_deref().unwrap_or("application/octet-stream");
            output.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{}\"; filename=\"audio.{}\"\r\nContent-Type: {mime}\r\n\r\n",
                    part.name,
                    extension(mime)
                )
                .as_bytes(),
            );
        } else {
            output.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{}\"\r\n\r\n",
                    part.name
                )
                .as_bytes(),
            );
        }
        output.extend_from_slice(&part.data);
        output.extend_from_slice(b"\r\n");
    }
    output.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let content_type = HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}"))
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    Ok((Bytes::from(output), content_type))
}

fn value_part(name: String, value: Value) -> Result<Part, ChannelError> {
    if name == "file" {
        let data_url = value
            .as_str()
            .ok_or_else(|| ChannelError::Prepare("transcription file must be a data URL".into()))?;
        let payload = data_url
            .strip_prefix("data:")
            .and_then(|value| value.split_once(','))
            .ok_or_else(|| ChannelError::Prepare("transcription data URL malformed".into()))?;
        let mime = payload
            .0
            .strip_suffix(";base64")
            .ok_or_else(|| ChannelError::Prepare("transcription file must be base64".into()))?;
        let data = base64::engine::general_purpose::STANDARD
            .decode(payload.1)
            .map_err(|error| ChannelError::Prepare(format!("transcription base64: {error}")))?;
        return Ok(Part {
            name,
            file: true,
            mime: Some(mime.into()),
            data,
        });
    }
    let text = match value {
        Value::String(value) => value,
        value => serde_json::to_string(&value)
            .map_err(|error| ChannelError::Prepare(error.to_string()))?,
    };
    Ok(Part {
        name,
        file: false,
        mime: None,
        data: text.into_bytes(),
    })
}

fn boundary(parts: &[Part]) -> String {
    let mut suffix = parts.iter().map(|part| part.data.len()).sum::<usize>();
    loop {
        let boundary = format!("gproxy-xai-{suffix:x}");
        if parts
            .iter()
            .all(|part| !contains(&part.data, boundary.as_bytes()))
        {
            return boundary;
        }
        suffix = suffix.saturating_add(1);
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|value| value == needle)
}

fn safe_name(name: &str) -> Result<(), ChannelError> {
    (!name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'[' | b']')))
    .then_some(())
    .ok_or_else(|| ChannelError::Prepare("multipart field name invalid".into()))
}

fn extension(mime: &str) -> &str {
    match mime {
        "audio/mpeg" => "mp3",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        "audio/mp4" => "m4a",
        _ => "wav",
    }
}
