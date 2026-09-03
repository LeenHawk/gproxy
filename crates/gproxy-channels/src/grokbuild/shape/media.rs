use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::Operation;
use http::{HeaderMap, HeaderValue};
use serde_json::{Map, Value, json};

pub(super) fn request(
    ctx: &PrepareCtx<'_>,
    headers: &mut HeaderMap,
) -> Result<Bytes, ChannelError> {
    if ctx.key.operation() == Operation::CreateTranscription {
        if ctx.stream {
            return Err(ChannelError::Prepare(
                "xAI transcription API does not use OpenAI SSE".into(),
            ));
        }
        let (body, content_type) = super::super::multipart::stt_request(ctx.headers, ctx.body)?;
        headers.insert(http::header::CONTENT_TYPE, content_type);
        return Ok(body);
    }
    if ctx.stream && ctx.key.operation() == Operation::CreateSpeech {
        return Err(ChannelError::Prepare(
            "xAI speech API does not use OpenAI SSE".into(),
        ));
    }
    if ctx.stream
        && matches!(
            ctx.key.operation(),
            Operation::CreateImage | Operation::EditImage
        )
    {
        return Err(ChannelError::Prepare(
            "xAI image API does not stream".into(),
        ));
    }
    let mut object = crate::shared::image_multipart::object(ctx.headers, ctx.body)?;
    crate::shared::image_multipart::json_fields(
        &mut object,
        &[
            "duration",
            "n",
            "output_compression",
            "partial_images",
            "seed",
            "seconds",
            "stream",
            "temperature",
        ],
    );
    if !ctx.upstream_model.is_empty() {
        object.insert("model".into(), Value::String(ctx.upstream_model.into()));
    }
    match ctx.key.operation() {
        Operation::CreateImage | Operation::EditImage => image(&mut object, ctx.key.operation())?,
        Operation::CreateSpeech => speech(&mut object)?,
        Operation::CreateVideo | Operation::EditVideo | Operation::ExtendVideo => {
            video(&mut object, ctx.key.operation())?
        }
        _ => {}
    }
    headers.insert(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    super::encode(Value::Object(object))
}

fn image(object: &mut Map<String, Value>, operation: Operation) -> Result<(), ChannelError> {
    for name in ["moderation", "partial_images", "size", "stream"] {
        object.remove(name);
    }
    if operation != Operation::EditImage {
        return Ok(());
    }
    object.remove("mask");
    let images = object
        .remove("images")
        .or_else(|| object.remove("image"))
        .map(values)
        .unwrap_or_default()
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
    Ok(())
}

fn speech(object: &mut Map<String, Value>) -> Result<(), ChannelError> {
    for name in ["model", "instructions", "stream_format"] {
        object.remove(name);
    }
    if let Some(input) = object.remove("input") {
        object.entry("text").or_insert(input);
    }
    if let Some(voice) = object.remove("voice") {
        object.entry("voice_id").or_insert(voice);
    }
    if let Some(format) = object.remove("response_format") {
        let output = object
            .entry("output_format")
            .or_insert_with(|| Value::Object(Map::new()));
        output
            .as_object_mut()
            .ok_or_else(|| ChannelError::Prepare("output_format must be an object".into()))?
            .insert("codec".into(), format);
    }
    Ok(())
}

fn video(object: &mut Map<String, Value>, operation: Operation) -> Result<(), ChannelError> {
    if let Some(seconds) = object.remove("seconds") {
        let duration = seconds
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            .map(Value::from)
            .unwrap_or(seconds);
        object.entry("duration").or_insert(duration);
    }
    if operation == Operation::CreateVideo {
        if let Some(image) = object.remove("input_reference") {
            object.entry("image").or_insert(source(image)?);
        }
    } else if let Some(video) = object.get_mut("video")
        && let Some(url) = video.as_str().map(str::to_owned)
    {
        *video = json!({"url":url});
    }
    Ok(())
}

fn source(value: Value) -> Result<Value, ChannelError> {
    if let Some(url) = value.as_str() {
        return Ok(json!({"url":url}));
    }
    let mut object = value
        .as_object()
        .cloned()
        .ok_or_else(|| ChannelError::Prepare("media source must be an object".into()))?;
    if object.get("url").is_none()
        && let Some(url) = object.remove("image_url")
    {
        object.insert("url".into(), url.get("url").cloned().unwrap_or(url));
    }
    Ok(Value::Object(object))
}

fn values(value: Value) -> Vec<Value> {
    match value {
        Value::Array(values) => values,
        value => vec![value],
    }
}

pub(super) fn video_response(body: &Bytes) -> Result<Bytes, ChannelError> {
    let mut value: Value =
        serde_json::from_slice(body).map_err(|error| ChannelError::Observe(error.to_string()))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| ChannelError::Observe("video response must be an object".into()))?;
    if object.get("id").is_none()
        && let Some(id) = object.get("request_id").cloned()
    {
        object.insert("id".into(), id);
    }
    let status = match object.get("status").and_then(Value::as_str) {
        Some("done" | "succeeded" | "success") => Some("completed"),
        Some("pending" | "processing") => Some("in_progress"),
        None if video_url(object).is_some() => Some("completed"),
        None if object.get("request_id").is_some() => Some("queued"),
        _ => None,
    };
    if let Some(status) = status {
        object.insert("status".into(), Value::String(status.into()));
    }
    if object.get("url").is_none()
        && let Some(url) = video_url(object).cloned()
    {
        object.insert("url".into(), url);
    }
    super::encode(value).map_err(|error| ChannelError::Observe(error.to_string()))
}

fn video_url(object: &Map<String, Value>) -> Option<&Value> {
    object.get("video")?.get("url")
}
