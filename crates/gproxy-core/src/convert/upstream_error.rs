//! Shared error details and classification for HTTP, SSE and Responses sockets.
use gproxy_protocol::{
    Dialect,
    transform::{TransformError, UpstreamError},
    wire::{
        claude, gemini,
        openai::{chat, responses},
    },
};
use serde_json::{Value, json};
use std::error::Error;

pub(crate) fn detail(error: &TransformError) -> Option<Value> {
    let source = error.source()?;
    macro_rules! payload {
        ($($ty:ty),* $(,)?) => {$(
            if let Some(error) = source.downcast_ref::<UpstreamError<$ty>>() {
                return serde_json::to_value(&error.0).ok().map(unwrap);
            }
        )*};
    }
    payload!(
        gemini::GenerateContentError,
        claude::stream::StreamError,
        chat::stream::StreamError,
        responses::stream::ResponseStreamError,
        responses::stream::ResponseErrorEvent,
        responses::ResponseError,
        responses::websocket::ErrorMessage,
    );
    None
}

pub(crate) fn unwrap(mut value: Value) -> Value {
    if value.get("error").is_some_and(Value::is_object) {
        let mut detail = value["error"].take();
        if value["status"].is_number() {
            detail["status"] = value["status"].take();
        }
        return detail;
    }
    if let Some(object) = value.as_object_mut() {
        if object.get("type").and_then(Value::as_str) == Some("error") {
            object.remove("type");
        }
        object.remove("sequence_number");
        object.remove("stream_id");
    }
    value
}

pub(crate) fn status(detail: &Value) -> u16 {
    fn numeric(value: &Value) -> Option<u16> {
        value
            .as_u64()
            .filter(|v| (400..600).contains(v))
            .map(|v| v as u16)
    }
    numeric(&detail["status"])
        .or_else(|| numeric(&detail["code"]))
        .or_else(|| detail["code"].as_str().and_then(classify))
        .or_else(|| detail["status"].as_str().and_then(classify))
        .or_else(|| detail["type"].as_str().and_then(classify))
        .unwrap_or(500)
}

fn classify(code: &str) -> Option<u16> {
    Some(match code {
        "rate_limit_error"
        | "rate_limit_exceeded"
        | "insufficient_quota"
        | "RESOURCE_EXHAUSTED" => 429,
        "authentication_error" | "invalid_api_key" | "UNAUTHENTICATED" => 401,
        "billing_error" => 402,
        "permission_error" | "permission_denied" | "PERMISSION_DENIED" => 403,
        "not_found_error" | "model_not_found" | "NOT_FOUND" => 404,
        "conflict_error" | "ALREADY_EXISTS" | "ABORTED" => 409,
        "request_too_large" => 413,
        "invalid_request_error"
        | "invalid_prompt"
        | "context_length_exceeded"
        | "data_residency_mismatch"
        | "bio_policy"
        | "invalid_image"
        | "invalid_image_format"
        | "invalid_base64_image"
        | "invalid_image_url"
        | "image_too_large"
        | "image_too_small"
        | "image_parse_error"
        | "image_content_policy_violation"
        | "invalid_image_mode"
        | "image_file_too_large"
        | "unsupported_image_media_type"
        | "empty_image_file"
        | "failed_to_download_image"
        | "image_file_not_found"
        | "INVALID_ARGUMENT"
        | "FAILED_PRECONDITION"
        | "OUT_OF_RANGE" => 400,
        "overloaded_error" | "UNAVAILABLE" => 503,
        "timeout_error" | "vector_store_timeout" | "DEADLINE_EXCEEDED" => 504,
        "api_error" | "server_error" | "INTERNAL" | "UNKNOWN" | "DATA_LOSS" => 500,
        _ => return None,
    })
}

pub(crate) fn kind(status: u16) -> &'static str {
    match status {
        401 => "authentication_error",
        402 => "billing_error",
        403 => "permission_error",
        404 => "not_found_error",
        409 => "conflict_error",
        413 => "request_too_large",
        429 => "rate_limit_error",
        503 => "overloaded_error",
        504 => "timeout_error",
        400..=499 => "invalid_request_error",
        _ => "api_error",
    }
}

fn code(detail: &Value) -> Value {
    if detail["code"].is_string() {
        return detail["code"].clone();
    }
    if detail["status"].is_string() {
        return detail["status"].clone();
    }
    if detail["code"].is_number() {
        return detail["code"].to_string().into();
    }
    detail["type"].as_str().unwrap_or("upstream_error").into()
}

pub(crate) fn body(detail: &Value, status: u16, dialect: Dialect) -> Option<Value> {
    Some(match dialect {
        Dialect::OpenAi | Dialect::OpenAiChat => {
            let mut error = detail.clone();
            error["type"] = detail["type"].as_str().unwrap_or(kind(status)).into();
            error["code"] = if dialect == Dialect::OpenAiChat && !detail["code"].is_null() {
                detail["code"].clone()
            } else {
                code(detail)
            };
            error
                .as_object_mut()
                .expect("upstream error object")
                .remove("status");
            error["param"] = detail["param"].clone();
            json!({"error": error})
        }
        Dialect::Claude => json!({
            "type": "error", "error": {"type": kind(status), "message": detail["message"]}
        }),
        Dialect::Gemini => {
            let mut error = json!({
                "code": status, "message": detail["message"],
                "status": match status {
                    429 => "RESOURCE_EXHAUSTED", 401 => "UNAUTHENTICATED", 403 => "PERMISSION_DENIED",
                    404 => "NOT_FOUND", 409 => "ABORTED", 400..=499 => "INVALID_ARGUMENT", 503 => "UNAVAILABLE",
                    504 => "DEADLINE_EXCEEDED", _ => "INTERNAL",
                }
            });
            if let Some(details) = detail.get("details") {
                error["details"] = details.clone();
            }
            json!({"error": error})
        }
        _ => return None,
    })
}

pub(crate) fn event(detail: &Value, status: u16, dialect: Dialect, sequence: i64) -> Option<Value> {
    if dialect == Dialect::OpenAi {
        return Some(json!({
            "type": "error", "code": code(detail), "message": detail["message"],
            "param": detail["param"], "sequence_number": sequence
        }));
    }
    body(detail, status, dialect)
}
