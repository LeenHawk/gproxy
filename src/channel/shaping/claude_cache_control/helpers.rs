use serde_json::{Map, Value};

pub(super) fn existing_cache_breakpoint_count(root: &Map<String, Value>) -> usize {
    let mut count = usize::from(root.contains_key("cache_control"));
    for field in ["tools", "system"] {
        if let Some(blocks) = root.get(field).and_then(Value::as_array) {
            count += blocks
                .iter()
                .filter(|block| block.get("cache_control").is_some())
                .count();
        }
    }
    if let Some(messages) = root.get("messages").and_then(Value::as_array) {
        for message in messages {
            if let Some(blocks) = message.get("content").and_then(Value::as_array) {
                count += blocks
                    .iter()
                    .filter(|block| block.get("cache_control").is_some())
                    .count();
            }
        }
    }
    count
}
