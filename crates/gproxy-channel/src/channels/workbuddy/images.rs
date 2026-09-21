//! WorkBuddy's image API, in both directions.
//!
//! The request is OpenAI's minus the fields Hunyuan has no place for, plus
//! two of its own (`footnote`, `revise`); an edit takes its sources under
//! `image` as bare data or URLs rather than as OpenAI's multipart parts. The
//! reply is the gateway envelope `{code, msg, data: {...}}`, which the channel
//! unwraps so the client sees an OpenAI image reply (v3 `workbuddy/shape.rs`).
//!
//! Only a JSON body is shaped. v3 rebuilt an OpenAI multipart form into JSON
//! through a shared multipart reader; v4 has none, and neither does `xai`,
//! which dropped the same rebuild for the same reason. A multipart edit is
//! forwarded as the caller wrote it and the upstream decides.

use super::envelope;
use crate::channel::ChannelError;
use gproxy_protocol::connection::Bytes;
use serde_json::{Map, Value};

/// What a generation body may carry.
const CREATE_FIELDS: &[&str] = &[
    "prompt",
    "background",
    "model",
    "n",
    "quality",
    "response_format",
    "size",
    "style",
    "footnote",
    "revise",
];

/// An edit additionally names its sources and how faithful to them to stay.
const EDIT_FIELDS: &[&str] = &[
    "image",
    "images",
    "prompt",
    "background",
    "input_fidelity",
    "model",
    "n",
    "quality",
    "response_format",
    "size",
    "style",
    "footnote",
    "revise",
];

/// The model a request that names none falls back to (v3 `shape.rs`).
const CREATE_MODEL: &str = "hunyuan-image-v3.0";
const EDIT_MODEL: &str = "hunyuan-image-v2.0-general-edit";

fn invalid(message: &str) -> ChannelError {
    ChannelError::InvalidConfig(format!("WorkBuddy: {message}"))
}

pub(super) fn request(body: &[u8], edit: bool) -> Result<Bytes, ChannelError> {
    let mut object: Map<String, Value> = serde_json::from_slice(body)
        .map_err(|_| invalid("an image request must be a JSON object"))?;
    let allowed = if edit { EDIT_FIELDS } else { CREATE_FIELDS };
    object.retain(|name, _| allowed.contains(&name.as_str()));
    object
        .entry("response_format")
        .or_insert_with(|| Value::String("b64_json".into()));
    let named = object
        .get("model")
        .and_then(Value::as_str)
        .is_some_and(|model| !model.trim().is_empty());
    if !named {
        let fallback = if edit { EDIT_MODEL } else { CREATE_MODEL };
        object.insert("model".into(), Value::String(fallback.into()));
    }
    if edit {
        let images = object
            .remove("images")
            .or_else(|| object.remove("image"))
            .ok_or_else(|| invalid("an image edit has no image"))?;
        object.insert("image".into(), sources(images));
    }
    Ok(Bytes::from(Value::Object(object).to_string()))
}

/// The upstream takes an array of bare sources; OpenAI's `{image_url: {url}}`
/// parts and `data:` prefixes are unwrapped.
fn sources(value: Value) -> Value {
    let images = match value {
        Value::Array(images) => images,
        image => vec![image],
    };
    Value::Array(
        images
            .into_iter()
            .map(|image| match image {
                Value::Object(object) => object
                    .get("image_url")
                    .or_else(|| object.get("url"))
                    .cloned()
                    .map(source)
                    .unwrap_or(Value::Object(object)),
                image => source(image),
            })
            .collect(),
    )
}

fn source(value: Value) -> Value {
    match value {
        Value::String(value) => {
            Value::String(value.strip_prefix("data:").unwrap_or(&value).to_owned())
        }
        Value::Object(object) => object
            .get("url")
            .cloned()
            .map(source)
            .unwrap_or(Value::Object(object)),
        other => other,
    }
}

/// Unwrap the gateway envelope around an image reply. A body that is not a
/// success envelope is returned exactly as it arrived.
pub(super) fn response(body: Bytes) -> Bytes {
    envelope::unwrap(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_generation_keeps_only_what_the_upstream_takes() {
        let body = json!({"prompt": "a cat", "model": "hunyuan-image-v3.0", "size": "1024x1024",
                          "moderation": "low", "user": "u", "footnote": true})
        .to_string();
        let out: Value = serde_json::from_slice(&request(body.as_bytes(), false).unwrap()).unwrap();
        assert_eq!(out["prompt"], "a cat");
        assert_eq!(out["footnote"], true);
        assert_eq!(out["response_format"], "b64_json");
        assert!(out.get("moderation").is_none());
        assert!(out.get("user").is_none());
    }

    #[test]
    fn an_edit_normalizes_its_sources_and_names_its_own_model() {
        let body = json!({"prompt": "brighter", "images": [
            "data:image/png;base64,QUJD",
            {"image_url": {"url": "https://a/2.png"}},
        ]})
        .to_string();
        let out: Value = serde_json::from_slice(&request(body.as_bytes(), true).unwrap()).unwrap();
        assert_eq!(out["model"], EDIT_MODEL);
        assert_eq!(
            out["image"],
            json!(["image/png;base64,QUJD", "https://a/2.png"])
        );
        assert!(out.get("images").is_none());
        assert!(request(json!({"prompt": "p"}).to_string().as_bytes(), true).is_err());
    }

    #[test]
    fn the_envelope_comes_off_a_successful_reply_only() {
        let wrapped = Bytes::from(
            json!({"code": 0, "msg": "ok", "data": {"data": [{"b64_json": "QUJD"}]}}).to_string(),
        );
        let out: Value = serde_json::from_slice(&response(wrapped)).unwrap();
        assert_eq!(out["data"][0]["b64_json"], "QUJD");
        assert_eq!(
            out["msg"], "ok",
            "outer fields the inner did not name survive"
        );

        let failed = Bytes::from(json!({"code": 11217, "msg": "pending"}).to_string());
        assert_eq!(response(failed.clone()), failed);
    }
}
