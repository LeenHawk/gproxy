//! DashScope's native multimodal-generation API, in both directions.
//!
//! Images do not go through the OpenAI-compatible mode at all: the request is
//! an envelope of `{model, input.messages[].content[], parameters}` and the
//! reply nests the result under `output.choices[0].message.content[].image`.
//! The channel converts an OpenAI image request into that envelope and folds
//! the reply back into `{data: [{url}]}`, parking the vendor's own usage
//! object under `dashscope_usage` for the usage extractor to read.

use crate::channel::ChannelError;
use gproxy_protocol::connection::Bytes;
use serde_json::{Map, Value, json};

/// The path the envelope is posted to, under the origin rather than under
/// any compatibility prefix.
pub(super) const PATH: &str = "/api/v1/services/aigc/multimodal-generation/generation";

/// OpenAI image parameters DashScope does not accept.
const OPENAI_ONLY: &[&str] = &[
    "background",
    "input_fidelity",
    "mask",
    "moderation",
    "output_compression",
    "output_format",
    "partial_images",
    "quality",
    "response_format",
    "stream",
    "style",
    "user",
];

fn invalid(message: &str) -> ChannelError {
    ChannelError::InvalidConfig(message.into())
}

/// An image reference, however the caller wrote it: a bare URL string, an
/// `{url}` object or OpenAI's `{image_url: {url}}` part.
fn image_url(value: &Value) -> Option<String> {
    match value {
        Value::String(url) => Some(url.clone()),
        Value::Object(_) => value
            .get("url")
            .or_else(|| value.pointer("/image_url/url"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => None,
    }
}

/// Convert an OpenAI image request into the DashScope envelope. `edit` also
/// carries the source images, which lead the content array.
pub(super) fn request(body: &[u8], edit: bool) -> Result<Bytes, ChannelError> {
    let mut object: Map<String, Value> = serde_json::from_slice(body)
        .map_err(|_| invalid("DashScope image requests must be a JSON object"))?;
    let model = object.remove("model");
    let prompt = object
        .remove("prompt")
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| invalid("DashScope image requests need a prompt"))?;
    let mut content = Vec::new();
    if edit {
        let sources = match (object.remove("image"), object.remove("images")) {
            (_, Some(Value::Array(values))) | (Some(Value::Array(values)), None) => values,
            (Some(value), _) => vec![value],
            (None, Some(value)) => vec![value],
            (None, None) => Vec::new(),
        };
        for source in &sources {
            let url = image_url(source)
                .ok_or_else(|| invalid("DashScope image editing needs image URLs"))?;
            content.push(json!({"image": url}));
        }
        if content.is_empty() {
            return Err(invalid("DashScope image editing needs at least one image"));
        }
    }
    content.push(json!({"text": prompt}));
    for name in OPENAI_ONLY {
        object.remove(*name);
    }
    // DashScope spells a resolution with a star and watermarks by default.
    if let Some(Value::String(size)) = object.get("size") {
        let size = size.replace('x', "*");
        object.insert("size".into(), Value::String(size));
    }
    object
        .entry("watermark".to_owned())
        .or_insert(Value::Bool(false));
    let envelope = json!({
        "model": model.unwrap_or(Value::Null),
        "input": {"messages": [{"role": "user", "content": content}]},
        "parameters": Value::Object(object),
    });
    Ok(Bytes::from(envelope.to_string()))
}

/// Fold the envelope back into an OpenAI image reply. A body that is not the
/// envelope (an error document) is returned untouched.
pub(super) fn response(body: Bytes) -> Bytes {
    let Ok(root) = serde_json::from_slice::<Value>(&body) else {
        return body;
    };
    let Some(content) = root
        .pointer("/output/choices/0/message/content")
        .and_then(Value::as_array)
    else {
        return body;
    };
    let data: Vec<Value> = content
        .iter()
        .filter_map(|part| part.get("image").and_then(Value::as_str))
        .map(|url| json!({"url": url}))
        .collect();
    if data.is_empty() {
        return body;
    }
    let mut reply = json!({"data": data});
    for name in ["request_id", "id"] {
        if let Some(value) = root.get(name) {
            reply[name] = value.clone();
        }
    }
    if let Some(usage) = root.get("usage") {
        // Kept under the vendor's name: it is not an OpenAI usage object and
        // must not be mistaken for one.
        reply["dashscope_usage"] = usage.clone();
    }
    Bytes::from(reply.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_openai_image_request_becomes_the_envelope() {
        let body = json!({"model": "qwen-image", "prompt": "a cat", "size": "1024x1024",
            "response_format": "url", "n": 2, "seed": 7})
        .to_string();
        let out: Value = serde_json::from_slice(&request(body.as_bytes(), false).unwrap()).unwrap();
        assert_eq!(out["model"], "qwen-image");
        assert_eq!(
            out["input"]["messages"][0]["content"],
            json!([{"text": "a cat"}])
        );
        assert_eq!(out["parameters"]["size"], "1024*1024");
        assert_eq!(out["parameters"]["watermark"], false);
        assert_eq!(out["parameters"]["seed"], 7, "unknown extras survive");
        assert!(out["parameters"].get("response_format").is_none());

        let edit = json!({"model": "m", "prompt": "brighter",
            "image": ["https://a/1.png", {"image_url": {"url": "https://a/2.png"}}]})
        .to_string();
        let out: Value = serde_json::from_slice(&request(edit.as_bytes(), true).unwrap()).unwrap();
        assert_eq!(
            out["input"]["messages"][0]["content"],
            json!([
                {"image": "https://a/1.png"},
                {"image": "https://a/2.png"},
                {"text": "brighter"}
            ])
        );
        assert!(matches!(
            request(json!({"model": "m"}).to_string().as_bytes(), false),
            Err(ChannelError::InvalidConfig(_))
        ));
    }

    #[test]
    fn the_envelope_reply_becomes_an_openai_image_reply() {
        let upstream = json!({"request_id": "r1", "output": {"choices": [{"message":
            {"content": [{"image": "https://cdn/1.png"}, {"text": "ignored"}]}}]},
            "usage": {"image_count": 1, "input_tokens": 10}})
        .to_string();
        let out: Value = serde_json::from_slice(&response(Bytes::from(upstream.clone()))).unwrap();
        assert_eq!(out["data"], json!([{"url": "https://cdn/1.png"}]));
        assert_eq!(out["request_id"], "r1");
        assert_eq!(out["dashscope_usage"]["image_count"], 1);
        assert!(out.get("usage").is_none(), "not an OpenAI usage object");

        let error = Bytes::from(json!({"code": "InvalidParameter"}).to_string());
        assert_eq!(response(error.clone()), error);
    }
}
