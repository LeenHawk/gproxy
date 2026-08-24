use base64::Engine as _;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use serde_json::{Map, Value};

pub(super) fn sanitize(object: &mut Map<String, Value>) {
    sanitize_input(object);
    remove_encrypted_include(object);
    strip_unsupported_effort(object);
}

fn sanitize_input(object: &mut Map<String, Value>) {
    let Some(Value::Array(input)) = object.get_mut("input") else {
        return;
    };
    let mut output = Vec::with_capacity(input.len());
    for mut item in std::mem::take(input) {
        let item_kind = kind(&item).to_owned();
        if !matches!(item_kind.as_str(), "reasoning" | "compaction") {
            output.push(item);
            continue;
        }
        let Some(fields) = item.as_object_mut() else {
            output.push(item);
            continue;
        };
        if fields.get("content").is_some_and(Value::is_null) {
            fields.remove("content");
        }
        let invalid = fields
            .get("encrypted_content")
            .is_some_and(|value| !valid_encrypted(value));
        if invalid && item_kind == "compaction" {
            continue;
        }
        if invalid {
            fields.remove("encrypted_content");
        }
        output.push(item);
    }
    *input = merge_summaries(output);
}

fn merge_summaries(items: Vec<Value>) -> Vec<Value> {
    let mut output: Vec<Value> = Vec::with_capacity(items.len());
    for item in items {
        if let Some(previous) = output.last_mut()
            && mergeable(previous, &item)
            && let Some(summary) = item.get("summary").and_then(Value::as_array)
            && let Some(previous) = previous.get_mut("summary").and_then(Value::as_array_mut)
        {
            previous.extend(summary.iter().cloned());
        } else {
            output.push(item);
        }
    }
    output
}

fn mergeable(previous: &Value, current: &Value) -> bool {
    kind(previous) == "reasoning"
        && kind(current) == "reasoning"
        && previous.get("summary").is_some_and(Value::is_array)
        && matches!(current.get("summary"), Some(Value::Array(items)) if !items.is_empty())
        && current.as_object().is_some_and(|object| {
            object
                .keys()
                .all(|key| matches!(key.as_str(), "type" | "summary"))
        })
}

fn remove_encrypted_include(object: &mut Map<String, Value>) {
    let Some(include) = object.get("include").and_then(Value::as_array) else {
        return;
    };
    let kept = include
        .iter()
        .filter(|value| value.as_str() != Some("reasoning.encrypted_content"))
        .cloned()
        .collect::<Vec<_>>();
    if kept.is_empty() {
        object.remove("include");
    } else if kept.len() != include.len() {
        object.insert("include".into(), Value::Array(kept));
    }
}

fn strip_unsupported_effort(object: &mut Map<String, Value>) {
    let model = object
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if supports_effort(model) {
        return;
    }
    let Some(reasoning) = object.get_mut("reasoning").and_then(Value::as_object_mut) else {
        return;
    };
    reasoning.remove("effort");
    if reasoning.is_empty() {
        object.remove("reasoning");
    }
}

fn supports_effort(model: &str) -> bool {
    let name = model
        .trim()
        .rsplit_once('/')
        .map_or(model.trim(), |(_, name)| name)
        .to_ascii_lowercase();
    name.starts_with("grok-3-mini")
        || name.starts_with("grok-4.20-multi-agent")
        || name.starts_with("grok-4.3")
}

fn valid_encrypted(value: &Value) -> bool {
    let Some(raw) = value.as_str() else {
        return false;
    };
    if raw.is_empty() || raw.trim() != raw || raw.starts_with("gAAAA") || raw.contains('=') {
        return false;
    }
    if !raw
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/'))
    {
        return false;
    }
    STANDARD_NO_PAD
        .decode(raw)
        .is_ok_and(|decoded| decoded.len() >= 50)
}

fn kind(value: &Value) -> &str {
    value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
}
