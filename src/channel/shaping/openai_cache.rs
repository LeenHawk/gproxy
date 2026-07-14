//! Explicit prompt-cache breakpoints for OpenAI Chat/Responses request bodies.
//!
//! OpenAI uses content-part `prompt_cache_breakpoint` markers instead of
//! Anthropic's `cache_control`. The same frozen GPROXY magic strings are
//! accepted so clients can use one trigger convention across protocol
//! families. OpenAI TTL is request-wide (`prompt_cache_options.ttl`), so the
//! magic string's Claude-specific auto/5m/1h distinction is intentionally not
//! copied onto individual OpenAI breakpoints.

use serde_json::{Map, Value, json};

use super::claude_magic_cache;
use crate::protocol::{ContentGenerationKind, OperationKey, OperationKind};

const MAX_BREAKPOINTS: usize = 4;

#[derive(Clone, Copy)]
enum ChatLocation {
    ContentString(usize),
    ContentPart(usize, usize),
}

#[derive(Clone, Copy)]
enum ResponsesLocation {
    Instructions,
    InputString,
    ContentString(usize),
    ContentPart(usize, usize),
}

/// Return the OpenAI content-generation kind carried by an existing route.
pub fn kind_for_operation(op: OperationKey) -> Option<ContentGenerationKind> {
    match op.kind {
        OperationKind::ContentGeneration(
            kind @ (ContentGenerationKind::OpenAiChatCompletions
            | ContentGenerationKind::OpenAiResponses
            | ContentGenerationKind::OpenAiResponsesWebSocket),
        ) => Some(kind),
        _ => None,
    }
}

/// Strip GPROXY magic strings and stamp explicit OpenAI cache breakpoints.
/// Existing markers remain available for OpenAI's read window; only markers
/// added by this pass count toward its four-new-writes limit.
pub fn apply_magic_string_cache_breakpoints(body: &mut Value, kind: ContentGenerationKind) {
    // OpenAI can read older markers from prior turns and independently limits
    // each request to four new writes. Cap only markers added by this pass;
    // existing markers must not suppress a new boundary on the latest turn.
    let mut remaining = MAX_BREAKPOINTS;
    let Some(root) = body.as_object_mut() else {
        return;
    };
    match kind {
        ContentGenerationKind::OpenAiChatCompletions => apply_chat_magic(root, &mut remaining),
        ContentGenerationKind::OpenAiResponses
        | ContentGenerationKind::OpenAiResponsesWebSocket => {
            apply_responses_magic(root, &mut remaining)
        }
        _ => {}
    }
}

/// Apply the existing manual `cache_breakpoint` rule to an OpenAI body.
///
/// `system` and `message` select content blocks. OpenAI does not support
/// breakpoints on tool definitions. `top_level`/`global` configures implicit
/// request-wide caching, matching the old target's global-policy semantics.
pub fn apply_manual_cache_breakpoint(
    body: &mut Value,
    kind: ContentGenerationKind,
    target: &str,
    index: Option<i64>,
    ttl: Option<&str>,
) -> Result<(), &'static str> {
    let root = body.as_object_mut().ok_or("body not an object")?;

    if matches!(target, "top_level" | "global") {
        set_prompt_cache_options(root, Some("implicit"), ttl)?;
        return Ok(());
    }
    if target == "tools" {
        return Err("OpenAI does not support cache breakpoints on tools");
    }

    match kind {
        ContentGenerationKind::OpenAiChatCompletions => apply_chat_manual(root, target, index)?,
        ContentGenerationKind::OpenAiResponses
        | ContentGenerationKind::OpenAiResponsesWebSocket => {
            apply_responses_manual(root, target, index)?
        }
        _ => return Err("non-OpenAI target"),
    }

    if ttl == Some("30m") {
        set_prompt_cache_options(root, None, ttl)?;
    }
    Ok(())
}

