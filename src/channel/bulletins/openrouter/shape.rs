//! OpenRouter response shaping (整形).
//!
//! OpenRouter error shape: `{error: {code: int, message: str, metadata?}, ...}`.
//! OpenAI's error model requires `message` + `type` as strings and an optional
//! string `code`. Coerce `code` to string and synthesize `type` from the
//! numeric code so downstream transforms deserialize cleanly. Best-effort:
//! returns the input unchanged on parse failure or when there is no int
//! `error.code`.

use bytes::Bytes;
use serde_json::Value;

/// Map an OpenRouter numeric error code to an OpenAI-style `error.type`.
fn error_type_for(code: i64) -> &'static str {
    match code {
        400 => "invalid_request_error",
        401 => "authentication_error",
        402 => "insufficient_quota",
        403 => "permission_error",
        404 => "not_found_error",
        408 => "timeout_error",
        429 => "rate_limit_error",
        500..=599 => "api_error",
        _ => "api_error",
    }
}

/// Reshape an OpenRouter error body: int `error.code` -> string, synthesize
/// `error.type` from the code if absent. No-op for non-error bodies, bodies
/// whose `error.code` is not an integer, or bodies that fail to parse.
pub(super) fn reshape_error(body: Bytes) -> Bytes {
    let Ok(mut v) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(err) = v.get_mut("error").and_then(Value::as_object_mut) else {
        return body;
    };
    let Some(code) = err.get("code").and_then(Value::as_i64) else {
        return body;
    };
    err.insert("code".to_string(), Value::from(code.to_string()));
    err.entry("type")
        .or_insert_with(|| Value::from(error_type_for(code)));
    serde_json::to_vec(&v).map(Bytes::from).unwrap_or(body)
}

/// Reshape an OpenRouter `/v1/models` body so it deserializes under the strict
/// OpenAI model-list shape on the proxy `/v1/models` path. OpenRouter returns
/// `{"data": [{"id", "name", ...}, ...]}` WITHOUT the top-level `object:
/// "list"` wrapper, and each item omits `object: "model"` and `owned_by`. Fill
/// the top-level `object` and per-item `object` + `owned_by` (derived from the
/// `id` org prefix before `/`, defaulting to `openrouter`). `id` is left intact
/// (`parse_models` reads it). No-op for error bodies or bodies that fail to
/// parse / lack a JSON object.
pub(super) fn reshape_model_list(body: Bytes) -> Bytes {
    let Ok(mut v) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(obj) = v.as_object_mut() else {
        return body;
    };
    if obj.contains_key("error") {
        return body;
    }
    obj.entry("object").or_insert_with(|| Value::from("list"));
    if let Some(arr) = obj.get_mut("data").and_then(Value::as_array_mut) {
        for item in arr {
            fill_model_fields(item);
        }
    }
    serde_json::to_vec(&v).map(Bytes::from).unwrap_or(body)
}

/// Fill an OpenAI model item's `object: "model"` and `owned_by` (derived from
/// the `id`'s org prefix before `/`, defaulting to `openrouter`). Existing
/// values are preserved.
fn fill_model_fields(item: &mut Value) {
    let Some(obj) = item.as_object_mut() else {
        return;
    };
    obj.entry("object").or_insert_with(|| Value::from("model"));
    if !obj.contains_key("owned_by") {
        let owner = obj
            .get("id")
            .and_then(Value::as_str)
            .and_then(|s| s.split_once('/'))
            .map(|(org, _)| org.to_string())
            .unwrap_or_else(|| "openrouter".to_string());
        obj.insert("owned_by".to_string(), Value::from(owner));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn int_code_becomes_string_and_type_synthesized() {
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "error": { "code": 429, "message": "rate limited" }
            }))
            .unwrap(),
        );
        let out = reshape_error(body);
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["error"]["code"], "429");
        assert_eq!(v["error"]["type"], "rate_limit_error");
        assert_eq!(v["error"]["message"], "rate limited");
    }

    #[test]
    fn existing_type_is_preserved() {
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "error": { "code": 400, "type": "custom_type", "message": "bad" }
            }))
            .unwrap(),
        );
        let out = reshape_error(body);
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["error"]["code"], "400");
        assert_eq!(v["error"]["type"], "custom_type");
    }

    #[test]
    fn non_error_body_is_untouched() {
        let raw = br#"{"data":[{"id":"x"}],"object":"list"}"#;
        let out = reshape_error(Bytes::from_static(raw));
        assert_eq!(out.as_ref(), raw);
    }

    #[test]
    fn non_int_code_is_untouched() {
        // Already a string code -> leave it (and any absent type) alone.
        let raw = br#"{"error":{"code":"already_str","message":"x"}}"#;
        let out = reshape_error(Bytes::from_static(raw));
        assert_eq!(out.as_ref(), raw);
    }

    #[test]
    fn parse_failure_returns_input() {
        let raw = b"not json at all";
        let out = reshape_error(Bytes::from_static(raw));
        assert_eq!(out.as_ref(), raw);
    }

    #[test]
    fn model_list_fills_object_wrapper_and_per_item_fields() {
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "data": [
                    { "id": "anthropic/claude-3.5-sonnet", "name": "Claude 3.5 Sonnet" },
                    { "id": "openai/gpt-4o", "name": "GPT-4o" },
                    { "id": "noslash", "name": "Bare" }
                ]
            }))
            .unwrap(),
        );
        let out = reshape_model_list(body);
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["object"], "list");
        let data = v["data"].as_array().unwrap();
        assert_eq!(data[0]["object"], "model");
        assert_eq!(data[0]["owned_by"], "anthropic");
        // id is left intact for parse_models.
        assert_eq!(data[0]["id"], "anthropic/claude-3.5-sonnet");
        assert_eq!(data[1]["owned_by"], "openai");
        // No org prefix -> default owner.
        assert_eq!(data[2]["owned_by"], "openrouter");
    }

    #[test]
    fn model_list_preserves_existing_fields_and_skips_errors() {
        // Existing object/owned_by are kept.
        let body = Bytes::from(
            serde_json::to_vec(&json!({
                "object": "list",
                "data": [{ "id": "anthropic/x", "object": "model", "owned_by": "custom" }]
            }))
            .unwrap(),
        );
        let out = reshape_model_list(body);
        let v: Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["data"][0]["owned_by"], "custom");

        // Error body is untouched.
        let raw = br#"{"error":{"code":404,"message":"no models"}}"#;
        let out = reshape_model_list(Bytes::from_static(raw));
        assert_eq!(out.as_ref(), raw);

        // Parse failure returns input.
        let bad = b"not json";
        let out = reshape_model_list(Bytes::from_static(bad));
        assert_eq!(out.as_ref(), bad);
    }
}
