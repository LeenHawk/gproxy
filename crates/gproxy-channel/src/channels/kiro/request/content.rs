//! Turns, text and images as the conversation envelope spells them.

use super::invalid;
use crate::channel::ChannelError;
use serde_json::{Value, json};

/// Model ids the envelope accepts. The upstream spells a minor version with a
/// dot and takes no date suffix, so a client's `claude-sonnet-4-5` and
/// `claude-sonnet-4-20250514` have to be rewritten; the remaining rows are v3's
/// aliases for clients that only know OpenAI or older Claude names (v3
/// `request/content.rs`). Which models a provider actually offers is the
/// catalogue's business — this is only what the wire accepts.
const MODEL_IDS: &[(&str, &str)] = &[
    ("claude-sonnet-4-20250514", "claude-sonnet-4"),
    ("claude-sonnet-4-5", "claude-sonnet-4.5"),
    ("claude-sonnet-4-6", "claude-sonnet-4.6"),
    ("claude-opus-4-7", "claude-opus-4.7"),
    ("claude-haiku-4-5", "claude-haiku-4.5"),
    ("claude-opus-4-5", "claude-opus-4.5"),
    ("claude-opus-4-6", "claude-opus-4.6"),
    ("claude-3-5-sonnet", "claude-sonnet-4.5"),
    ("claude-3-opus", "claude-sonnet-4.5"),
    ("claude-3-sonnet", "claude-sonnet-4"),
    ("claude-3-haiku", "claude-haiku-4.5"),
    ("gpt-4-turbo", "claude-sonnet-4.5"),
    ("gpt-4o", "claude-sonnet-4.5"),
    ("gpt-4", "claude-sonnet-4.5"),
    ("gpt-3.5-turbo", "claude-sonnet-4.5"),
];

pub(in crate::channels::kiro) fn map_model(model: &str) -> String {
    let lower = model.to_ascii_lowercase().replace('_', "-");
    for (needle, replacement) in MODEL_IDS {
        if lower.contains(needle) {
            return (*replacement).into();
        }
    }
    model.into()
}

/// A user turn. `origin` and the empty editor state are what the Kiro CLI
/// sends on every turn.
pub(super) fn user(text: &str, model: &str, images: Vec<Value>) -> Value {
    let mut message = json!({
        "origin": "KIRO_CLI",
        "content": fallback(text, !images.is_empty()),
        "modelId": model,
        "userInputMessageContext": {"editorState": {}},
    });
    if !images.is_empty() {
        message["images"] = Value::Array(images);
    }
    json!({ "userInputMessage": message })
}

pub(super) fn assistant(text: String) -> Value {
    json!({"assistantResponseMessage": {"content": text}})
}

/// System text has no turn of its own: it becomes a user turn the assistant
/// immediately acknowledges, so the history stays alternating.
pub(super) fn push_system(messages: &mut Vec<Value>, system: Option<&str>, model: &str) {
    let Some(system) = system.map(str::trim).filter(|text| !text.is_empty()) else {
        return;
    };
    messages.push(user(system, model, Vec::new()));
    messages.push(assistant("I will follow these instructions.".into()));
}

pub(super) fn optional_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Null => None,
        Value::String(text) => Some(text.clone()),
        value => text_and_images(value).ok().map(|(text, _)| text),
    }
}

pub(super) fn join(left: Option<&str>, right: &str) -> String {
    [left.unwrap_or_default(), right]
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Responses content in any of its shapes, flattened into the one string the
/// envelope has room for plus the images it carries separately.
pub(super) fn text_and_images(value: &Value) -> Result<(String, Vec<Value>), ChannelError> {
    match value {
        Value::String(text) => Ok((text.clone(), Vec::new())),
        Value::Array(items) => {
            let mut texts = Vec::new();
            let mut images = Vec::new();
            for item in items {
                let (text, mut found) = text_and_images(item)?;
                if !text.is_empty() {
                    texts.push(text);
                }
                images.append(&mut found);
            }
            Ok((texts.join("\n"), images))
        }
        Value::Object(object) => match object.get("type").and_then(Value::as_str) {
            Some("input_text" | "text" | "output_text") => Ok((
                object
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("Kiro text content has no text"))?
                    .into(),
                Vec::new(),
            )),
            Some("input_image" | "image_url") => {
                let url = object
                    .get("image_url")
                    .and_then(|value| {
                        value
                            .as_str()
                            .or_else(|| value.get("url").and_then(Value::as_str))
                    })
                    .ok_or_else(|| invalid("Kiro image content has no URL"))?;
                Ok((String::new(), vec![image(url)?]))
            }
            Some(kind) => Err(invalid(format!("Kiro has no content type `{kind}`"))),
            None => object
                .get("content")
                .map(text_and_images)
                .unwrap_or_else(|| Ok((String::new(), Vec::new()))),
        },
        Value::Null => Ok((String::new(), Vec::new())),
        _ => Err(invalid("Kiro takes only text and image content")),
    }
}

/// The envelope carries an image as `{format, source: {bytes}}`, so only a
/// data URL can be forwarded; a remote URL has nothing to become.
fn image(url: &str) -> Result<Value, ChannelError> {
    let (meta, bytes) = url
        .split_once(',')
        .ok_or_else(|| invalid("Kiro images must be data URLs"))?;
    let format = meta
        .strip_prefix("data:image/")
        .and_then(|meta| meta.split(';').next())
        .filter(|format| !format.is_empty())
        .ok_or_else(|| invalid("Kiro images must be image data"))?;
    Ok(json!({"format": format.to_ascii_lowercase(), "source": {"bytes": bytes}}))
}

/// The upstream rejects an empty turn, and an image-only turn needs something
/// to ask about (v3 `request/content.rs::fallback`).
fn fallback(text: &str, images: bool) -> String {
    match text.trim() {
        "" if images => "Please analyze the attached image.".into(),
        "" => ".".into(),
        text => text.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minor_version_gains_its_dot_and_a_date_suffix_is_dropped() {
        assert_eq!(map_model("claude-sonnet-4-5"), "claude-sonnet-4.5");
        assert_eq!(map_model("CLAUDE_SONNET_4_5"), "claude-sonnet-4.5");
        assert_eq!(map_model("claude-sonnet-4-20250514"), "claude-sonnet-4");
        assert_eq!(map_model("claude-sonnet-4.5"), "claude-sonnet-4.5");
        assert_eq!(map_model("kiro-custom"), "kiro-custom");
    }

    #[test]
    fn content_flattens_to_one_string_and_data_urls_become_image_parts() {
        let (text, images) = text_and_images(&json!([
            {"type": "input_text", "text": "look"},
            {"type": "input_image", "image_url": {"url": "data:image/PNG;base64,QUJD"}},
        ]))
        .unwrap();
        assert_eq!(text, "look");
        assert_eq!(
            images,
            vec![json!({"format": "png", "source": {"bytes": "QUJD"}})]
        );
        assert!(
            text_and_images(&json!([{"type": "input_image", "image_url": "https://a/1.png"}]))
                .is_err()
        );
        assert!(text_and_images(&json!([{"type": "input_audio"}])).is_err());
    }

    #[test]
    fn an_empty_turn_still_says_something() {
        let message = user("  ", "m", Vec::new());
        assert_eq!(message["userInputMessage"]["content"], ".");
        let with_image = user("", "m", vec![json!({})]);
        assert_eq!(
            with_image["userInputMessage"]["content"],
            "Please analyze the attached image."
        );
    }
}