fn apply_chat_magic(root: &mut Map<String, Value>, remaining: &mut usize) {
    let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) else {
        return;
    };
    for message in messages {
        let Some(message) = message.as_object_mut() else {
            continue;
        };
        let supported_string = !matches!(
            message.get("role").and_then(Value::as_str),
            Some("function") | None
        );
        let Some(content) = message.get_mut("content") else {
            continue;
        };
        if supported_string && let Some(text) = take_magic_string(content, remaining) {
            *content = Value::Array(vec![chat_text_part(text, true)]);
            continue;
        }
        if let Value::Array(parts) = content {
            apply_magic_to_parts(parts, PartFamily::Chat, remaining);
        }
    }
}

fn apply_responses_magic(root: &mut Map<String, Value>, remaining: &mut usize) {
    let instruction_triggered = root
        .get_mut("instructions")
        .and_then(|value| match value {
            Value::String(text) => Some(text),
            _ => None,
        })
        .is_some_and(claude_magic_cache::strip_magic_tokens);
    if instruction_triggered && *remaining > 0 && prepend_instruction_anchor(root) {
        *remaining -= 1;
    }

    if let Some(variables) = root
        .get_mut("prompt")
        .and_then(Value::as_object_mut)
        .and_then(|prompt| prompt.get_mut("variables"))
        .and_then(Value::as_object_mut)
    {
        for value in variables.values_mut() {
            if let Some(text) = take_magic_string(value, remaining) {
                *value = response_input_text_part(text, true);
                continue;
            }
            if let Value::Object(part) = value {
                apply_magic_to_part(part, PartFamily::Responses, remaining);
            }
        }
    }

    let Some(input) = root.get_mut("input") else {
        return;
    };
    if let Some(text) = take_magic_string(input, remaining) {
        *input = Value::Array(vec![response_message(text, true)]);
        return;
    }
    if let Value::Array(items) = input {
        for item in items {
            let Some(item) = item.as_object_mut() else {
                continue;
            };
            for field in ["content", "output"] {
                let Some(content) = item.get_mut(field) else {
                    continue;
                };
                if let Some(text) = take_magic_string(content, remaining) {
                    *content = Value::Array(vec![response_input_text_part(text, true)]);
                    continue;
                }
                if let Value::Array(parts) = content {
                    apply_magic_to_parts(parts, PartFamily::Responses, remaining);
                }
            }
        }
    }
}

fn take_magic_string(value: &mut Value, remaining: &mut usize) -> Option<String> {
    let Value::String(text) = value else {
        return None;
    };
    if !claude_magic_cache::strip_magic_tokens(text) || text.trim().is_empty() || *remaining == 0 {
        return None;
    }
    *remaining -= 1;
    Some(std::mem::take(text))
}

#[derive(Clone, Copy)]
enum PartFamily {
    Chat,
    Responses,
}

fn apply_magic_to_parts(parts: &mut [Value], family: PartFamily, remaining: &mut usize) {
    for part in parts {
        if let Some(part) = part.as_object_mut() {
            apply_magic_to_part(part, family, remaining);
        }
    }
}

fn apply_magic_to_part(part: &mut Map<String, Value>, family: PartFamily, remaining: &mut usize) {
    let text_key = match (family, part.get("type").and_then(Value::as_str)) {
        (PartFamily::Chat, Some("text")) => "text",
        (PartFamily::Chat, Some("refusal")) => "refusal",
        (PartFamily::Responses, Some("input_text")) => "text",
        _ => return,
    };
    let has_breakpoint = part.contains_key("prompt_cache_breakpoint");
    let Some(text) = part.get_mut(text_key).and_then(|value| match value {
        Value::String(text) => Some(text),
        _ => None,
    }) else {
        return;
    };
    if !claude_magic_cache::strip_magic_tokens(text)
        || text.trim().is_empty()
        || *remaining == 0
        || has_breakpoint
    {
        return;
    }
    part.insert("prompt_cache_breakpoint".into(), explicit_breakpoint());
    *remaining -= 1;
}

