use base64::Engine as _;
use bytes::Bytes;
use http::{HeaderMap, HeaderValue, header};
use serde_json::{Map, Value};

use crate::channel::ChannelError;
use crate::protocol::Operation;

/// Restore the public multipart wire shape after ingress normalized an OpenAI
/// audio, image, or video upload to JSON/data URLs for routing and transforms.
pub fn restore_media_multipart(
    operation: Operation,
    headers: &mut HeaderMap,
    body: Bytes,
) -> Result<Bytes, ChannelError> {
    if !matches!(
        operation,
        Operation::CreateTranscription
            | Operation::CreateTranslation
            | Operation::EditImage
            | Operation::CreateVideo
            | Operation::CreateVideoCharacter
            | Operation::EditVideo
            | Operation::ExtendVideo
    ) {
        return Ok(body);
    }

    let fields: Map<String, Value> = serde_json::from_slice(&body)
        .map_err(|error| ChannelError::Build(format!("media upload body is not JSON: {error}")))?;
    let digest = blake3::hash(&body).to_hex();
    let boundary = format!("gproxy-media-{}", &digest.as_str()[..24]);
    let mut output = Vec::with_capacity(body.len());
    for (name, value) in fields {
        append_value(&mut output, &boundary, operation, &name, value)?;
    }
    output.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}"))
            .map_err(|error| ChannelError::Build(error.to_string()))?,
    );
    headers.remove(header::CONTENT_LENGTH);
    Ok(Bytes::from(output))
}

fn append_value(
    output: &mut Vec<u8>,
    boundary: &str,
    operation: Operation,
    name: &str,
    value: Value,
) -> Result<(), ChannelError> {
    if let Value::Array(values) = value {
        let array_name = format!("{name}[]");
        for value in values {
            append_value(output, boundary, operation, &array_name, value)?;
        }
        return Ok(());
    }
    if is_file_value(operation, name, &value) {
        let data_url = value
            .as_str()
            .ok_or_else(|| ChannelError::Build("media file must be a data URL".into()))?;
        let (mime, data) = decode_data_url(data_url)?;
        append_header(output, boundary, name, Some(filename(mime)), Some(mime))?;
        output.extend_from_slice(&data);
    } else {
        let text = match value {
            Value::String(value) => value,
            Value::Bool(value) => value.to_string(),
            Value::Number(value) => value.to_string(),
            other => serde_json::to_string(&other)
                .map_err(|error| ChannelError::Build(error.to_string()))?,
        };
        append_header(output, boundary, name, None, None)?;
        output.extend_from_slice(text.as_bytes());
    }
    output.extend_from_slice(b"\r\n");
    Ok(())
}

fn is_file_value(operation: Operation, name: &str, value: &Value) -> bool {
    let name = name.strip_suffix("[]").unwrap_or(name);
    let upload_field = match operation {
        Operation::CreateTranscription | Operation::CreateTranslation => name == "file",
        Operation::EditImage => matches!(name, "image" | "mask"),
        Operation::CreateVideo => name == "input_reference",
        Operation::CreateVideoCharacter | Operation::EditVideo | Operation::ExtendVideo => {
            name == "video"
        }
        _ => false,
    };
    upload_field
        && value
            .as_str()
            .is_some_and(|value| value.starts_with("data:"))
}

fn append_header(
    output: &mut Vec<u8>,
    boundary: &str,
    name: &str,
    filename: Option<&str>,
    content_type: Option<&str>,
) -> Result<(), ChannelError> {
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'[' | b']'))
    {
        return Err(ChannelError::Build("invalid multipart field name".into()));
    }
    output.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    output.extend_from_slice(format!("Content-Disposition: form-data; name=\"{name}\"").as_bytes());
    if let Some(filename) = filename {
        output.extend_from_slice(format!("; filename=\"{filename}\"").as_bytes());
    }
    output.extend_from_slice(b"\r\n");
    if let Some(content_type) = content_type {
        output.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
    }
    output.extend_from_slice(b"\r\n");
    Ok(())
}

