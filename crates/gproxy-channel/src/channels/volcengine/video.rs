//! create/get/list-video-generation-task(s)-api and
//! cancel-or-delete-video-generation-tasks-api. Only the prefix changes for Plan.
//! Ark content blocks stay available alongside the gateway video extensions.

use super::{Volcengine, json_response};
use crate::channel::{BaseChannel, ChannelError, OperationContext, PrepareContext};
use crate::channels::shared::compatible::http::{invalid_response, read_body};
use gproxy_protocol::{HttpBody, Operation, OperationKey, WireResponse};
use http::{Method, StatusCode, header};
use serde_json::{Map, Value, json};

pub(super) fn task_id(path: &str) -> Result<&str, ChannelError> {
    let path = path.trim_end_matches('/');
    let id = path
        .strip_suffix("/content")
        .unwrap_or(path)
        .rsplit('/')
        .next()
        .unwrap_or_default();
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    {
        return Err(ChannelError::InvalidConfig(
            "Invalid Ark video task ID".into(),
        ));
    }
    Ok(id)
}

pub(super) fn request(bytes: &[u8]) -> Result<Vec<u8>, ChannelError> {
    let invalid = |message: &str| ChannelError::InvalidConfig(message.into());
    let mut body: Map<String, Value> =
        serde_json::from_slice(bytes).map_err(|e| invalid(&e.to_string()))?;
    let mut content = match body.remove("content") {
        Some(Value::Array(items)) => items,
        None => vec![],
        _ => return Err(invalid("Ark video content must be an array")),
    };
    if let Some(prompt) = body.remove("prompt") {
        content.push(json!({"type": "text", "text": prompt}));
    }
    for (name, frames) in [("frame_images", true), ("input_references", false)] {
        if let Some(items) = body.remove(name) {
            for item in items
                .as_array()
                .ok_or_else(|| invalid("Video references must be arrays"))?
            {
                let mut item = item
                    .as_object()
                    .cloned()
                    .ok_or_else(|| invalid("Video reference must be an object"))?;
                let role = if frames {
                    item.remove("frame_type")
                        .ok_or_else(|| invalid("Video frame_type is required"))?
                } else {
                    match item.get("type").and_then(Value::as_str) {
                        Some("image_url") => "reference_image",
                        Some("video_url") => "reference_video",
                        Some("audio_url") => "reference_audio",
                        _ => return Err(invalid("Unknown video reference type")),
                    }
                    .into()
                };
                item.insert("role".into(), role);
                content.push(Value::Object(item));
            }
        }
    }
    body.insert("content".into(), Value::Array(content));
    if let Some(ratio) = body.remove("aspect_ratio") {
        body.insert("ratio".into(), ratio);
    }
    if let Some(seconds) = body.remove("seconds") {
        let duration = seconds
            .as_i64()
            .or_else(|| seconds.as_str().and_then(|s| s.parse().ok()))
            .ok_or_else(|| invalid("Video seconds must be an integer"))?;
        body.entry("duration").or_insert(duration.into());
    }
    for key in ["size", "input_reference", "provider"] {
        if body.contains_key(key) {
            return Err(invalid(
                "Ark video uses resolution, aspect_ratio, frame_images and input_references; Sora size/input_reference/provider are unsupported",
            ));
        }
    }
    Ok(Value::Object(body).to_string().into_bytes())
}

fn task(mut value: Value) -> Result<Value, ChannelError> {
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_response("Ark video reply missing id"))?;
    let polling_url = format!("/v1/videos/{id}");
    let status = match value.get("status").and_then(Value::as_str) {
        Some("queued") => "pending",
        Some("running") => "in_progress",
        Some("succeeded") => "completed",
        Some("failed") => "failed",
        Some("cancelled") => "cancelled",
        Some("expired") => "expired",
        _ => return Err(invalid_response("Unknown Ark video status")),
    };
    value["status"] = status.into();
    value["polling_url"] = polling_url.into();
    if let Some(url) = value.pointer("/content/video_url").and_then(Value::as_str) {
        value["unsigned_urls"] = json!([url]);
    }
    if let Some(error) = value.get("error").filter(|e| !e.is_null()).cloned() {
        value["error"] = error
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| error.to_string())
            .into();
        value["volcengine_error"] = error;
    }
    Ok(value)
}

