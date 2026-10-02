//! H3 V2 task envelopes mapped to the OpenAI-family video extension.
//! No polling loop or task persistence: each retrieve performs one query.

use super::MiniMax;
use crate::channel::{BaseChannel, ChannelError, OperationContext, PrepareContext};
use crate::channels::shared::compatible::http::read_body;
use gproxy_protocol::{HttpBody, Operation, OperationKey, WireResponse};
use http::{HeaderMap, StatusCode, header};
use serde_json::{Map, Value, json};

fn invalid(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(message.into())
}

pub(super) fn task_id(path: &str) -> Result<&str, ChannelError> {
    let id = path
        .trim_end_matches('/')
        .strip_suffix("/content")
        .unwrap_or(path.trim_end_matches('/'))
        .rsplit('/')
        .next()
        .unwrap_or_default();
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ChannelError::InvalidConfig(
            "MiniMax video task ID must be numeric".into(),
        ));
    }
    Ok(id)
}

pub(super) fn request(body: &[u8]) -> Result<Vec<u8>, ChannelError> {
    let mut body: Map<String, Value> = serde_json::from_slice(body)
        .map_err(|e| ChannelError::InvalidConfig(format!("MiniMax video JSON: {e}")))?;
    // Native content remains available for all H3 reference modalities.
    let mut content = match body.remove("content") {
        Some(Value::Array(items)) => items,
        None => vec![],
        Some(_) => {
            return Err(ChannelError::InvalidConfig(
                "video content must be an array".into(),
            ));
        }
    };
    if let Some(prompt) = body.remove("prompt") {
        content.push(json!({"type": "text", "text": prompt}));
    }
    for (name, frames) in [("frame_images", true), ("input_references", false)] {
        if let Some(items) = body.remove(name) {
            let items = items
                .as_array()
                .ok_or_else(|| ChannelError::InvalidConfig(format!("{name} must be an array")))?;
            for item in items {
                let mut item = item.as_object().cloned().ok_or_else(|| {
                    ChannelError::InvalidConfig(format!("{name} entries must be objects"))
                })?;
                let role = if frames {
                    item.remove("frame_type").ok_or_else(|| {
                        ChannelError::InvalidConfig("frame_type is required".into())
                    })?
                } else {
                    Value::String(
                        match item.get("type").and_then(Value::as_str) {
                            Some("image_url") => "reference_image",
                            Some("video_url") => "reference_video",
                            Some("audio_url") => "reference_audio",
                            _ => {
                                return Err(ChannelError::InvalidConfig(
                                    "unknown video reference type".into(),
                                ));
                            }
                        }
                        .into(),
                    )
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
    // Native Sora multipart/size semantics cannot be silently approximated.
    for key in ["seconds", "size", "input_reference", "provider"] {
        if body.contains_key(key) {
            return Err(ChannelError::InvalidConfig(format!(
                "MiniMax H3 uses duration, resolution, frame_images and input_references; unsupported field: {key}"
            )));
        }
    }
    serde_json::to_vec(&body).map_err(|e| ChannelError::InvalidConfig(e.to_string()))
}

fn task(value: &Value) -> Result<Value, ChannelError> {
    let mut out = value
        .as_object()
        .cloned()
        .ok_or_else(|| invalid("MiniMax task must be an object"))?;
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("MiniMax task missing id"))?;
    let status = match value.get("status").and_then(Value::as_str) {
        Some("queued") => "pending",
        Some("running") => "in_progress",
        Some("succeeded") => "completed",
        Some("failed") => "failed",
        Some("cancelled") => "cancelled",
        Some("expired") => "expired",
        _ => return Err(invalid("unknown MiniMax task status")),
    };
    out.insert("status".into(), status.into());
    out.insert("polling_url".into(), format!("/v1/videos/{id}").into());
    if let Some(url) = value.pointer("/content/url").and_then(Value::as_str) {
        out.insert("unsigned_urls".into(), json!([url]));
    }
    // The public video extension represents its error as text. Keep the
    // vendor's structured details alongside it for callers that need them.
    if let Some(error) = out.remove("error")
        && !error.is_null()
    {
        out.insert(
            "error".into(),
            error
                .as_str()
                .map(str::to_owned)
                .or_else(|| {
                    error
                        .get("message")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| error.to_string())
                .into(),
        );
        out.insert("minimax_error".into(), error);
    }
    Ok(Value::Object(out))
}

fn response(operation: Operation, value: Value) -> Result<Value, ChannelError> {
    match operation {
        Operation::CreateVideo => {
            let id = value
                .get("task_id")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("MiniMax create reply missing task_id"))?;
            Ok(json!({"id": id, "status": "pending", "polling_url": format!("/v1/videos/{id}")}))
        }
        Operation::RetrieveVideo => task(
            value
                .get("task")
                .ok_or_else(|| invalid("MiniMax query reply missing task"))?,
        ),
        Operation::ListVideos => {
            let items = value
                .get("items")
                .and_then(Value::as_array)
                .ok_or_else(|| invalid("MiniMax list reply missing items"))?;
            let data = items.iter().map(task).collect::<Result<Vec<_>, _>>()?;
            Ok(json!({"object": "list", "data": data, "total": value.get("total")}))
        }
        // Cancellation of a queued task and deletion of a finished task
        // are distinct upstream outcomes, so preserve action and status.
        Operation::DeleteVideo => {
            let id = value
                .get("task_id")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("MiniMax delete reply missing task_id"))?;
            Ok(json!({"id": id, "object": "video.deleted",
                "deleted": value.get("action").and_then(Value::as_str) == Some("deleted"),
                "action": value.get("action"), "status": value.get("status")}))
        }
        _ => Err(invalid("unexpected video operation")),
    }
}

pub(super) async fn call(
    channel: &MiniMax,
    operation: Operation,
    ctx: OperationContext<'_>,
) -> Result<WireResponse<HttpBody>, ChannelError> {
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
    let value = serde_json::from_slice(&bytes)
        .map_err(|e| invalid(format!("MiniMax video response: {e}")))?;
    let out = response(operation, value)?;
    reply.headers.remove(header::CONTENT_LENGTH);
    reply.headers.insert(
        header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    reply.body = HttpBody::Bytes(out.to_string().into());
    Ok(reply)
}

pub(super) async fn download(
    channel: &MiniMax,
    mut ctx: OperationContext<'_>,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    // Query uses the assigned API credential. The CDN download must never
    // carry that credential or the caller's source authentication.
    ctx.request.method = http::Method::GET;
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
    let value: Value = serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
    let task = value
        .get("task")
        .ok_or_else(|| invalid("MiniMax query reply missing task"))?;
    if task.get("status").and_then(Value::as_str) != Some("succeeded") {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        return Ok(WireResponse { status: StatusCode::CONFLICT, headers,
            body: HttpBody::Bytes(json!({"error": {"message": "Video is not ready for download", "type": "invalid_request_error"}}).to_string().into()) });
    }
    let url = task
        .pointer("/content/url")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("MiniMax completed task missing content.url"))?;
    let uri: http::Uri = url
        .parse()
        .map_err(|_| invalid("invalid video download URL"))?;
    if !matches!(uri.scheme_str(), Some("http" | "https")) || uri.authority().is_none() {
        return Err(invalid("invalid video download URL"));
    }
    let request = http::Request::get(uri)
        .body(HttpBody::Bytes(Default::default()))
        .map_err(|e| invalid(e.to_string()))?;
    Ok(ctx.client.send(request).await?)
}
