use serde_json::Value;

use crate::protocol::{claude, gemini};

pub(super) fn gemini_tools_to_claude(tools: Vec<gemini::Tool>) -> Vec<claude::Tool> {
    tools
        .into_iter()
        .flat_map(|tool| tool.function_declarations)
        .map(|declaration| {
            claude::Tool::Custom(claude::CustomTool {
                input_schema: declaration
                    .parameters_json_schema
                    .and_then(value_to_claude_schema)
                    .or_else(|| declaration.parameters.map(schema_to_claude_schema))
                    .unwrap_or_else(empty_schema),
                name: declaration.name,
                type_: Some(claude::CustomToolType::Custom),
                description: (!declaration.description.is_empty())
                    .then_some(declaration.description),
                eager_input_streaming: None,
                common: Default::default(),
            })
        })
        .collect()
}

pub(super) fn gemini_tool_config_to_claude(
    config: Option<gemini::ToolConfig>,
) -> Option<claude::ToolChoice> {
    let config = config?.function_calling_config?;
    match config.mode? {
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Auto)
        | gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::ModeUnspecified) => {
            Some(claude::ToolChoice::Auto(claude::ToolChoiceAuto {
                type_: claude::ToolChoiceAutoType::Auto,
                disable_parallel_tool_use: None,
                extra: Default::default(),
            }))
        }
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Any)
        | gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Validated) => {
            if let Some(name) = config.allowed_function_names.into_iter().next() {
                Some(claude::ToolChoice::Tool(claude::ToolChoiceTool {
                    name,
                    type_: claude::ToolChoiceToolType::Tool,
                    disable_parallel_tool_use: None,
                    extra: Default::default(),
                }))
            } else {
                Some(claude::ToolChoice::Any(claude::ToolChoiceAny {
                    type_: claude::ToolChoiceAnyType::Any,
                    disable_parallel_tool_use: None,
                    extra: Default::default(),
                }))
            }
        }
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::None) => {
            Some(claude::ToolChoice::None(claude::ToolChoiceNone {
                type_: claude::ToolChoiceNoneType::None,
                extra: Default::default(),
            }))
        }
        gemini::FunctionCallingMode::Unknown(_) => None,
    }
}

fn schema_to_claude_schema(schema: gemini::Schema) -> claude::JsonSchema {
    value_to_claude_schema(
        serde_json::to_value(schema).unwrap_or(Value::Object(Default::default())),
    )
    .unwrap_or_else(empty_schema)
}

fn value_to_claude_schema(mut value: Value) -> Option<claude::JsonSchema> {
    normalize_schema_types(&mut value);
    let Value::Object(mut map) = value else {
        return None;
    };
    let properties = map
        .remove("properties")
        .and_then(|value| match value {
            Value::Object(map) => Some(map.into_iter().collect()),
            _ => None,
        })
        .unwrap_or_default();
    let required = map
        .remove("required")
        .and_then(|value| match value {
            Value::Array(values) => Some(
                values
                    .into_iter()
                    .filter_map(|value| value.as_str().map(str::to_owned))
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default();
    map.remove("type");
    Some(claude::JsonSchema {
        type_: claude::JsonSchemaObjectType::Known(claude::JsonSchemaObjectTypeKnown::Object),
        properties,
        required,
        extra: Default::default(),
    })
}

fn normalize_schema_types(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(kind) = map.get_mut("type") {
                normalize_type(kind);
            }
            map.values_mut().for_each(normalize_schema_types);
        }
        Value::Array(values) => values.iter_mut().for_each(normalize_schema_types),
        _ => {}
    }
}

fn normalize_type(value: &mut Value) {
    match value {
        Value::String(kind) => kind.make_ascii_lowercase(),
        Value::Array(kinds) => kinds.iter_mut().for_each(normalize_type),
        _ => {}
    }
}

fn empty_schema() -> claude::JsonSchema {
    claude::JsonSchema {
        type_: claude::JsonSchemaObjectType::Known(claude::JsonSchemaObjectTypeKnown::Object),
        properties: Default::default(),
        required: Vec::new(),
        extra: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn normalizes_nested_gemini_schema_types() {
        let schema = value_to_claude_schema(json!({
            "type": "OBJECT",
            "properties": {
                "city": { "type": "STRING" },
                "days": { "type": ["INTEGER", "NULL"] }
            }
        }))
        .unwrap();

        assert_eq!(schema.properties["city"]["type"], "string");
        assert_eq!(
            schema.properties["days"]["type"],
            json!(["integer", "null"])
        );
    }
}