pub(super) async fn call(
    channel: &Volcengine,
    operation: Operation,
    ctx: OperationContext<'_>,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    let id = (operation == Operation::DeleteVideo)
        .then(|| task_id(&ctx.request.path).map(str::to_owned))
        .transpose()?;
    let request = channel.prepare(PrepareContext {
        provider: ctx.provider,
        credential: ctx.credential,
        operation: OperationKey {
            operation,
            dialect: ctx.dialect,
        },
        request: ctx.request,
        endpoint_override: ctx.endpoint_override,
    })?;
    let mut reply = ctx.client.send(request).await?;
    if !reply.status.is_success() {
        return Ok(reply);
    }
    let bytes = read_body(reply.body).await?;
    let value: Value = if bytes.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&bytes).map_err(|e| invalid_response(e.to_string()))?
    };
    let out = match operation {
        Operation::CreateVideo => {
            let id = value
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid_response("Ark create reply missing id"))?;
            json!({"id": id, "status": "pending", "polling_url": format!("/v1/videos/{id}")})
        }
        Operation::RetrieveVideo => task(value)?,
        Operation::ListVideos => {
            let data = value
                .get("items")
                .and_then(Value::as_array)
                .ok_or_else(|| invalid_response("Ark video list missing items"))?
                .iter()
                .cloned()
                .map(task)
                .collect::<Result<Vec<_>, _>>()?;
            json!({"object": "list", "data": data, "total": value.get("total")})
        }
        // A queued task is cancelled; a completed task is deleted. The API
        // returns no discriminator, so do not invent a `deleted: true` flag.
        Operation::DeleteVideo => json!({"id": id, "accepted": true, "action": "cancel_or_delete"}),
        _ => return Err(invalid_response("Unexpected Ark video operation")),
    };
    reply.headers.remove(header::CONTENT_LENGTH);
    reply.headers.insert(
        header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    reply.body = HttpBody::Bytes(out.to_string().into());
    Ok(reply)
}

pub(super) async fn download(
    channel: &Volcengine,
    mut ctx: OperationContext<'_>,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    ctx.request.method = Method::GET;
    ctx.request.query = None;
    let request = channel.prepare(PrepareContext {
        provider: ctx.provider,
        credential: ctx.credential,
        operation: OperationKey {
            operation: Operation::DownloadVideoContent,
            dialect: ctx.dialect,
        },
        request: ctx.request,
        endpoint_override: ctx.endpoint_override,
    })?;
    let reply = ctx.client.send(request).await?;
    if !reply.status.is_success() {
        return Ok(reply);
    }
    let bytes = read_body(reply.body).await?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|e| invalid_response(e.to_string()))?;
    if value["status"].as_str() != Some("succeeded") {
        return Ok(json_response(
            StatusCode::CONFLICT,
            json!({"error": {"type": "invalid_request_error", "message": "Video is not ready for download"}}),
        ));
    }
    let url = value
        .pointer("/content/video_url")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_response("Ark completed video missing content.video_url"))?;
    let uri: http::Uri = url
        .parse()
        .map_err(|_| invalid_response("Invalid video download URL"))?;
    if !matches!(uri.scheme_str(), Some("http" | "https")) || uri.authority().is_none() {
        return Err(invalid_response("Invalid video download URL"));
    }
    // Neither the source credential nor the Ark key goes to the media CDN.
    let request = http::Request::get(uri)
        .body(HttpBody::Bytes(Default::default()))
        .map_err(|e| invalid_response(e.to_string()))?;
    Ok(ctx.client.send(request).await?)
}