fn apply_chat_manual(
    root: &mut Map<String, Value>,
    target: &str,
    index: Option<i64>,
) -> Result<(), &'static str> {
    let messages = root
        .get("messages")
        .and_then(Value::as_array)
        .ok_or("missing messages array")?;
    let locations = match target {
        "system" => chat_system_locations(messages),
        "message" => chat_message_locations(messages),
        _ => return Err("unsupported cache breakpoint target"),
    };
    let selected = resolve_location(&locations, index)?;
    let messages = root
        .get_mut("messages")
        .and_then(Value::as_array_mut)
        .ok_or("missing messages array")?;
    stamp_chat_location(messages, selected)
}

fn chat_system_locations(messages: &[Value]) -> Vec<ChatLocation> {
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

fn chat_message_locations(messages: &[Value]) -> Vec<ChatLocation> {
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

fn stamp_chat_location(messages: &mut [Value], location: ChatLocation) -> Result<(), &'static str> {
    match location {
        ChatLocation::ContentString(message_index) => {
            let content = messages
                .get_mut(message_index)
                .and_then(|message| message.get_mut("content"))
                .ok_or("target content not found")?;
            let text = content
                .as_str()
                .ok_or("target content is not text")?
                .to_string();
            *content = Value::Array(vec![chat_text_part(text, true)]);
            Ok(())
        }
        ChatLocation::ContentPart(message_index, part_index) => {
            let part = messages
                .get_mut(message_index)
                .and_then(|message| message.get_mut("content"))
                .and_then(Value::as_array_mut)
                .and_then(|parts| parts.get_mut(part_index))
                .and_then(Value::as_object_mut)
                .ok_or("target content block not found")?;
            if !is_supported_chat_part(part) {
                return Err("target block does not support an OpenAI cache breakpoint");
            }
            part.entry("prompt_cache_breakpoint")
                .or_insert_with(explicit_breakpoint);
            Ok(())
        }
    }
}

fn apply_responses_manual(
    root: &mut Map<String, Value>,
    target: &str,
    index: Option<i64>,
) -> Result<(), &'static str> {
    let locations = match target {
        "system" => response_system_locations(root),
        "message" => response_message_locations(root),
        _ => return Err("unsupported cache breakpoint target"),
    };
    let selected = resolve_location(&locations, index)?;
    stamp_response_location(root, selected)
}

fn response_system_locations(root: &Map<String, Value>) -> Vec<ResponsesLocation> {
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

fn response_message_locations(root: &Map<String, Value>) -> Vec<ResponsesLocation> {
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

fn stamp_response_location(
    root: &mut Map<String, Value>,
    location: ResponsesLocation,
) -> Result<(), &'static str> {
    match location {
        ResponsesLocation::Instructions => prepend_instruction_anchor(root)
            .then_some(())
            .ok_or("unable to insert instructions cache anchor"),
        ResponsesLocation::InputString => {
            let text = root
                .get("input")
                .and_then(Value::as_str)
                .ok_or("input is not text")?
                .to_string();
            root.insert(
                "input".into(),
                Value::Array(vec![response_message(text, true)]),
            );
            Ok(())
        }
        ResponsesLocation::ContentString(item_index) => {
            let content = root
                .get_mut("input")
                .and_then(Value::as_array_mut)
                .and_then(|items| items.get_mut(item_index))
                .and_then(|message| message.get_mut("content"))
                .ok_or("target content not found")?;
            let text = content
                .as_str()
                .ok_or("target content is not text")?
                .to_string();
            *content = Value::Array(vec![response_input_text_part(text, true)]);
            Ok(())
        }
        ResponsesLocation::ContentPart(item_index, part_index) => {
            let part = root
                .get_mut("input")
                .and_then(Value::as_array_mut)
                .and_then(|items| items.get_mut(item_index))
                .and_then(|message| message.get_mut("content"))
                .and_then(Value::as_array_mut)
                .and_then(|parts| parts.get_mut(part_index))
                .and_then(Value::as_object_mut)
                .ok_or("target content block not found")?;
            if !is_supported_response_part(part) {
                return Err("target block does not support an OpenAI cache breakpoint");
            }
            part.entry("prompt_cache_breakpoint")
                .or_insert_with(explicit_breakpoint);
            Ok(())
        }
    }
}

fn resolve_location<T: Copy>(locations: &[T], index: Option<i64>) -> Result<T, &'static str> {
    let index =
        resolve_block_index(locations.len(), index).ok_or("index out of range or invalid")?;
    Ok(locations[index])
}

