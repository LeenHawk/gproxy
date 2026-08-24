use base64::Engine as _;
use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use http::{HeaderMap, HeaderValue};
use serde_json::Value;

const DROP: &[&str] = &[
    "model",
    "prompt",
    "response_format",
    "stream",
    "temperature",
    "timestamp_granularities",
];

pub(in crate::grokbuild) fn request(
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<(Bytes, HeaderValue), ChannelError> {
    let mut fields = crate::shared::image_multipart::object(headers, body)?;
    for name in DROP {
        fields.remove(*name);
    }
    let file = fields
        .remove("file")
        .ok_or_else(|| ChannelError::Prepare("transcription file missing".into()))?;
    let (mime, file) = data_url(single(file)?)?;
    let boundary = boundary(&file, body.len());
    let mut output = Vec::new();
    for (name, value) in fields {
        safe_name(&name)?;
        append_header(&mut output, &boundary, &name, None, None);
        let text = match value {
            Value::String(value) => value,
            value => serde_json::to_string(&value)
                .map_err(|error| ChannelError::Prepare(error.to_string()))?,
        };
        output.extend_from_slice(text.as_bytes());
        output.extend_from_slice(b"\r\n");
    }
    append_header(
        &mut output,
        &boundary,
        "file",
        Some(&format!("audio.{}", extension(&mime))),
        Some(&mime),
    );
    output.extend_from_slice(&file);
    output.extend_from_slice(b"\r\n");
    output.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let content_type = HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}"))
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    Ok((Bytes::from(output), content_type))
}

fn single(value: Value) -> Result<String, ChannelError> {
    match value {
        Value::String(value) => Ok(value),
        Value::Array(mut values) if values.len() == 1 => values
            .pop()
            .and_then(|value| value.as_str().map(str::to_owned))
            .ok_or_else(|| ChannelError::Prepare("transcription file invalid".into())),
        _ => Err(ChannelError::Prepare("transcription file invalid".into())),
    }
}

fn data_url(value: String) -> Result<(String, Vec<u8>), ChannelError> {
    let (metadata, payload) = value
        .strip_prefix("data:")
        .and_then(|value| value.split_once(','))
        .ok_or_else(|| ChannelError::Prepare("transcription data URL malformed".into()))?;
    let mime = metadata
        .strip_suffix(";base64")
        .ok_or_else(|| ChannelError::Prepare("transcription file must be base64".into()))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|error| ChannelError::Prepare(format!("transcription base64: {error}")))?;
    Ok((mime.into(), bytes))
}

fn append_header(
    output: &mut Vec<u8>,
    boundary: &str,
    name: &str,
    filename: Option<&str>,
    mime: Option<&str>,
) {
    output.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    output.extend_from_slice(format!("Content-Disposition: form-data; name=\"{name}\"").as_bytes());
    if let Some(filename) = filename {
        output.extend_from_slice(format!("; filename=\"{filename}\"").as_bytes());
    }
    output.extend_from_slice(b"\r\n");
    if let Some(mime) = mime {
        output.extend_from_slice(format!("Content-Type: {mime}\r\n").as_bytes());
    }
    output.extend_from_slice(b"\r\n");
}

fn boundary(file: &[u8], seed: usize) -> String {
    let mut suffix = seed;
    loop {
        let value = format!("gproxy-grok-{suffix:x}");
        if !file
            .windows(value.len())
            .any(|bytes| bytes == value.as_bytes())
        {
            return value;
        }
        suffix = suffix.saturating_add(1);
    }
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
