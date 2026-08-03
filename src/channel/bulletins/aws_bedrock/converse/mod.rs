mod content;
mod request;
mod response;

use bytes::Bytes;

pub(super) fn request(body: Bytes) -> Bytes {
    request::convert(body)
}

pub(super) fn response(body: Bytes) -> Bytes {
    response::convert(body)
}

pub(super) fn request_value(value: serde_json::Value) -> serde_json::Value {
    request::convert_value(value)
}

pub(super) fn usage(value: serde_json::Value) -> serde_json::Value {
    response::usage(value)
}

pub(super) fn stop_reason(value: Option<serde_json::Value>) -> serde_json::Value {
    response::stop_reason(value)
}

pub(super) fn push_sse(out: &mut Vec<u8>, event: &str, value: serde_json::Value) {
    out.extend_from_slice(format!("event: {event}\ndata: {value}\n\n").as_bytes());
}

pub(super) fn stream_index(value: &serde_json::Value) -> u64 {
    value
        .get("contentBlockIndex")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

pub(super) fn cache_point(control: serde_json::Value) -> serde_json::Value {
    let ttl = control
        .get("ttl")
        .and_then(serde_json::Value::as_str)
        .filter(|ttl| matches!(*ttl, "5m" | "1h"));
    match ttl {
        Some(ttl) => serde_json::json!({ "cachePoint": { "type": "default", "ttl": ttl } }),
        None => serde_json::json!({ "cachePoint": { "type": "default" } }),
    }
}
