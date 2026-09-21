//! Tool declarations the Grok Build proxy accepts (v3
//! `shape/responses/tools.rs`).
//!
//! It has no namespace grouping, no hosted tool search, no image-generation
//! tool and no `apply_patch` custom tool; a custom tool is a function with a
//! schema, and web search is one type with no options. A tool choice that
//! names a search tool is dropped along with the tool itself, and a request
//! left with no tools drops the fields that only make sense beside them.

use serde_json::{Map, Value, json};

/// Every spelling of the hosted web-search tool the proxy folds into one.
const WEB_SEARCH: &[&str] = &[
    "web_search",
    "web_search_2025_08_26",
    "web_search_preview",
    "web_search_preview_2025_03_11",
];

pub(super) fn normalize(object: &mut Map<String, Value>) {
    if let Some(Value::Array(tools)) = object.get(&"tools".to_owned()) {
        let mut changed = false;
        let mut output = Vec::new();
        for tool in tools {
            if kind(tool) == "namespace" {
                changed = true;
                for nested in tool
                    .get("tools")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let (tool, normalized) = normalize_tool(nested);
                    changed |= normalized;
                    output.extend(tool);
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
    }
    normalize_choice(object);
    if !matches!(object.get("tools"), Some(Value::Array(tools)) if !tools.is_empty()) {
        object.remove("tools");
        object.remove("tool_choice");
        object.remove("parallel_tool_calls");
    }
}

/// The tool as the proxy takes it, and whether anything about it moved.
fn normalize_tool(tool: &Value) -> (Option<Value>, bool) {
    match kind(tool) {
        "tool_search" | "image_generation" => (None, true),
        "custom" if tool.get("name").and_then(Value::as_str) == Some("apply_patch") => (None, true),
        "custom" => {
            let mut tool = tool.clone();
            let Some(object) = tool.as_object_mut() else {
                return (Some(tool), false);
            };
            object.insert("type".into(), Value::String("function".into()));
            object
                .entry("parameters")
                .or_insert_with(default_parameters);
            (Some(tool), true)
        }
        name if WEB_SEARCH.contains(&name) => {
            let mut tool = tool.clone();
            let Some(object) = tool.as_object_mut() else {
                return (Some(tool), false);
            };
            let changed = name != "web_search"
                || object.contains_key("external_web_access")
                || object.contains_key("search_context_size");
            object.insert("type".into(), Value::String("web_search".into()));
            object.remove("external_web_access");
            object.remove("search_context_size");
            (Some(tool), changed)
        }
        "function" => {
            let mut tool = tool.clone();
            let Some(object) = tool.as_object_mut() else {
                return (Some(tool), false);
            };
            let changed = !object.contains_key("parameters");
            object
                .entry("parameters")
                .or_insert_with(default_parameters);
            (Some(tool), changed)
        }
        _ => (Some(tool.clone()), false),
    }
}

/// A choice that insists on a search tool has nothing to insist on.
fn normalize_choice(object: &mut Map<String, Value>) {
    let Some(choice) = object.get("tool_choice").and_then(Value::as_object) else {
        return;
    };
    let search = match choice.get("type").and_then(Value::as_str) {
        Some("x_search") => true,
        Some(name) if WEB_SEARCH.contains(&name) => true,
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
    kind == "x_search" || WEB_SEARCH.contains(&kind)
}

fn kind(tool: &Value) -> &str {
    tool.get("type").and_then(Value::as_str).unwrap_or_default()
}

fn default_parameters() -> Value {
    json!({"type": "object", "properties": {}})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalized(value: Value) -> Value {
        let mut object = value.as_object().cloned().unwrap();
        normalize(&mut object);
        Value::Object(object)
    }

    #[test]
    fn a_namespace_flattens_and_a_hosted_tool_disappears() {
        let out = normalized(json!({"tools": [
            {"type": "namespace", "tools": [{"type": "function", "name": "f"}]},
            {"type": "tool_search"},
            {"type": "custom", "name": "apply_patch"},
            {"type": "custom", "name": "shell"},
            {"type": "web_search_preview", "search_context_size": "high"},
        ]}));
        let tools = out["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0]["name"], "f");
        assert_eq!(
            tools[0]["parameters"],
            json!({"type": "object", "properties": {}})
        );
        assert_eq!(tools[1]["type"], "function", "a custom tool is a function");
        assert_eq!(tools[2]["type"], "web_search");
        assert!(tools[2].get("search_context_size").is_none());
    }

    #[test]
    fn nothing_left_to_choose_from_takes_the_choice_with_it() {
        let out = normalized(json!({
            "tools": [{"type": "image_generation"}],
            "tool_choice": "auto", "parallel_tool_calls": true,
        }));
        assert!(out.get("tools").is_none());
        assert!(out.get("tool_choice").is_none());
        assert!(out.get("parallel_tool_calls").is_none());

        let out = normalized(json!({
            "tools": [{"type": "function", "name": "f", "parameters": {}}],
            "tool_choice": {"type": "x_search"},
        }));
        assert_eq!(out["tools"].as_array().unwrap().len(), 1);
        assert!(out.get("tool_choice").is_none(), "a search choice is gone");
    }
}
