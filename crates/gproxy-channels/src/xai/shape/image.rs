use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::Operation;
use http::{HeaderMap, HeaderValue};
use serde_json::{Value, json};

pub(super) fn request(
    ctx: &PrepareCtx<'_>,
    headers: &mut HeaderMap,
) -> Result<Bytes, ChannelError> {
    if ctx.stream {
        return Err(ChannelError::Prepare(
            "xAI image API does not stream".into(),
        ));
    }
    let mut object = crate::shared::image_multipart::object(ctx.headers, ctx.body)?;
    crate::shared::image_multipart::json_fields(
        &mut object,
        &["n", "output_compression", "partial_images", "stream"],
    );
    if !ctx.upstream_model.is_empty() {
        object.insert("model".into(), Value::String(ctx.upstream_model.into()));
    }
    for name in ["moderation", "partial_images", "size", "stream"] {
        object.remove(name);
    }
    if ctx.key.operation == Operation::EditImage {
        object.remove("mask");
        let images = object
            .remove("images")
            .or_else(|| object.remove("image"))
            .map(values)
            .unwrap_or_default();
        let images = images
            .into_iter()
            .map(source)
            .collect::<Result<Vec<_>, _>>()?;
        match images.as_slice() {
            [] => {}
            [image] => {
                object.insert("image".into(), image.clone());
            }
            _ => {
                object.insert("images".into(), Value::Array(images));
            }
        }
    }
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    super::encode(Value::Object(object))
}

fn source(value: Value) -> Result<Value, ChannelError> {
    if let Some(url) = value.as_str() {
        return Ok(json!({"url":url}));
    }
    let mut object = value
        .as_object()
        .cloned()
        .ok_or_else(|| ChannelError::Prepare("xAI image source must be an object".into()))?;
    if object.get("url").is_none()
        && let Some(url) = object.remove("image_url")
    {
        let url = url.get("url").cloned().unwrap_or(url);
        object.insert("url".into(), url);
    }
    if object.get("url").is_none() && object.get("file_id").is_none() {
        return Err(ChannelError::Prepare(
            "xAI image source missing url or file_id".into(),
        ));
    }
    Ok(Value::Object(object))
}

fn values(value: Value) -> Vec<Value> {
    match value {
        Value::Array(values) => values,
        value => vec![value],
    }
}