fn decode_data_url(value: &str) -> Result<(&str, Vec<u8>), ChannelError> {
    let (metadata, payload) = value
        .strip_prefix("data:")
        .and_then(|value| value.split_once(','))
        .ok_or_else(|| ChannelError::Build("media file must be a data URL".into()))?;
    let mime = metadata
        .strip_suffix(";base64")
        .ok_or_else(|| ChannelError::Build("media file data URL must be base64".into()))?;
    if mime.is_empty()
        || !mime
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'+' | b'-' | b'.'))
    {
        return Err(ChannelError::Build("invalid media file MIME type".into()));
    }
    let data = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|error| ChannelError::Build(format!("invalid media file base64: {error}")))?;
    Ok((mime, data))
}

fn filename(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "image.jpg",
        "image/png" => "image.png",
        "image/webp" => "image.webp",
        "video/mp4" => "video.mp4",
        "video/mpeg" => "video.mpeg",
        "video/quicktime" => "video.mov",
        "video/webm" => "video.webm",
        "audio/flac" => "audio.flac",
        "audio/mpeg" | "audio/mp3" => "audio.mp3",
        "audio/mp4" => "audio.mp4",
        "audio/ogg" => "audio.ogg",
        "audio/webm" => "audio.webm",
        "audio/x-m4a" | "audio/m4a" => "audio.m4a",
        _ => "audio.wav",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_binary_file_and_repeated_fields() {
        let mut headers = HeaderMap::new();
        let body = Bytes::from_static(
            br#"{"file":"data:audio/wav;base64,UklGRg==","model":"whisper-1","timestamp_granularities":["word","segment"],"stream":"true"}"#,
        );
        let output =
            restore_media_multipart(Operation::CreateTranscription, &mut headers, body).unwrap();
        let text = String::from_utf8(output.to_vec()).unwrap();
        assert!(
            headers[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .starts_with("multipart/form-data; boundary=")
        );
        assert!(text.contains("name=\"file\"; filename=\"audio.wav\""));
        assert!(text.contains("Content-Type: audio/wav\r\n\r\nRIFF"));
        assert_eq!(
            text.matches("name=\"timestamp_granularities[]\"").count(),
            2
        );
        assert!(text.contains("name=\"stream\"\r\n\r\ntrue"));
    }

    #[test]
    fn restores_video_file_but_keeps_existing_video_reference_as_json() {
        let mut headers = HeaderMap::new();
        let body = Bytes::from_static(
            br#"{"prompt":"extend","seconds":"8","video":"data:video/mp4;base64,AAAA"}"#,
        );
        let output = restore_media_multipart(Operation::ExtendVideo, &mut headers, body).unwrap();
        let text = String::from_utf8(output.to_vec()).unwrap();
        assert!(text.contains("name=\"video\"; filename=\"video.mp4\""));
        assert!(text.contains("Content-Type: video/mp4\r\n\r\n\0\0\0"));

        let reference =
            Bytes::from_static(br#"{"prompt":"extend","seconds":"8","video":{"id":"video_123"}}"#);
        let output =
            restore_media_multipart(Operation::ExtendVideo, &mut headers, reference).unwrap();
        let text = String::from_utf8(output.to_vec()).unwrap();
        assert!(text.contains("name=\"video\"\r\n\r\n{\"id\":\"video_123\"}"));
        assert!(!text.contains("filename=\"video.mp4\""));
    }

    #[test]
    fn restores_image_edit_file_arrays() {
        let mut headers = HeaderMap::new();
        let body = Bytes::from_static(
            br#"{"image":["data:image/png;base64,AAAA","data:image/png;base64,AQID"],"prompt":"edit"}"#,
        );
        let output = restore_media_multipart(Operation::EditImage, &mut headers, body).unwrap();
        let text = String::from_utf8(output.to_vec()).unwrap();
        assert_eq!(
            text.matches("name=\"image[]\"; filename=\"image.png\"")
                .count(),
            2
        );
    }
}
