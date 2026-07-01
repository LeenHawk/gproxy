use serde_json::Value;

use crate::protocol::{claude, gemini};

pub(super) fn claude_tools_to_gemini(tools: Vec<claude::Tool>) -> Vec<gemini::Tool> {
    let declarations = tools
        .into_iter()
        .filter_map(|tool| match tool {
            claude::Tool::Custom(tool) => Some(gemini::FunctionDeclaration {
                name: tool.name,
                description: tool.description.unwrap_or_default(),
                behavior: None,
                parameters: None,
                parameters_json_schema: Some(claude_schema_to_value(tool.input_schema)),
                response: None,
                response_json_schema: None,
                extra: Default::default(),
            }),
            claude::Tool::WebSearch(_) => None,
            _ => None,
        })
        .collect::<Vec<_>>();

    if declarations.is_empty() {
        Vec::new()
    } else {
        vec![gemini::Tool {
            function_declarations: declarations,
            ..Default::default()
        }]
    }
}

pub(super) fn claude_tool_choice_to_gemini(
    choice: Option<claude::ToolChoice>,
) -> Option<gemini::ToolConfig> {
    let (mode, allowed_function_names) = match choice? {
        claude::ToolChoice::Auto(_) => (
            gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Auto),
            Vec::new(),
        ),
        claude::ToolChoice::Any(_) => (
            gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Any),
            Vec::new(),
        ),
        claude::ToolChoice::None(_) => (
            gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::None),
            Vec::new(),
        ),
        claude::ToolChoice::Tool(choice) => (
            gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Any),
            vec![choice.name],
        ),
        claude::ToolChoice::Unknown(_) => return None,
    };

    Some(gemini::ToolConfig {
        function_calling_config: Some(gemini::FunctionCallingConfig {
            mode: Some(mode),
            allowed_function_names,
            extra: Default::default(),
        }),
        retrieval_config: None,
        include_server_side_tool_invocations: None,
        extra: Default::default(),
    })
}

fn claude_schema_to_value(schema: claude::JsonSchema) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("type".to_owned(), Value::String("object".to_owned()));
    if !schema.properties.is_empty() {
        object.insert(
            "properties".to_owned(),
            Value::Object(schema.properties.into_iter().collect()),
        );
    }
    if !schema.required.is_empty() {
        object.insert(
            "required".to_owned(),
            Value::Array(schema.required.into_iter().map(Value::String).collect()),
        );
    }
    Value::Object(object)
}
