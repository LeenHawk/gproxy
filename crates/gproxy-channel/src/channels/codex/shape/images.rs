//! Preserve image parameters; adapt buffered multipart edits to backend JSON.
use crate::channel::ChannelError;
use base64::Engine;
use futures_util::{FutureExt, StreamExt};
use gproxy_protocol::{
    HttpBody,
    codec::{CodecLimits, MultipartDecoder},
    connection::Bytes,
    wire::openai::images::{CreateImageRequestBody, EditImageJsonBody},
};
use http::{HeaderMap, header};
use serde_json::{Value, json};

fn invalid(error: impl std::fmt::Display) -> ChannelError {
    ChannelError::InvalidConfig(format!("Codex image request: {error}"))
}

pub(in crate::channels::codex) fn create(body: &Bytes) -> Result<Bytes, ChannelError> {
    let input: CreateImageRequestBody = serde_json::from_slice(body).map_err(invalid)?;
    serde_json::to_vec(&input).map(Bytes::from).map_err(invalid)
}

pub(in crate::channels::codex) fn edit(
    headers: &HeaderMap,
    body: &Bytes,
) -> Result<Bytes, ChannelError> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    let input: EditImageJsonBody = if content_type
        .split(';')
        .next()
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("multipart/form-data"))
    {
        let boundary = attribute(content_type, "boundary")
            .ok_or_else(|| invalid("multipart boundary missing"))?;
        // The input is already entirely in memory. The shared codec and its part
        // streams cannot wait for I/O; use one poll instead of blocking a runtime.
        parse_multipart(body.clone(), boundary)
            .now_or_never()
            .ok_or_else(|| invalid("buffered multipart decoder unexpectedly pending"))??
    } else {
        serde_json::from_slice(body).map_err(invalid)?
    };
    serde_json::to_vec(&input).map(Bytes::from).map_err(invalid)
}

async fn parse_multipart(body: Bytes, boundary: String) -> Result<EditImageJsonBody, ChannelError> {
    let size = body.len() as u64;
    let mut decoder = MultipartDecoder::new(
        HttpBody::Bytes(body),
        boundary,
        CodecLimits {
            max_buffer_bytes: 64 * 1024,
            max_line_bytes: 64 * 1024,
            max_body_bytes: size,
            max_part_bytes: size,
            max_parts: usize::MAX,
            max_value_bytes: size,
        },
    )
    .map_err(invalid)?;
    let mut object = serde_json::Map::new();
    let mut images = Vec::new();
    while let Some(part) = decoder.next_part().await.map_err(invalid)? {
        let disposition = part
            .headers
            .get(header::CONTENT_DISPOSITION)
            .and_then(|h| h.to_str().ok())
            .ok_or_else(|| invalid("multipart content disposition missing"))?;
        let name = attribute(disposition, "name")
            .ok_or_else(|| invalid("multipart field name missing"))?;
        let name = name.strip_suffix("[]").unwrap_or(&name);
        let file = attribute(disposition, "filename").is_some();
        let data = match part.body {
            HttpBody::Bytes(bytes) => bytes.to_vec(),
            HttpBody::Stream(mut stream) => {
                let mut data = Vec::new();
                while let Some(chunk) = stream.next().await {
                    data.extend_from_slice(&chunk.map_err(invalid)?);
                }
                data
            }
        };
        let value = if file {
            let mime = part
                .headers
                .get(header::CONTENT_TYPE)
                .and_then(|h| h.to_str().ok())
                .unwrap_or("application/octet-stream");
            Value::String(format!(
                "data:{mime};base64,{}",
                base64::engine::general_purpose::STANDARD.encode(data)
            ))
        } else {
            let text = String::from_utf8(data).map_err(invalid)?;
            match name {
                "n" | "output_compression" | "partial_images" => {
                    Value::from(text.parse::<i64>().map_err(invalid)?)
                }
                "stream" => Value::from(text.parse::<bool>().map_err(invalid)?),
                _ => Value::String(text),
            }
        };
        match name {
            "image" | "images" => images.push(json!({"image_url":value})),
            "mask" => {
                object.insert(name.into(), json!({"image_url":value}));
            }
            _ => {
                object.insert(name.into(), value);
            }
        }
    }
    object.insert("images".into(), Value::Array(images));
    serde_json::from_value(Value::Object(object)).map_err(invalid)
}

fn attribute(value: &str, name: &str) -> Option<String> {
    value
        .split(';')
        .skip(1)
        .find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            key.eq_ignore_ascii_case(name)
                .then(|| value.trim().trim_matches('"').to_owned())
        })
        .filter(|value| !value.is_empty())
}