fn resolve_block_index(len: usize, index: Option<i64>) -> Option<usize> {
    if len == 0 {
        return None;
    }
    match index {
        None => Some(len - 1),
        Some(0) => None,
        Some(i) if i > 0 => {
            let nth = i as usize;
            (nth <= len).then(|| nth - 1)
        }
        Some(i) => {
            let from_end = i.unsigned_abs() as usize;
            (from_end <= len).then(|| len - from_end)
        }
    }
}

fn prepend_instruction_anchor(root: &mut Map<String, Value>) -> bool {
    let input = root.remove("input");
    let mut items = match input {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::String(text)) => vec![response_message(text, false)],
        Some(Value::Array(items)) => items,
        Some(other) => {
            root.insert("input".into(), other);
            return false;
        }
    };
    // `instructions` cannot carry content-part metadata. A whitespace developer
    // block immediately after it creates the same rendered-prefix boundary
    // while preserving top-level instruction semantics.
    items.insert(
        0,
        response_message_with_role("developer", " ".to_string(), true),
    );
    root.insert("input".into(), Value::Array(items));
    true
}

fn set_prompt_cache_options(
    root: &mut Map<String, Value>,
    mode: Option<&str>,
    ttl: Option<&str>,
) -> Result<(), &'static str> {
    let options = root
        .entry("prompt_cache_options")
        .or_insert_with(|| json!({}));
    let options = options
        .as_object_mut()
        .ok_or("prompt_cache_options is not an object")?;
    if let Some(mode) = mode {
        options.entry("mode").or_insert_with(|| json!(mode));
    }
    if ttl == Some("30m") {
        options.insert("ttl".into(), json!("30m"));
    }
    Ok(())
}

fn is_supported_chat_part(part: &Map<String, Value>) -> bool {
    matches!(
        part.get("type").and_then(Value::as_str),
        Some("text" | "image_url" | "input_audio" | "file" | "refusal")
    )
}

fn is_supported_response_part(part: &Map<String, Value>) -> bool {
    matches!(
        part.get("type").and_then(Value::as_str),
        Some("input_text" | "input_image" | "input_file")
    )
}

