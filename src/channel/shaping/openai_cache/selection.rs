use serde_json::{Map, Value};

use super::schema::{is_cacheable_chat_part, is_cacheable_response_part};

#[derive(Clone, Copy)]
pub(super) enum ChatLocation {
    ContentString(usize),
    ContentPart(usize, usize),
}

#[derive(Clone, Copy)]
pub(super) enum ResponsesLocation {
    Instructions,
    InputString,
    ContentString(usize),
    ContentPart(usize, usize),
}

pub(super) fn chat_system_locations(messages: &[Value]) -> Vec<ChatLocation> {
    let mut locations = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        if matches!(
            message.get("role").and_then(Value::as_str),
            Some("system" | "developer")
        ) {
            locations.extend(chat_content_locations(messages, index));
        }
    }
    locations
}

pub(super) fn chat_message_locations(messages: &[Value]) -> Vec<ChatLocation> {
    let mut locations = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        if message.get("role").and_then(Value::as_str) == Some("function") {
            continue;
        }
        locations.extend(chat_content_locations(messages, index));
    }
    locations
}

fn chat_content_locations(messages: &[Value], message_index: usize) -> Vec<ChatLocation> {
    match messages
        .get(message_index)
        .and_then(|message| message.get("content"))
    {
        Some(Value::String(text)) if !text.trim().is_empty() => {
            vec![ChatLocation::ContentString(message_index)]
        }
        Some(Value::Array(parts)) => (0..parts.len())
            .filter(|part_index| {
                parts
                    .get(*part_index)
                    .and_then(Value::as_object)
                    .is_some_and(is_cacheable_chat_part)
            })
            .map(|part_index| ChatLocation::ContentPart(message_index, part_index))
            .collect(),
        _ => Vec::new(),
    }
}

pub(super) fn response_system_locations(root: &Map<String, Value>) -> Vec<ResponsesLocation> {
    let mut locations = Vec::new();
    if root
        .get("instructions")
        .and_then(Value::as_str)
        .is_some_and(|text| !text.trim().is_empty())
    {
        locations.push(ResponsesLocation::Instructions);
    }
    let Some(items) = root.get("input").and_then(Value::as_array) else {
        return locations;
    };
    for (item_index, item) in items.iter().enumerate() {
        if matches!(
            item.get("role").and_then(Value::as_str),
            Some("system" | "developer")
        ) {
            locations.extend(response_content_locations(items, item_index));
        }
    }
    locations
}

pub(super) fn response_message_locations(root: &Map<String, Value>) -> Vec<ResponsesLocation> {
    match root.get("input") {
        Some(Value::String(text)) if !text.trim().is_empty() => {
            vec![ResponsesLocation::InputString]
        }
        Some(Value::Array(items)) => {
            let mut locations = Vec::new();
            for (index, item) in items.iter().enumerate() {
                if item.get("role").is_some() && item.get("content").is_some() {
                    locations.extend(response_content_locations(items, index));
                }
            }
            locations
        }
        _ => Vec::new(),
    }
}

fn response_content_locations(items: &[Value], item_index: usize) -> Vec<ResponsesLocation> {
    match items
        .get(item_index)
        .and_then(|message| message.get("content"))
    {
        Some(Value::String(text)) if !text.trim().is_empty() => {
            vec![ResponsesLocation::ContentString(item_index)]
        }
        Some(Value::Array(parts)) => (0..parts.len())
            .filter(|part_index| {
                parts
                    .get(*part_index)
                    .and_then(Value::as_object)
                    .is_some_and(is_cacheable_response_part)
            })
            .map(|part_index| ResponsesLocation::ContentPart(item_index, part_index))
            .collect(),
        _ => Vec::new(),
    }
}
