use serde_json::{Map, Value, json};

pub(crate) fn sanitize(body: &mut Value) {
    canonicalize(body);
    let Some(root) = body.as_object_mut() else {
        return;
    };
    if let Some(Value::Array(blocks)) = root.remove("system") {
        let blocks = clean_blocks(blocks, &mut [], Scope::System);
        if !blocks.is_empty() {
            root.insert("system".into(), Value::Array(blocks));
        }
    }
    let Some(Value::Array(messages)) = root.remove("messages") else {
        return;
    };
    let mut kept = Vec::with_capacity(messages.len());
    for mut message in messages {
        let Some(map) = message.as_object_mut() else {
            kept.push(message);
            continue;
        };
        let role = map.get("role").and_then(Value::as_str).map(str::to_owned);
        let Some(Value::Array(blocks)) = map.remove("content") else {
            kept.push(message);
            continue;
        };
        let blocks = clean_blocks(blocks, &mut kept, Scope::Message(role.as_deref()));
        if !blocks.is_empty() {
            map.insert("content".into(), Value::Array(blocks));
            kept.push(message);
        }
    }
    root.insert("messages".into(), Value::Array(kept));
}

#[derive(Clone, Copy)]
enum Scope<'a> {
    System,
    Message(Option<&'a str>),
}

fn clean_blocks(
    blocks: Vec<Value>,
    previous_messages: &mut [Value],
    scope: Scope<'_>,
) -> Vec<Value> {
    let mut kept = Vec::with_capacity(blocks.len());
    for block in blocks {
        let Value::Object(mut map) = block else {
            kept.push(block);
            continue;
        };
        let kind = map.get("type").and_then(Value::as_str);
        if matches!(kind, Some("thinking" | "redacted_thinking"))
            && !matches!(scope, Scope::Message(Some("assistant")))
        {
            continue;
        }
        if kind == Some("text") {
            let trimmed = map
                .get("text")
                .and_then(Value::as_str)
                .map(str::trim)
                .map(str::to_owned);
            if let Some(text) = trimmed {
                if text.is_empty() {
                    if let Some(control) = map.remove("cache_control")
                        && !attach(&mut kept, &control, scope)
                    {
                        attach_to_messages(previous_messages, &control);
                    }
                    continue;
                }
                map.insert("text".into(), Value::String(text));
            }
        }
        kept.push(Value::Object(map));
    }
    kept
}

fn attach(blocks: &mut [Value], control: &Value, scope: Scope<'_>) -> bool {
    for block in blocks.iter_mut().rev() {
        let Some(map) = block.as_object_mut() else {
            continue;
        };
        let cacheable = match scope {
            Scope::System => cacheable(map),
            Scope::Message(role) => message_cacheable(role, map),
        };
        if cacheable {
            map.entry("cache_control")
                .or_insert_with(|| control.clone());
            return true;
        }
    }
    false
}

fn attach_to_messages(messages: &mut [Value], control: &Value) {
    for message in messages.iter_mut().rev() {
        let Some(map) = message.as_object_mut() else {
            continue;
        };
        let role = map.get("role").and_then(Value::as_str).map(str::to_owned);
        let Some(blocks) = map.get_mut("content").and_then(Value::as_array_mut) else {
            continue;
        };
        if attach(blocks, control, Scope::Message(role.as_deref())) {
            return;
        }
    }
}

fn cacheable(block: &Map<String, Value>) -> bool {
    match block.get("type").and_then(Value::as_str) {
        Some("thinking" | "redacted_thinking" | "citation" | "citations") => false,
        Some("char_location" | "page_location" | "content_block_location") => false,
        Some("text") => block
            .get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.trim().is_empty()),
        _ => true,
    }
}

fn message_cacheable(role: Option<&str>, block: &Map<String, Value>) -> bool {
    cacheable(block)
        && (!matches!(
            block.get("type").and_then(Value::as_str),
            Some("image" | "document")
        ) || role == Some("user"))
}

fn canonicalize(body: &mut Value) {
    let Some(root) = body.as_object_mut() else {
        return;
    };
    if let Some(system) = root.get_mut("system") {
        content(system);
    }
    if let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            if let Some(content_value) = message
                .as_object_mut()
                .and_then(|message| message.get_mut("content"))
            {
                content(content_value);
            }
        }
    }
}

fn content(value: &mut Value) {
    match value {
        Value::String(text) => {
            let text = std::mem::take(text);
            *value = Value::Array(vec![json!({"type": "text", "text": text})]);
        }
        Value::Object(_) => {
            let block = std::mem::take(value);
            *value = Value::Array(vec![block]);
        }
        Value::Array(blocks) => {
            for block in blocks {
                if let Value::String(text) = block {
                    let text = std::mem::take(text);
                    *block = json!({"type": "text", "text": text});
                }
            }
        }
        _ => {}
    }
}
