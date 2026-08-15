//! OpenAI-compatible request and response shaping for xAI's public media API.

use bytes::Bytes;
use serde_json::{Map, Value, json};

use crate::protocol::Operation;

pub(crate) fn image_request(body: Bytes) -> Bytes {
    crate::channel::shaping::with_json_body(body, |value| {
        let Some(map) = value.as_object_mut() else {
            return;
        };
        for key in ["moderation", "partial_images", "size", "stream"] {
            map.remove(key);
        }
    })
}

pub(crate) fn image_edit_request(body: Bytes) -> Bytes {
    let body = image_request(body);
    crate::channel::shaping::with_json_body(body, |value| {
        let Some(map) = value.as_object_mut() else {
            return;
        };
        map.remove("mask");
        let Some(images) = map.remove("image") else {
            return;
        };
        match images {
            Value::Array(images) if images.len() == 1 => {
                map.insert(
                    "image".into(),
                    image_source(images.into_iter().next().unwrap()),
                );
            }
            Value::Array(images) => {
                map.insert(
                    "images".into(),
                    Value::Array(images.into_iter().map(image_source).collect()),
                );
            }
            image => {
                map.insert("image".into(), image_source(image));
            }
        }
    })
}

pub(crate) fn speech_request(body: Bytes) -> Bytes {
    crate::channel::shaping::with_json_body(body, |value| {
        let Some(map) = value.as_object_mut() else {
            return;
        };
        map.remove("model");
        map.remove("instructions");
        map.remove("stream_format");
        if let Some(input) = map.remove("input") {
            map.entry("text").or_insert(input);
        }
        if let Some(voice) = map.remove("voice") {
            map.entry("voice_id").or_insert(voice);
        }
        if let Some(format) = map.remove("response_format") {
            let output_format = map
                .entry("output_format")
                .or_insert_with(|| Value::Object(Map::new()));
            if let Some(output_format) = output_format.as_object_mut() {
                output_format.entry("codec").or_insert(format);
            }
        }
    })
}

pub(crate) fn transcription_request(body: Bytes) -> Bytes {
    crate::channel::shaping::with_json_body(body, |value| {
        let Some(map) = value.as_object_mut() else {
            return;
        };
        for key in [
            "model",
            "prompt",
            "response_format",
            "stream",
            "temperature",
            "timestamp_granularities",
        ] {
            map.remove(key);
        }
    })
}

pub(crate) fn video_request(body: Bytes, operation: Operation) -> Bytes {
    crate::channel::shaping::with_json_body(body, |value| {
        let Some(map) = value.as_object_mut() else {
            return;
        };
        if let Some(seconds) = map.remove("seconds") {
            let duration = seconds
                .as_str()
                .and_then(|value| value.parse::<u64>().ok())
                .map(Value::from)
                .unwrap_or(seconds);
            map.entry("duration").or_insert(duration);
        }
        if operation == Operation::CreateVideo {
            if let Some(reference) = map.remove("input_reference") {
                map.entry("image").or_insert(image_source(reference));
            }
            return;
        }
        if let Some(video) = map.get_mut("video")
            && let Some(url) = video.as_str().map(str::to_owned)
        {
            *video = json!({ "url": url });
        }
    })
}

pub(crate) fn video_response(body: Bytes) -> Bytes {
    crate::channel::shaping::with_json_body(body, |value| {
        let Some(map) = value.as_object_mut() else {
            return;
        };
        if map.get("id").is_none()
            && let Some(id) = map
                .get("request_id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        {
            map.insert("id".into(), Value::String(id));
        }
        let status = match map.get("status").and_then(Value::as_str) {
            Some("done" | "succeeded" | "success") => Some("completed"),
            Some("pending" | "processing") => Some("in_progress"),
            None if map.get("video").is_some() => Some("completed"),
            None => Some("queued"),
            _ => None,
        };
        if let Some(status) = status {
            map.insert("status".into(), Value::String(status.into()));
        }
        if map.get("url").is_none()
            && let Some(url) = map
                .get("video")
                .and_then(|video| video.get("url"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        {
            map.insert("url".into(), Value::String(url));
        }
    })
}

fn image_source(value: Value) -> Value {
    match value {
        Value::String(url) => json!({ "url": url }),
        value => value,
    }
}
