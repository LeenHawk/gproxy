use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::Operation;
use http::{HeaderMap, HeaderValue};
use serde_json::{Value, json};

pub(super) fn request(
    ctx: &PrepareCtx<'_>,
    headers: &mut HeaderMap,
) -> Result<Bytes, ChannelError> {
    let mut object = super::super::multipart::object(ctx.headers, ctx.body)?;
    crate::shared::image_multipart::json_fields(
        &mut object,
        &["n", "output_compression", "partial_images", "stream"],
    );
    if !ctx.upstream_model.is_empty() {
        object.insert("model".into(), Value::String(ctx.upstream_model.into()));
    }
    object.remove("response_format");
    if ctx.key.operation == Operation::EditImage {
        object.remove("mask");
        let images = object
            .remove("images")
            .or_else(|| object.remove("image"))
            .map(values)
            .unwrap_or_default();
        let references = images
            .into_iter()
            .map(reference)
            .collect::<Result<Vec<_>, _>>()?;
        if !references.is_empty() {
            object.insert("input_references".into(), Value::Array(references));
        }
    }
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    super::encode(Value::Object(object))
}

pub(super) fn reference(value: Value) -> Result<Value, ChannelError> {
    if let Some(url) = value.as_str() {
        return Ok(json!({"type":"image_url", "image_url":{"url":url}}));
    }
    let object = value
        .as_object()
        .ok_or_else(|| ChannelError::Prepare("image reference must be a URL".into()))?;
    if object.get("type").and_then(Value::as_str) == Some("image_url") {
        return Ok(value);
    }
    let url = object
        .get("url")
        .or_else(|| object.get("image_url"))
        .and_then(|value| value.as_str().or_else(|| value.get("url")?.as_str()))
        .ok_or_else(|| ChannelError::Prepare("OpenRouter image reference requires a URL".into()))?;
    Ok(json!({"type":"image_url", "image_url":{"url":url}}))
}

fn values(value: Value) -> Vec<Value> {
    match value {
        Value::Array(values) => values,
        value => vec![value],
    }
}
