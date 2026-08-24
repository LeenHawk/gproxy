use serde_json::{Map, Value};

pub(super) fn normalize(root: &mut Map<String, Value>) {
    if let Some(Value::Array(tools)) = root.remove("tools") {
        let tools = tools
            .into_iter()
            .filter_map(function_tool)
            .collect::<Vec<_>>();
        if !tools.is_empty() {
            root.insert("tools".into(), Value::Array(tools));
        }
    }
    if let Some(choice) = root.remove("tool_choice")
        && let Some(mut choice) = tool_choice(choice)
    {
        let has_tools = root
            .get("tools")
            .and_then(Value::as_array)
            .is_some_and(|tools| !tools.is_empty());
        if !has_tools && choice.as_str() != Some("none") {
            choice = Value::String("none".into());
        }
        root.insert("tool_choice".into(), choice);
    }
}

fn function_tool(value: Value) -> Option<Value> {
    let mut tool = value.as_object()?.clone();
    if tool.remove("type")?.as_str()? != "function" {
        return None;
    }
    let function = tool.remove("function")?.as_object()?.clone();
    Some(serde_json::json!({"type":"function", "function":function}))
}

fn tool_choice(value: Value) -> Option<Value> {
    match value {
        Value::String(mode) if matches!(mode.as_str(), "none" | "auto" | "required") => {
            Some(Value::String(mode))
        }
        Value::Object(mut choice) => {
            if choice.remove("type")?.as_str()? != "function" {
                return None;
            }
            let name = choice.remove("function")?.get("name")?.as_str()?.to_owned();
            Some(serde_json::json!({
                "type":"function", "function":{"name":name}
            }))
        }
        _ => None,
    }
}
