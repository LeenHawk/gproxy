use gproxy_protocol::ContentGenerationKind;
use serde_json::{Map, Value, json};

use super::CacheBreakpointConfig;

pub(super) fn apply(
    body: &mut Value,
    kind: Option<ContentGenerationKind>,
    config: &CacheBreakpointConfig,
) -> bool {
    match kind {
        Some(ContentGenerationKind::ClaudeMessages) => claude(body, config),
        Some(ContentGenerationKind::OpenAiChat) => openai_chat(body, config),
        Some(
            ContentGenerationKind::OpenAiResponses
            | ContentGenerationKind::OpenAiResponsesWebSocket,
        ) => openai_responses(body, config),
        Some(ContentGenerationKind::GeminiGenerateContent) | None => false,
    }
}

#[derive(Clone, Copy)]
enum ClaudeLocation {
    Tool(usize),
    System(usize),
    Message(usize, usize),
}

fn claude(body: &mut Value, config: &CacheBreakpointConfig) -> bool {
    canonicalize_claude(body);
    let Some(root) = body.as_object_mut() else {
        return false;
    };
    if matches!(config.target.as_str(), "top_level" | "global") {
        if root.contains_key("cache_control") || claude_marker_count(root) >= 4 {
            return false;
        }
        root.insert("cache_control".into(), claude_marker(config));
        return true;
    }
    let locations = claude_locations(root, &config.target);
    let Some(location) = select(&locations, config.index) else {
        return false;
    };
    if claude_marker_count(root) >= 4 {
        return false;
    }
    let block = match location {
        ClaudeLocation::Tool(index) => root
            .get_mut("tools")
            .and_then(Value::as_array_mut)
            .and_then(|values| values.get_mut(index)),
        ClaudeLocation::System(index) => root
            .get_mut("system")
            .and_then(Value::as_array_mut)
            .and_then(|values| values.get_mut(index)),
        ClaudeLocation::Message(message, block) => root
            .get_mut("messages")
            .and_then(Value::as_array_mut)
            .and_then(|values| values.get_mut(message))
            .and_then(|message| message.get_mut("content"))
            .and_then(Value::as_array_mut)
            .and_then(|values| values.get_mut(block)),
    };
    let Some(block) = block.and_then(Value::as_object_mut) else {
        return false;
    };
    if block.contains_key("cache_control") {
        return false;
    }
    block.insert("cache_control".into(), claude_marker(config));
    true
}

fn claude_locations(root: &Map<String, Value>, target: &str) -> Vec<ClaudeLocation> {
    match target {
        "tools" => root
            .get("tools")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
            .filter(|(_, value)| value.is_object())
            .map(|(index, _)| ClaudeLocation::Tool(index))
            .collect(),
        "system" => root
            .get("system")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
            .filter(|(_, value)| value.as_object().is_some_and(cacheable_claude))
            .map(|(index, _)| ClaudeLocation::System(index))
            .collect(),
        "message" => {
            let mut locations = Vec::new();
            if let Some(messages) = root.get("messages").and_then(Value::as_array) {
                for (message_index, message) in messages.iter().enumerate() {
                    let role = message.get("role").and_then(Value::as_str);
                    if let Some(blocks) = message.get("content").and_then(Value::as_array) {
                        for (block_index, block) in blocks.iter().enumerate() {
                            if block.as_object().is_some_and(|block| {
                                cacheable_claude(block)
                                    && (!matches!(
                                        block.get("type").and_then(Value::as_str),
                                        Some("image" | "document")
                                    ) || role == Some("user"))
                            }) {
                                locations.push(ClaudeLocation::Message(message_index, block_index));
                            }
                        }
                    }
                }
            }
            locations
        }
        _ => Vec::new(),
    }
}

fn canonicalize_claude(body: &mut Value) {
    let Some(root) = body.as_object_mut() else {
        return;
    };
    if let Some(system) = root.get_mut("system") {
        canonicalize_content(system);
    }
    if let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            if let Some(content) = message.get_mut("content") {
                canonicalize_content(content);
            }
        }
    }
}

