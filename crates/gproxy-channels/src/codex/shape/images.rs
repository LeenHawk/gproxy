use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use gproxy_protocol::openai::common::OpenAiModelId;
use gproxy_protocol::openai::images::{CreateImageRequest, EditImageRequest};

pub(super) fn create(body: &Bytes, model: &str) -> Result<Bytes, ChannelError> {
    let input: CreateImageRequest = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Prepare(format!("image request JSON: {error}")))?;
    let output = CreateImageRequest {
        prompt: input.prompt,
        background: input.background,
        model: Some(OpenAiModelId::from(model)),
        moderation: None,
        n: input.n,
        output_compression: None,
        output_format: None,
        partial_images: None,
        quality: input.quality,
        response_format: None,
        size: input.size,
        stream: None,
        style: None,
        user: None,
        rest: input.rest,
    };
    serde_json::to_vec(&output)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

pub(super) fn edit(
    headers: &http::HeaderMap,
    body: &Bytes,
    model: &str,
) -> Result<Bytes, ChannelError> {
    let input = if multipart(headers) {
        parse_multipart(headers, body)?
    } else {
        serde_json::from_slice(body)
            .map_err(|error| ChannelError::Prepare(format!("image edit request JSON: {error}")))?
    };
    narrow_edit(input, model)
}

fn narrow_edit(input: EditImageRequest, model: &str) -> Result<Bytes, ChannelError> {
    let output = EditImageRequest {
        images: input.images,
        prompt: input.prompt,
        background: input.background,
        input_fidelity: None,
        mask: None,
        model: Some(OpenAiModelId::from(model)),
        moderation: None,
        n: input.n,
        output_compression: None,
        output_format: None,
        partial_images: None,
        quality: input.quality,
        size: input.size,
        stream: None,
        user: None,
        rest: input.rest,
    };
    serde_json::to_vec(&output)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

fn multipart(headers: &http::HeaderMap) -> bool {
    headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("multipart/form-data"))
        })
}

fn parse_multipart(
    headers: &http::HeaderMap,
    body: &Bytes,
) -> Result<EditImageRequest, ChannelError> {
    let content_type = headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ChannelError::Prepare("multipart content type missing".into()))?;
    let boundary = content_type
        .split(';')
        .find_map(|part| part.trim().strip_prefix("boundary="))
        .map(|value| value.trim_matches('"'))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelError::Prepare("multipart boundary missing".into()))?;
    let mut object = serde_json::Map::new();
    let mut images = Vec::new();
    for part in super::super::multipart::split(body, boundary) {
        let part = part.strip_prefix(b"\r\n").unwrap_or(part);
        let Some(header_end) = find(part, b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&part[..header_end]);
        let Some(name) = attribute(&head, "name") else {
            continue;
        };
        let name = name.strip_suffix("[]").unwrap_or(&name);
        let data = trim_crlf(&part[header_end + 4..]);
        let filename = attribute(&head, "filename");
        let value = if filename.is_some() {
            let mime = head
                .lines()
                .find_map(|line| line.strip_prefix("Content-Type:"))
                .map(str::trim)
                .unwrap_or("application/octet-stream");
            serde_json::Value::String(format!("data:{mime};base64,{}", base64(data)))
        } else {
            text_value(name, data)
        };
        match name {
            "image" | "images" => {
                images.push(serde_json::json!({"image_url":value}));
            }
            "mask" => {
                object.insert("mask".into(), serde_json::json!({"image_url":value}));
            }
            _ => insert(&mut object, name, value),
        }
    }
    object.insert("images".into(), serde_json::Value::Array(images));
    serde_json::from_value(serde_json::Value::Object(object))
        .map_err(|error| ChannelError::Prepare(format!("normalized image edit: {error}")))
}

fn text_value(name: &str, bytes: &[u8]) -> serde_json::Value {
    let value = String::from_utf8_lossy(bytes).into_owned();
    match name {
        "n" | "output_compression" | "partial_images" => value
            .parse::<u64>()
            .map(serde_json::Value::from)
            .unwrap_or_else(|_| serde_json::Value::String(value)),
        "stream" => value
            .parse::<bool>()
            .map(serde_json::Value::from)
            .unwrap_or_else(|_| serde_json::Value::String(value)),
        _ => serde_json::Value::String(value),
    }
}

fn insert(
    object: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
    value: serde_json::Value,
) {
    match object.remove(name) {
        None => {
            object.insert(name.into(), value);
        }
        Some(serde_json::Value::Array(mut values)) => {
            values.push(value);
            object.insert(name.into(), serde_json::Value::Array(values));
        }
        Some(first) => {
            object.insert(name.into(), serde_json::Value::Array(vec![first, value]));
        }
    }
}

fn trim_crlf(value: &[u8]) -> &[u8] {
    value.strip_suffix(b"\r\n").unwrap_or(value)
}

fn attribute(headers: &str, name: &str) -> Option<String> {
    let marker = format!("{name}=\"");
    Some(headers.split(&marker).nth(1)?.split('"').next()?.to_owned())
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let value = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        output.push(TABLE[((value >> 18) & 63) as usize] as char);
        output.push(TABLE[((value >> 12) & 63) as usize] as char);
        output.push(if chunk.len() > 1 {
            TABLE[((value >> 6) & 63) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            TABLE[(value & 63) as usize] as char
        } else {
            '='
        });
    }
    output
}
