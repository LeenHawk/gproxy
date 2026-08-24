use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use serde_json::Value;

pub(super) fn request(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    let body = crate::shared::openai::shape_request(
        ctx.key,
        ctx.stream,
        ctx.upstream_model,
        ctx.headers,
        ctx.body,
    )?;
    super::with_object(body, |object| {
        if let Some(seconds) = object.remove("seconds") {
            let duration = seconds
                .as_str()
                .and_then(|value| value.parse::<u64>().ok())
                .map(Value::from)
                .unwrap_or(seconds);
            object.entry("duration").or_insert(duration);
        }
        if let Some(reference) = object.remove("input_reference") {
            object
                .entry("input_references")
                .or_insert(Value::Array(vec![super::image::reference(reference)?]));
        }
        Ok(())
    })
}

pub(super) fn response(body: &Bytes) -> Result<Bytes, ChannelError> {
    let mut value = serde_json::from_slice::<Value>(body)
        .map_err(|error| ChannelError::Observe(error.to_string()))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| ChannelError::Observe("video response is not an object".into()))?;
    if object.get("status").and_then(Value::as_str) == Some("pending") {
        object.insert("status".into(), Value::String("queued".into()));
    }
    if object.get("url").is_none()
        && let Some(url) = object
            .get("unsigned_urls")
            .and_then(Value::as_array)
            .and_then(|values| values.first())
            .and_then(Value::as_str)
            .map(str::to_owned)
    {
        object.insert("url".into(), Value::String(url));
    }
    super::encode(value).map_err(|error| ChannelError::Observe(error.to_string()))
}
