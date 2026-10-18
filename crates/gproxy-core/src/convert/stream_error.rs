//! Preserve upstream errors through failed settlement before writing client frames.
use gproxy_protocol::{
    Dialect,
    adapt::generate::stream::reader::SourceFraming,
    connection::{Bytes, TransportError},
    transform::{TransformError, UpstreamError},
    wire::{
        claude, gemini,
        openai::{chat, responses},
    },
};
use serde_json::{Value, json};
use std::error::Error;

#[derive(Debug, thiserror::Error)]
#[error("{error}")]
struct StreamError {
    error: TransformError,
    bytes: Bytes,
}

pub(super) fn converted(
    error: TransformError,
    dialect: Dialect,
    framing: SourceFraming,
    sequence: i64,
) -> TransportError {
    let Some(detail) = error.source().and_then(upstream_detail) else {
        return super::generate::transport(error);
    };
    let code = &detail["code"];
    let kind = detail["type"].as_str().unwrap_or_default();
    let status = detail["status"]
        .as_u64()
        .or_else(|| code.as_u64())
        .unwrap_or_else(|| match code.as_str().unwrap_or(kind) {
            "rate_limit_error" | "rate_limit_exceeded" | "insufficient_quota" => 429,
            "authentication_error" | "invalid_api_key" => 401,
            "permission_error" | "permission_denied" => 403,
            "not_found_error" | "model_not_found" => 404,
            "invalid_request_error" | "invalid_prompt" => 400,
            "overloaded_error" => 503,
            "timeout_error" => 504,
            _ => 500,
        });
    let kind = match status {
        429 => "rate_limit_error",
        401 => "authentication_error",
        403 => "permission_error",
        404 => "not_found_error",
        503 => "overloaded_error",
        504 => "timeout_error",
        400..=499 => "invalid_request_error",
        _ => "api_error",
    };
    let message = &detail["message"];
    let value = match dialect {
        Dialect::OpenAiChat => json!({"error": {
            "type": kind, "code": code, "status": detail["status"],
            "message": message, "param": detail["param"]
        }}),
        Dialect::OpenAi => json!({
            "type": "error", "code": detail["status"].as_str().or(code.as_str()).unwrap_or("upstream_error"),
            "message": message, "param": detail["param"],
            "sequence_number": sequence, "status": status
        }),
        Dialect::Claude => json!({
            "type": "error", "error": {"type": kind, "message": message},
            "status": status
        }),
        Dialect::Gemini => json!({"error": {
            "code": status, "message": message,
            "status": match status {
                429 => "RESOURCE_EXHAUSTED", 401 => "UNAUTHENTICATED", 403 => "PERMISSION_DENIED",
                404 => "NOT_FOUND", 400 => "INVALID_ARGUMENT", 503 => "UNAVAILABLE",
                504 => "DEADLINE_EXCEEDED", _ => "INTERNAL",
            }
        }}),
        _ => return super::generate::transport(error),
    };
    let bytes = match framing {
        SourceFraming::Sse if dialect == Dialect::OpenAiChat => {
            Bytes::from(format!("data: {value}\n\ndata: [DONE]\n\n"))
        }
        SourceFraming::Sse => Bytes::from(format!("event: error\ndata: {value}\n\n")),
        SourceFraming::JsonArray => {
            let prefix = if sequence == 0 { "[" } else { "," };
            Bytes::from(format!("{prefix}{value}]"))
        }
        SourceFraming::Ndjson => Bytes::from(format!("{value}\n")),
    };
    tracing::warn!(%error, "upstream generation stream failed");
    Box::new(StreamError { error, bytes })
}

fn upstream_detail(source: &(dyn Error + 'static)) -> Option<Value> {
    if let Some(error) = source.downcast_ref::<UpstreamError<responses::websocket::ErrorMessage>>()
    {
        let mut detail = serde_json::to_value(&error.0.error).ok()?;
        detail["status"] = json!(error.0.status);
        return Some(detail);
    }
    macro_rules! payload {
        ($($ty:ty),* $(,)?) => {$(
            if let Some(error) = source.downcast_ref::<UpstreamError<$ty>>() {
                return serde_json::to_value(&error.0).ok();
            }
        )*};
    }
    payload!(
        gemini::GenerateContentError,
        claude::stream::StreamError,
        chat::stream::StreamError,
        responses::stream::ResponseErrorEvent,
        responses::ResponseError,
    );
    None
}

pub(crate) fn into_event(error: TransportError) -> Result<Bytes, TransportError> {
    error.downcast::<StreamError>().map(|error| error.bytes)
}

pub(crate) fn is_error_frame(frame: &gproxy_protocol::codec::SseFrame) -> bool {
    let gproxy_protocol::codec::SseFrame::Event(event) = frame else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(&event.data) else {
        return false;
    };
    match value["type"].as_str() {
        Some("error" | "response.failed") => true,
        None => value.get("error").is_some_and(Value::is_object),
        // Tool failures and metadata can carry errors without failing the turn.
        Some(_) => false,
    }
}
