//! Anthropic's many-image limit: a Messages request with more than 20 image
//! blocks (on Bedrock and Vertex, document blocks count too) refuses any image
//! whose side exceeds 3000 px, including images resent from earlier turns and
//! images nested in `tool_result`. Such base64 images are scaled down to fit;
//! everything else is left as the client sent it.
use base64::{Engine, engine::general_purpose::STANDARD};
#[cfg(any(
    feature = "azure",
    feature = "cloudflare_ai_gateway",
    feature = "opencode"
))]
use gproxy_protocol::connection::Bytes;
use image::{
    ImageFormat, ImageReader,
    codecs::{jpeg::JpegEncoder, png::PngEncoder},
    imageops::FilterType,
};
use serde_json::Value;
use std::io::Cursor;

const MAX_BLOCKS: usize = 20;
const MAX_SIDE: u32 = 3000;

/// Whether any image was rescaled.
pub(crate) fn fit(body: &mut Value, count_documents: bool) -> bool {
    let Some(messages) = body.get_mut("messages") else {
        return false;
    };
    blocks(messages, count_documents) > MAX_BLOCKS && shrink(messages)
}

/// `fit` for a channel that otherwise forwards the body byte for byte.
#[cfg(any(
    feature = "azure",
    feature = "cloudflare_ai_gateway",
    feature = "opencode"
))]
pub(crate) fn fit_bytes(bytes: Bytes, count_documents: bool) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<Value>(&bytes) else {
        return bytes;
    };
    if fit(&mut value, count_documents) {
        Bytes::from(value.to_string())
    } else {
        bytes
    }
}

fn blocks(value: &Value, count_documents: bool) -> usize {
    match value {
        Value::Array(items) => items.iter().map(|v| blocks(v, count_documents)).sum(),
        Value::Object(object) => {
            let own = match object.get("type").and_then(Value::as_str) {
                Some("image") => 1,
                Some("document") if count_documents => 1,
                _ => 0,
            };
            own + object
                .values()
                .map(|v| blocks(v, count_documents))
                .sum::<usize>()
        }
        _ => 0,
    }
}

fn shrink(value: &mut Value) -> bool {
    match value {
        Value::Array(items) => items
            .iter_mut()
            .fold(false, |changed, v| shrink(v) | changed),
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("image")
                && let Some(Value::Object(source)) = object.get_mut("source")
                && source.get("type").and_then(Value::as_str) == Some("base64")
            {
                let media_type = source.get("media_type").and_then(Value::as_str);
                let data = source.get("data").and_then(Value::as_str);
                if let (Some(media_type), Some(data)) = (media_type, data)
                    && let Some((media_type, data)) = resize(media_type, data)
                {
                    source.insert("media_type".into(), Value::String(media_type.into()));
                    source.insert("data".into(), Value::String(data));
                    return true;
                }
                return false;
            }
            object
                .values_mut()
                .fold(false, |changed, v| shrink(v) | changed)
        }
        _ => false,
    }
}

/// `None` keeps the image untouched: within bounds, or not decodable here.
fn resize(media_type: &str, data: &str) -> Option<(&'static str, String)> {
    let format = ImageFormat::from_mime_type(media_type)?;
    let bytes = STANDARD.decode(data).ok()?;
    let (width, height) = ImageReader::with_format(Cursor::new(&bytes), format)
        .into_dimensions()
        .ok()?;
    if width <= MAX_SIDE && height <= MAX_SIDE {
        return None;
    }
    let image = image::load_from_memory_with_format(&bytes, format)
        .ok()?
        .resize(MAX_SIDE, MAX_SIDE, FilterType::Triangle);
    let mut output = Vec::new();
    // JPEG stays JPEG; PNG, WebP and GIF become PNG, which keeps transparency.
    let media_type = if format == ImageFormat::Jpeg {
        image
            .into_rgb8()
            .write_with_encoder(JpegEncoder::new_with_quality(&mut output, 90))
            .ok()?;
        "image/jpeg"
    } else {
        image
            .write_with_encoder(PngEncoder::new(&mut output))
            .ok()?;
        "image/png"
    };
    Some((media_type, STANDARD.encode(output)))
}