fn canonicalize_content(content: &mut Value) {
    match content {
        Value::String(text) => {
            *content = json!([{"type":"text","text":std::mem::take(text)}]);
        }
        Value::Object(_) => *content = Value::Array(vec![std::mem::take(content)]),
        Value::Array(values) => {
            for value in values {
                if let Value::String(text) = value {
                    *value = json!({"type":"text","text":std::mem::take(text)});
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn cacheable_claude(block: &Map<String, Value>) -> bool {
    match block.get("type").and_then(Value::as_str) {
        Some(
            "thinking"
            | "redacted_thinking"
            | "citation"
            | "citations"
            | "char_location"
            | "page_location"
            | "content_block_location",
        ) => false,
        Some("text") => block
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.trim().is_empty()),
        _ => true,
    }
}

fn claude_marker_count(root: &Map<String, Value>) -> usize {
    usize::from(root.contains_key("cache_control"))
        + ["tools", "system"]
            .into_iter()
            .filter_map(|name| root.get(name).and_then(Value::as_array))
            .flatten()
            .filter(|value| value.get("cache_control").is_some())
            .count()
        + root
            .get("messages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|message| message.get("content").and_then(Value::as_array))
            .flatten()
            .filter(|value| value.get("cache_control").is_some())
            .count()
}

fn claude_marker(config: &CacheBreakpointConfig) -> Value {
    config.ttl.as_ref().map_or_else(
        || json!({"type":"ephemeral"}),
        |ttl| json!({"type":"ephemeral","ttl":ttl}),
    )
}

#[derive(Clone, Copy)]
enum OpenAiLocation {
    Instructions,
    InputString,
    ContentString(usize),
    ContentPart(usize, usize),
}

fn openai_chat(body: &mut Value, config: &CacheBreakpointConfig) -> bool {
    let Some(root) = body.as_object_mut() else {
        return false;
    };
    if openai_global(root, config) {
        return true;
    }
    let Some(messages) = root.get("messages").and_then(Value::as_array) else {
        return false;
    };
    let mut locations = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        let role = message.get("role").and_then(Value::as_str);
        if config.target == "system" && !matches!(role, Some("system" | "developer")) {
            continue;
        }
        if config.target != "message" && config.target != "system" || role == Some("function") {
            continue;
        }
        locations.extend(openai_content_locations(messages, index, true));
    }
    let Some(location) = select(&locations, config.index) else {
        return false;
    };
    let changed = stamp_openai(root, location, true);
    set_openai_ttl(root, config, changed);
    changed
}

fn openai_responses(body: &mut Value, config: &CacheBreakpointConfig) -> bool {
    let Some(root) = body.as_object_mut() else {
        return false;
    };
    if openai_global(root, config) {
        return true;
    }
    let mut locations = Vec::new();
    if config.target == "system" {
        if root
            .get("instructions")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.trim().is_empty())
        {
            locations.push(OpenAiLocation::Instructions);
        }
        if let Some(items) = root.get("input").and_then(Value::as_array) {
            for (index, item) in items.iter().enumerate() {
                if matches!(
                    item.get("role").and_then(Value::as_str),
                    Some("system" | "developer")
                ) {
                    locations.extend(openai_content_locations(items, index, false));
                }
            }
        }
    } else if config.target == "message" {
        match root.get("input") {
            Some(Value::String(text)) if !text.trim().is_empty() => {
                locations.push(OpenAiLocation::InputString)
            }
            Some(Value::Array(items)) => {
                for (index, item) in items.iter().enumerate() {
                    if item.get("role").is_some() && item.get("content").is_some() {
                        locations.extend(openai_content_locations(items, index, false));
                    }
                }
            }
            _ => {}
        }
    }
    let Some(location) = select(&locations, config.index) else {
        return false;
    };
    let changed = stamp_openai(root, location, false);
    set_openai_ttl(root, config, changed);
    changed
}

fn openai_content_locations(messages: &[Value], index: usize, chat: bool) -> Vec<OpenAiLocation> {
    match messages
        .get(index)
        .and_then(|message| message.get("content"))
    {
        Some(Value::String(text)) if !text.trim().is_empty() => {
            vec![OpenAiLocation::ContentString(index)]
        }
        Some(Value::Array(parts)) => parts
            .iter()
            .enumerate()
            .filter(|(_, part)| {
                part.as_object()
                    .is_some_and(|part| cacheable_openai(part, chat))
            })
            .map(|(part, _)| OpenAiLocation::ContentPart(index, part))
            .collect(),
        _ => Vec::new(),
    }
}

fn cacheable_openai(part: &Map<String, Value>, chat: bool) -> bool {
    let kind = part.get("type").and_then(Value::as_str);
    let supported = if chat {
        matches!(
            kind,
            Some("text" | "image_url" | "input_audio" | "file" | "refusal")
        )
    } else {
        matches!(
            kind,
            Some("input_text" | "input_image" | "input_file" | "output_text" | "refusal")
        )
    };
    supported
        && match kind {
            Some("text" | "input_text" | "output_text") => part
                .get("text")
                .and_then(Value::as_str)
                .is_some_and(|text| !text.trim().is_empty()),
            Some("refusal") => part
                .get("refusal")
                .and_then(Value::as_str)
                .is_some_and(|text| !text.trim().is_empty()),
            _ => true,
        }
}

fn stamp_openai(root: &mut Map<String, Value>, location: OpenAiLocation, chat: bool) -> bool {
    match location {
        OpenAiLocation::Instructions => prepend_anchor(root),
        OpenAiLocation::InputString => {
            let Some(text) = root
                .remove("input")
                .and_then(|value| value.as_str().map(str::to_owned))
            else {
                return false;
            };
            root.insert(
                "input".into(),
                Value::Array(vec![openai_message("user", text, true)]),
            );
            true
        }
        OpenAiLocation::ContentString(message) => {
            let Some(item) = root
                .get_mut(if chat { "messages" } else { "input" })
                .and_then(Value::as_array_mut)
                .and_then(|items| items.get_mut(message))
            else {
                return false;
            };
            let role = item.get("role").and_then(Value::as_str).map(str::to_owned);
            let Some(content) = item.get_mut("content") else {
                return false;
            };
            let text = content.as_str().unwrap_or_default().to_owned();
            let kind = if chat {
                "text"
            } else if role.as_deref() == Some("assistant") {
                "output_text"
            } else {
                "input_text"
            };
            *content = Value::Array(vec![openai_part(kind, text, true)]);
            true
        }
        OpenAiLocation::ContentPart(message, part) => {
            let Some(part) = root
                .get_mut(if chat { "messages" } else { "input" })
                .and_then(Value::as_array_mut)
                .and_then(|items| items.get_mut(message))
                .and_then(|item| item.get_mut("content"))
                .and_then(Value::as_array_mut)
                .and_then(|parts| parts.get_mut(part))
                .and_then(Value::as_object_mut)
            else {
                return false;
            };
            part.entry("prompt_cache_breakpoint")
                .or_insert_with(breakpoint);
            true
        }
    }
}

fn prepend_anchor(root: &mut Map<String, Value>) -> bool {
    let mut input = match root.remove("input") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::String(text)) => vec![openai_message("user", text, false)],
        Some(Value::Array(items)) => items,
        Some(other) => {
            root.insert("input".into(), other);
            return false;
        }
    };
    input.insert(0, openai_message("developer", " ".into(), true));
    root.insert("input".into(), Value::Array(input));
    true
}

fn openai_message(role: &str, text: String, marked: bool) -> Value {
    let kind = if role == "assistant" {
        "output_text"
    } else {
        "input_text"
    };
    json!({"type":"message","role":role,"content":[openai_part(kind,text,marked)]})
}

fn openai_part(kind: &str, text: String, marked: bool) -> Value {
    let mut part = json!({"type":kind,"text":text});
    if marked {
        part["prompt_cache_breakpoint"] = breakpoint();
    }
    part
}

fn breakpoint() -> Value {
    json!({"mode":"explicit"})
}

fn openai_global(root: &mut Map<String, Value>, config: &CacheBreakpointConfig) -> bool {
    if !matches!(config.target.as_str(), "top_level" | "global") {
        return false;
    }
    let options = root
        .entry("prompt_cache_options")
        .or_insert_with(|| json!({}));
    let Some(options) = options.as_object_mut() else {
        return false;
    };
    options.entry("mode").or_insert_with(|| json!("implicit"));
    if config.ttl.as_deref() == Some("30m") {
        options.insert("ttl".into(), json!("30m"));
    }
    true
}

fn set_openai_ttl(root: &mut Map<String, Value>, config: &CacheBreakpointConfig, changed: bool) {
    if changed && config.ttl.as_deref() == Some("30m") {
        let options = root
            .entry("prompt_cache_options")
            .or_insert_with(|| json!({}));
        if let Some(options) = options.as_object_mut() {
            options.insert("ttl".into(), json!("30m"));
        }
    }
}

fn select<T: Copy>(values: &[T], index: Option<i64>) -> Option<T> {
    let resolved = match index {
        None => values.len().checked_sub(1),
        Some(0) => None,
        Some(index) if index > 0 => usize::try_from(index)
            .ok()
            .filter(|index| *index <= values.len())
            .map(|index| index - 1),
        Some(index) => usize::try_from(index.unsigned_abs())
            .ok()
            .filter(|index| *index <= values.len())
            .map(|index| values.len() - index),
    }?;
    values.get(resolved).copied()
}
