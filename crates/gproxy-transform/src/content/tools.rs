use serde_json::{Value, json};

use crate::TransformError;

pub(super) fn chat_tools_to_claude(value: Option<Value>) -> Result<Option<Value>, TransformError> {
    map_tools(value, |tool| {
        let mut object = tool
            .as_object()
            .cloned()
            .ok_or_else(|| TransformError::shape("OpenAI Chat tools", "tool must be an object"))?;
        match object.get("type").and_then(Value::as_str) {
            Some("function") => {
                let mut function = object
                    .remove("function")
                    .and_then(|value| value.as_object().cloned())
                    .ok_or_else(|| {
                        TransformError::shape("OpenAI Chat tools", "function is missing")
                    })?;
                function.insert("type".into(), Value::String("custom".into()));
                if let Some(parameters) = function.remove("parameters") {
                    function.insert("input_schema".into(), parameters);
                }
                Ok(Value::Object(function))
            }
            Some(other) => Err(TransformError::unsupported("OpenAI Chat tool", other)),
            None => Err(TransformError::shape("OpenAI Chat tool", "type is missing")),
        }
    })
}

pub(super) fn claude_tools_to_chat(value: Option<Value>) -> Result<Option<Value>, TransformError> {
    map_tools(value, |tool| {
        let mut object = tool
            .as_object()
            .cloned()
            .ok_or_else(|| TransformError::shape("Claude tools", "tool must be an object"))?;
        if !matches!(
            object.get("type").and_then(Value::as_str),
            None | Some("custom")
        ) {
            return Err(TransformError::unsupported(
                "Claude tool",
                object["type"].to_string(),
            ));
        }
        object.remove("type");
        if let Some(schema) = object.remove("input_schema") {
            object.insert("parameters".into(), schema);
        }
        Ok(json!({"type":"function","function":object}))
    })
}

pub(super) fn responses_tools_to_claude(
    value: Option<Value>,
) -> Result<Option<Value>, TransformError> {
    map_tools(value, |tool| {
        let mut object = tool.as_object().cloned().ok_or_else(|| {
            TransformError::shape("OpenAI Responses tools", "tool must be an object")
        })?;
        match object.get("type").and_then(Value::as_str) {
            Some("function" | "custom") => {
                object.insert("type".into(), Value::String("custom".into()));
                if let Some(parameters) = object.remove("parameters") {
                    object.insert("input_schema".into(), parameters);
                }
                object
                    .entry("input_schema")
                    .or_insert_with(|| json!({"type":"object","properties":{}}));
                object.remove("format");
                Ok(Value::Object(object))
            }
            Some(other) if other.starts_with("web_search_") => Ok(Value::Object(object)),
            Some(other) => Err(TransformError::unsupported("OpenAI Responses tool", other)),
            None => Err(TransformError::shape(
                "OpenAI Responses tool",
                "type is missing",
            )),
        }
    })
}

pub(super) fn claude_tools_to_responses(
    value: Option<Value>,
) -> Result<Option<Value>, TransformError> {
    map_tools(value, |tool| {
        let mut object = tool
            .as_object()
            .cloned()
            .ok_or_else(|| TransformError::shape("Claude tools", "tool must be an object"))?;
        match object.get("type").and_then(Value::as_str) {
            None | Some("custom") => {
                object.insert("type".into(), Value::String("function".into()));
                if let Some(schema) = object.remove("input_schema") {
                    object.insert("parameters".into(), schema);
                }
                Ok(Value::Object(object))
            }
            Some(other) if other.starts_with("web_search_") => Ok(Value::Object(object)),
            Some(other) => Err(TransformError::unsupported("Claude tool", other)),
        }
    })
}

pub(super) fn chat_choice_to_claude(value: Option<Value>, parallel: Option<bool>) -> Option<Value> {
    let disable = parallel.map(|parallel| !parallel);
    match value {
        Some(Value::String(mode)) => match mode.as_str() {
            "auto" => Some(json!({"type":"auto","disable_parallel_tool_use":disable})),
            "required" => Some(json!({"type":"any","disable_parallel_tool_use":disable})),
            "none" => Some(json!({"type":"none"})),
            _ => None,
        },
        Some(Value::Object(choice)) => choice
            .get("function")
            .and_then(|function| function.get("name"))
            .cloned()
            .map(|name| json!({"type":"tool","name":name,"disable_parallel_tool_use":disable})),
        _ => None,
    }
}

pub(super) fn claude_choice_to_openai(value: Option<Value>) -> Option<Value> {
    let choice = value?.as_object()?.clone();
    match choice.get("type").and_then(Value::as_str) {
        Some("auto") => Some(Value::String("auto".into())),
        Some("any") => Some(Value::String("required".into())),
        Some("none") => Some(Value::String("none".into())),
        Some("tool") => Some(json!({
            "type":"function",
            "function":{"name":choice.get("name").cloned().unwrap_or(Value::Null)}
        })),
        _ => None,
    }
}

fn map_tools(
    value: Option<Value>,
    mut convert: impl FnMut(Value) -> Result<Value, TransformError>,
) -> Result<Option<Value>, TransformError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let tools = value
        .as_array()
        .ok_or_else(|| TransformError::shape("tools", "tools must be an array"))?
        .iter()
        .cloned()
        .map(&mut convert)
        .collect::<Result<Vec<_>, _>>()?;
    Ok((!tools.is_empty()).then_some(Value::Array(tools)))
}
