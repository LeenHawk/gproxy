use serde_json::Value;

use super::schema::json_text_block;

pub(super) fn body(body: &mut Value) {
    let Some(root) = body.as_object_mut() else {
        return;
    };

    if let Some(system) = root.get_mut("system") {
        system_value(system);
    }

    if let Some(messages) = root.get_mut("messages").and_then(Value::as_array_mut) {
        for message in messages {
            message_value(message);
        }
    }
}

fn system_value(system: &mut Value) {
    match system {
        Value::String(text) => {
            let text = std::mem::take(text);
            *system = Value::Array(vec![json_text_block(text.as_str())]);
        }
        Value::Array(blocks) => blocks_slice(blocks),
        _ => {}
    }
}

fn message_value(message: &mut Value) {
    let Some(message_map) = message.as_object_mut() else {
        return;
    };
    let Some(content) = message_map.get_mut("content") else {
        return;
    };
    content_value(content);
}

fn content_value(content: &mut Value) {
    match content {
        Value::String(text) => {
            let text = std::mem::take(text);
            *content = Value::Array(vec![json_text_block(text.as_str())]);
        }
        Value::Object(_) => {
            let block = std::mem::take(content);
            *content = Value::Array(vec![block]);
        }
        Value::Array(blocks) => blocks_slice(blocks),
        _ => {}
    }
}

fn blocks_slice(blocks: &mut [Value]) {
    for block in blocks {
        if let Value::String(text) = block {
            let text = std::mem::take(text);
            *block = json_text_block(text.as_str());
        }
    }
}
