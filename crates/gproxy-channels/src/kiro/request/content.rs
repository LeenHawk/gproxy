use gproxy_channel_api::ChannelError;
use serde_json::{Value, json};

pub(super) fn map_model(model: &str) -> String {
    let lower = model.to_ascii_lowercase().replace('_', "-");
    for (needle, replacement) in [
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
    ] {
        if lower.contains(needle) {
            return replacement.into();
        }
    }
    model.into()
}

pub(super) fn user(text: &str, model: &str, images: Vec<Value>) -> Value {
    user_with_images(text, model, images)
}

pub(super) fn user_with_images(text: &str, model: &str, images: Vec<Value>) -> Value {
    let content = fallback(text, !images.is_empty());
    let mut message = json!({
        "origin":"KIRO_CLI",
        "content":content,
        "modelId":model,
        "userInputMessageContext":{"editorState":{}}
    });
    if !images.is_empty() {
        message["images"] = Value::Array(images);
    }
    json!({"userInputMessage":message})
}

pub(super) fn assistant(text: String) -> Value {
    json!({"assistantResponseMessage":{"content":text}})
}

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
                    .ok_or_else(|| {
                        ChannelError::Prepare("Kiro text content is missing text".into())
                    })?
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
                    .ok_or_else(|| ChannelError::Prepare("Kiro image has no URL".into()))?;
                Ok((String::new(), vec![image(url)?]))
            }
            Some(kind) => Err(ChannelError::Prepare(format!(
                "Kiro does not support content type {kind}"
            ))),
            None => object
                .get("content")
                .map(text_and_images)
                .unwrap_or_else(|| Ok((String::new(), Vec::new()))),
        },
        Value::Null => Ok((String::new(), Vec::new())),
        _ => Err(ChannelError::Prepare(
            "Kiro only supports text and image content".into(),
        )),
    }
}

fn image(url: &str) -> Result<Value, ChannelError> {
    let (meta, bytes) = url
        .split_once(',')
        .ok_or_else(|| ChannelError::Prepare("Kiro image must be a data URL".into()))?;
    let format = meta
        .strip_prefix("data:image/")
        .and_then(|meta| meta.split(';').next())
        .filter(|format| !format.is_empty())
        .ok_or_else(|| ChannelError::Prepare("Kiro image must be image data".into()))?;
    Ok(json!({"format":format.to_ascii_lowercase(),"source":{"bytes":bytes}}))
}

fn fallback(text: &str, images: bool) -> String {
    match text.trim() {
        "" if images => "Please analyze the attached image.".into(),
        "" => ".".into(),
        text => text.into(),
    }
}