fn is_cacheable_chat_part(part: &Map<String, Value>) -> bool {
    if !is_supported_chat_part(part) {
        return false;
    }
    match part.get("type").and_then(Value::as_str) {
        Some("text") => part
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

fn is_cacheable_response_part(part: &Map<String, Value>) -> bool {
    if !is_supported_response_part(part) {
        return false;
    }
    match part.get("type").and_then(Value::as_str) {
        Some("input_text") => part
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.trim().is_empty()),
        _ => true,
    }
}

fn explicit_breakpoint() -> Value {
    json!({"mode": "explicit"})
}

fn chat_text_part(text: String, breakpoint: bool) -> Value {
    let mut part = json!({"type": "text", "text": text});
    if breakpoint {
        part["prompt_cache_breakpoint"] = explicit_breakpoint();
    }
    part
}

fn response_input_text_part(text: String, breakpoint: bool) -> Value {
    let mut part = json!({"type": "input_text", "text": text});
    if breakpoint {
        part["prompt_cache_breakpoint"] = explicit_breakpoint();
    }
    part
}

fn response_message(text: String, breakpoint: bool) -> Value {
    response_message_with_role("user", text, breakpoint)
}

fn response_message_with_role(role: &str, text: String, breakpoint: bool) -> Value {
    json!({
        "type": "message",
        "role": role,
        "content": [response_input_text_part(text, breakpoint)]
    })
}

#[cfg(test)]
fn count_breakpoints(value: &Value) -> usize {
    match value {
        Value::Array(values) => values.iter().map(count_breakpoints).sum(),
        Value::Object(object) => {
            usize::from(object.contains_key("prompt_cache_breakpoint"))
                + object.values().map(count_breakpoints).sum::<usize>()
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAGIC: &str = "GPROXY_MAGIC_STRING_TRIGGER_CACHING_CREATE_7D9ASD7A98SD7A9S8D79ASC98A7FNKJBVV80SCMSHDSIUCH";

    #[test]
    fn chat_magic_converts_string_content_and_stamps_breakpoint() {
        let mut body = json!({
            "model": "gpt-5.6",
            "messages": [{"role": "system", "content": format!("stable {MAGIC}")}]
        });
        apply_magic_string_cache_breakpoints(
            &mut body,
            ContentGenerationKind::OpenAiChatCompletions,
        );

        assert_eq!(body["messages"][0]["content"][0]["text"], "stable ");
        assert_eq!(
            body["messages"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
            "explicit"
        );
        serde_json::from_value::<crate::protocol::openai::generate_content::ChatCompletionRequest>(
            body,
        )
        .unwrap();
    }

    #[test]
    fn responses_instruction_magic_uses_prefix_anchor() {
        let mut body = json!({"instructions": format!("stable {MAGIC}"), "input": "hello"});
        apply_magic_string_cache_breakpoints(&mut body, ContentGenerationKind::OpenAiResponses);

        assert_eq!(body["instructions"], "stable ");
        assert_eq!(body["input"][0]["role"], "developer");
        assert_eq!(
            body["input"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
            "explicit"
        );
        assert_eq!(body["input"][1]["content"][0]["text"], "hello");
        serde_json::from_value::<crate::protocol::openai::generate_content::ResponseCreateRequest>(
            body,
        )
        .unwrap();
    }

    #[test]
    fn magic_caps_new_markers_without_counting_prior_turns() {
        let marked = |text: &str| {
            json!({
                "type": "input_text",
                "text": text,
                "prompt_cache_breakpoint": {"mode": "explicit"}
            })
        };
        let mut body = json!({"input": [{
            "role": "user",
            "content": [
                marked("a"), marked("b"), marked("c"),
                {"type": "input_text", "text": format!("d {MAGIC}")},
                {"type": "input_text", "text": format!("e {MAGIC}")}
            ]
        }]});
        apply_magic_string_cache_breakpoints(&mut body, ContentGenerationKind::OpenAiResponses);

        assert_eq!(count_breakpoints(&body), 5);
        assert!(!body.to_string().contains(MAGIC));
        assert!(body["input"][0]["content"][4]["prompt_cache_breakpoint"].is_object());
    }

    #[test]
    fn manual_message_flattens_supported_parts_across_chat_messages() {
        let mut body = json!({"messages": [
            {"role": "user", "content": [
                {"type": "text", "text": "first"},
                {"type": "custom", "value": "unsupported"}
            ]},
            {"role": "assistant", "content": [
                {"type": "text", "text": "second"},
                {"type": "text", "text": "   "}
            ]}
        ]});

        apply_manual_cache_breakpoint(
            &mut body,
            ContentGenerationKind::OpenAiChatCompletions,
            "message",
            Some(-2),
            None,
        )
        .unwrap();

        assert!(body["messages"][0]["content"][0]["prompt_cache_breakpoint"].is_object());
        assert!(
            body["messages"][0]["content"][1]
                .get("prompt_cache_breakpoint")
                .is_none()
        );
    }

    #[test]
    fn manual_responses_message_sets_request_ttl() {
        let mut body = json!({"input": [{"role": "user", "content": "hello"}]});
        apply_manual_cache_breakpoint(
            &mut body,
            ContentGenerationKind::OpenAiResponses,
            "message",
            None,
            Some("30m"),
        )
        .unwrap();

        assert_eq!(
            body["input"][0]["content"][0]["prompt_cache_breakpoint"]["mode"],
            "explicit"
        );
        assert_eq!(body["prompt_cache_options"]["ttl"], "30m");
    }
}
