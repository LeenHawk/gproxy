use serde_json::{Map, Value, json};

pub(super) fn normalize(object: &mut Map<String, Value>) {
    let Some(Value::Array(tools)) = object.get("tools") else {
        return;
    };
    let mut changed = false;
    let mut output = Vec::new();
    for tool in tools {
        if kind(tool) == "namespace" {
            changed = true;
            if let Some(nested) = tool.get("tools").and_then(Value::as_array) {
                for tool in nested {
                    let (tool, normalized) = normalize_tool(tool);
                    changed |= normalized;
                    output.extend(tool);
                }
            }
        } else {
            let (tool, normalized) = normalize_tool(tool);
            changed |= normalized;
            output.extend(tool);
        }
    }
    if changed {
        if output.is_empty() {
            object.remove("tools");
        } else {
            object.insert("tools".into(), Value::Array(output));
        }
    }
    normalize_choice(object);
    if !matches!(object.get("tools"), Some(Value::Array(tools)) if !tools.is_empty()) {
        object.remove("tools");
        object.remove("tool_choice");
        object.remove("parallel_tool_calls");
    }
}

fn normalize_tool(tool: &Value) -> (Option<Value>, bool) {
    match kind(tool) {
        "tool_search" | "image_generation" => (None, true),
        "custom" if tool.get("name").and_then(Value::as_str) == Some("apply_patch") => (None, true),
        "custom" => {
            let mut tool = tool.clone();
            let object = tool.as_object_mut().expect("typed tool remains an object");
            object.insert("type".into(), Value::String("function".into()));
            object
                .entry("parameters")
                .or_insert_with(default_parameters);
            (Some(tool), true)
        }
        "web_search"
        | "web_search_2025_08_26"
        | "web_search_preview"
        | "web_search_preview_2025_03_11" => {
            let mut tool = tool.clone();
            let object = tool.as_object_mut().expect("typed tool remains an object");
            let changed = object.get("type").and_then(Value::as_str) != Some("web_search")
                || object.contains_key("external_web_access")
                || object.contains_key("search_context_size");
            object.insert("type".into(), Value::String("web_search".into()));
            object.remove("external_web_access");
            object.remove("search_context_size");
            (Some(tool), changed)
        }
        "function" => {
            let mut tool = tool.clone();
            let object = tool.as_object_mut().expect("typed tool remains an object");
            let changed = !object.contains_key("parameters");
            object
                .entry("parameters")
                .or_insert_with(default_parameters);
            (Some(tool), changed)
        }
        _ => (Some(tool.clone()), false),
    }
}

fn normalize_choice(object: &mut Map<String, Value>) {
    let Some(choice) = object.get("tool_choice").and_then(Value::as_object) else {
        return;
    };
    let search = match choice.get("type").and_then(Value::as_str) {
        Some(
            "web_search"
            | "web_search_2025_08_26"
            | "web_search_preview"
            | "web_search_preview_2025_03_11"
            | "x_search",
        ) => true,
        Some("allowed_tools") => choice
            .get("tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| tools.iter().any(|tool| search_kind(kind(tool)))),
        _ => false,
    };
    if search {
        object.remove("tool_choice");
    }
}

fn search_kind(kind: &str) -> bool {
    matches!(
        kind,
        "web_search"
            | "web_search_2025_08_26"
            | "web_search_preview"
            | "web_search_preview_2025_03_11"
            | "x_search"
    )
}

fn kind(tool: &Value) -> &str {
    tool.get("type").and_then(Value::as_str).unwrap_or_default()
}

fn default_parameters() -> Value {
    json!({"type":"object","properties":{}})
}
