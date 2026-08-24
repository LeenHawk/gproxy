use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::Operation;
use serde_json::{Value, json};

pub(super) fn request(ctx: &PrepareCtx<'_>) -> Result<Bytes, ChannelError> {
    let body = super::super::model::body(ctx)?;
    let mut object = super::json_object(&body, "video")?;
    if let Some(seconds) = object.remove("seconds") {
        let duration = seconds
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            .map(Value::from)
            .unwrap_or(seconds);
        object.entry("duration").or_insert(duration);
    }
    if ctx.key.operation == Operation::CreateVideo {
        if let Some(reference) = object.remove("input_reference") {
            object.entry("image").or_insert(source(reference)?);
        }
    } else if let Some(video) = object.get_mut("video")
        && let Some(url) = video.as_str().map(str::to_owned)
    {
        *video = json!({"url":url});
    }
    super::encode(Value::Object(object))
}

pub(super) fn response(body: &Bytes) -> Result<Bytes, ChannelError> {
    let mut value = serde_json::from_slice::<Value>(body)
        .map_err(|error| ChannelError::Observe(error.to_string()))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| ChannelError::Observe("video response is not an object".into()))?;
    if object.get("id").is_none()
        && let Some(id) = object.get("request_id").cloned()
    {
        object.insert("id".into(), id);
    }
    let status = match object.get("status").and_then(Value::as_str) {
        Some("done" | "succeeded" | "success") => Some("completed"),
        Some("pending" | "processing") => Some("in_progress"),
        None if object.get("request_id").is_some() => Some("queued"),
        _ => None,
    };
    if let Some(status) = status {
        object.insert("status".into(), Value::String(status.into()));
    }
    if object.get("url").is_none()
        && let Some(url) = object
            .get("video")
            .and_then(|video| video.get("url"))
            .cloned()
    {
        object.insert("url".into(), url);
    }
    super::encode(value).map_err(|error| ChannelError::Observe(error.to_string()))
}

fn source(value: Value) -> Result<Value, ChannelError> {
    if let Some(url) = value.as_str() {
        return Ok(json!({"url":url}));
    }
    let mut object = value
        .as_object()
        .cloned()
        .ok_or_else(|| ChannelError::Prepare("xAI video image source invalid".into()))?;
    if object.get("url").is_none()
        && let Some(url) = object.remove("image_url")
    {
        object.insert("url".into(), url.get("url").cloned().unwrap_or(url));
    }
    Ok(Value::Object(object))
}
