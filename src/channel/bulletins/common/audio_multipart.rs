use base64::Engine as _;
use bytes::Bytes;
use http::{HeaderMap, HeaderValue, header};
use serde_json::{Map, Value};

use crate::channel::ChannelError;
use crate::protocol::Operation;

/// Restore the public multipart wire shape after ingress normalized an OpenAI
/// audio, image, or video upload to JSON/data URLs for routing and transforms.
pub fn restore_media_multipart(
    channel_id: &str,
    operation: Operation,
    headers: &mut HeaderMap,
    body: Bytes,
) -> Result<Bytes, ChannelError> {
    if !needs_multipart(channel_id, operation) {
        return Ok(body);
    }

    let mut fields: Map<String, Value> = serde_json::from_slice(&body)
        .map_err(|error| ChannelError::Build(format!("media upload body is not JSON: {error}")))?;
    let file_name = (operation == Operation::CreateFile)
        .then(|| fields.remove("__gproxy_file_name"))
        .flatten()
        .and_then(|value| value.as_str().map(str::to_owned));
    // xAI's streaming STT parser only observes option fields placed before the
    // file part, so emit that upload last for the Grok Build adapter.
    let trailing_file = (matches!(channel_id, "grokbuild" | "xai")
        && operation == Operation::CreateTranscription)
        .then(|| fields.remove("file"))
        .flatten();
    let digest = blake3::hash(&body).to_hex();
    let boundary = format!("gproxy-media-{}", &digest.as_str()[..24]);
    let mut output = Vec::with_capacity(body.len());
    for (name, value) in fields {
        let file_name = (operation == Operation::CreateFile && name == "file")
            .then_some(file_name.as_deref())
            .flatten();
        append_value(&mut output, &boundary, operation, &name, value, file_name)?;
    }
    if let Some(file) = trailing_file {
        append_value(&mut output, &boundary, operation, "file", file, None)?;
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

fn needs_multipart(channel_id: &str, operation: Operation) -> bool {
    match operation {
        // OpenAI-compatible upload surfaces use multipart, while the media
        // shaping layer can still translate their option fields per channel.
        Operation::CreateTranscription | Operation::CreateTranslation => {
            matches!(
                channel_id,
                "openai" | "custom" | "openrouter" | "grokbuild" | "xai"
            )
        }
        Operation::EditImage => matches!(channel_id, "openai" | "azure" | "custom"),
        // AI Studio's Sora-compatible create endpoint is multipart too, while
        // OpenRouter, xAI, Vertex and Bedrock use JSON for video requests.
        Operation::CreateVideo => {
            matches!(channel_id, "openai" | "azure" | "custom" | "aistudio")
        }
        Operation::CreateVideoCharacter | Operation::EditVideo | Operation::ExtendVideo => {
            matches!(channel_id, "openai" | "azure" | "custom")
        }
        Operation::CreateFile => channel_id == "openai",
        _ => false,
    }
}

fn append_value(
    output: &mut Vec<u8>,
    boundary: &str,
    operation: Operation,
    name: &str,
    value: Value,
    file_name: Option<&str>,
) -> Result<(), ChannelError> {
    if let Value::Array(values) = value {
        let array_name = format!("{name}[]");
        for value in values {
            append_value(output, boundary, operation, &array_name, value, file_name)?;
        }
        return Ok(());
    }
    if is_file_value(operation, name, &value) {
        let data_url = value
            .as_str()
            .ok_or_else(|| ChannelError::Build("media file must be a data URL".into()))?;
        let (mime, data) = decode_data_url(data_url)?;
        let file_name = file_name
            .filter(|name| safe_filename(name))
            .unwrap_or_else(|| filename(mime));
        append_header(output, boundary, name, Some(file_name), Some(mime))?;
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

fn safe_filename(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1024
        && !value
            .bytes()
            .any(|byte| matches!(byte, b'\r' | b'\n' | b'"' | b'\\'))
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
        Operation::CreateFile => name == "file",
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
            restore_media_multipart("openai", Operation::CreateTranscription, &mut headers, body)
                .unwrap();
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
        let output =
            restore_media_multipart("openai", Operation::ExtendVideo, &mut headers, body).unwrap();
        let text = String::from_utf8(output.to_vec()).unwrap();
        assert!(text.contains("name=\"video\"; filename=\"video.mp4\""));
        assert!(text.contains("Content-Type: video/mp4\r\n\r\n\0\0\0"));

        let reference =
            Bytes::from_static(br#"{"prompt":"extend","seconds":"8","video":{"id":"video_123"}}"#);
        let output =
            restore_media_multipart("openai", Operation::ExtendVideo, &mut headers, reference)
                .unwrap();
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
        let output =
            restore_media_multipart("openai", Operation::EditImage, &mut headers, body).unwrap();
        let text = String::from_utf8(output.to_vec()).unwrap();
        assert_eq!(
            text.matches("name=\"image[]\"; filename=\"image.png\"")
                .count(),
            2
        );
    }

    #[test]
    fn keeps_json_for_native_json_media_apis() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        let body = Bytes::from_static(br#"{"image":"data:image/png;base64,AAAA","prompt":"edit"}"#);
        let output =
            restore_media_multipart("xai", Operation::EditImage, &mut headers, body.clone())
                .unwrap();
        assert_eq!(output, body);
        assert_eq!(headers[header::CONTENT_TYPE], "application/json");

        let video = Bytes::from_static(br#"{"model":"grok-imagine-video","prompt":"cat"}"#);
        let output =
            restore_media_multipart("xai", Operation::CreateVideo, &mut headers, video.clone())
                .unwrap();
        assert_eq!(output, video);
    }

    #[test]
    fn grokbuild_places_stt_file_after_all_option_fields() {
        let mut headers = HeaderMap::new();
        let body = Bytes::from_static(
            br#"{"file":"data:audio/wav;base64,UklGRg==","language":"en","diarize":true}"#,
        );
        let output = restore_media_multipart(
            "grokbuild",
            Operation::CreateTranscription,
            &mut headers,
            body,
        )
        .unwrap();
        let text = String::from_utf8(output.to_vec()).unwrap();
        let language = text.find("name=\"language\"").unwrap();
        let diarize = text.find("name=\"diarize\"").unwrap();
        let file = text.find("name=\"file\"").unwrap();
        assert!(language < file);
        assert!(diarize < file);
        assert!(text.contains("name=\"file\"; filename=\"audio.wav\""));
    }
}
