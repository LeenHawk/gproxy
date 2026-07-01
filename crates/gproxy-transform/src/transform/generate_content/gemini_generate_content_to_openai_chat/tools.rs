use serde_json::Value;

use crate::protocol::{gemini, openai};

pub(super) fn gemini_tools_to_chat(tools: Vec<gemini::Tool>) -> Vec<openai::ChatTool> {
    tools
        .into_iter()
        .flat_map(|tool| tool.function_declarations)
        .map(|declaration| openai::ChatTool::Function {
            function: openai::FunctionDefinition {
                name: declaration.name,
                description: (!declaration.description.is_empty())
                    .then_some(declaration.description),
                parameters: declaration
                    .parameters_json_schema
                    .and_then(value_to_json_schema)
                    .or_else(|| declaration.parameters.map(schema_to_json_schema)),
                strict: None,
                extra: Default::default(),
            },
            extra: Default::default(),
        })
        .collect()
}

pub(super) fn gemini_tool_config_to_chat(
    config: Option<gemini::ToolConfig>,
) -> Option<openai::ChatToolChoice> {
    let config = config?.function_calling_config?;
    match config.mode? {
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Auto)
        | gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::ModeUnspecified) => {
            Some(openai::ChatToolChoice::Mode(openai::ToolChoiceMode::Auto))
        }
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Any)
        | gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Validated) => {
            if let Some(name) = config.allowed_function_names.into_iter().next() {
                Some(openai::ChatToolChoice::Named(
                    openai::ChatNamedToolChoice::Function {
                        type_: openai::FunctionToolChoiceType::Function,
                        function: openai::NamedTool {
                            name,
                            extra: Default::default(),
                        },
                        extra: Default::default(),
                    },
                ))
            } else {
                Some(openai::ChatToolChoice::Mode(
                    openai::ToolChoiceMode::Required,
                ))
            }
        }
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::None) => {
            Some(openai::ChatToolChoice::Mode(openai::ToolChoiceMode::None))
        }
        gemini::FunctionCallingMode::Unknown(_) => None,
    }
}

fn schema_to_json_schema(schema: gemini::Schema) -> openai::JsonSchema {
    value_to_json_schema(serde_json::to_value(schema).unwrap_or(Value::Object(Default::default())))
        .unwrap_or_default()
}

fn value_to_json_schema(value: Value) -> Option<openai::JsonSchema> {
    match value {
        Value::Object(map) => Some(map.into_iter().collect()),
        _ => None,
    }
}
