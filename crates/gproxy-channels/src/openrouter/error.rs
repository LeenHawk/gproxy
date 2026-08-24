use bytes::Bytes;
use serde_json::Value;

pub(super) fn shape(body: &Bytes) -> Bytes {
    let Ok(mut value) = serde_json::from_slice::<Value>(body) else {
        return body.clone();
    };
    let Some(error) = value.get_mut("error").and_then(Value::as_object_mut) else {
        return body.clone();
    };
    let Some(code) = error.get("code").and_then(Value::as_i64) else {
        return body.clone();
    };
    error.insert("code".into(), Value::String(code.to_string()));
    error.entry("type").or_insert(Value::String(
        match code {
            400 => "invalid_request_error",
            401 => "authentication_error",
            402 => "insufficient_quota",
            403 => "permission_error",
            404 => "not_found_error",
            408 => "timeout_error",
            429 => "rate_limit_error",
            _ => "api_error",
        }
        .into(),
    ));
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .unwrap_or_else(|_| body.clone())
}
