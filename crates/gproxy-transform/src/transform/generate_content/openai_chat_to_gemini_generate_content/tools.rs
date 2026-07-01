use serde_json::Value;

use crate::protocol::{gemini, openai};

pub(super) fn chat_tools_to_gemini(tools: Vec<openai::ChatTool>) -> Vec<gemini::Tool> {
    let declarations = tools
        .into_iter()
        .map(|tool| match tool {
            openai::ChatTool::Function { function, .. } => gemini::FunctionDeclaration {
                name: function.name,
                description: function.description.unwrap_or_default(),
                behavior: None,
                parameters: None,
                parameters_json_schema: function.parameters.map(json_schema_value),
                response: None,
                response_json_schema: None,
                extra: Default::default(),
            },
            openai::ChatTool::Custom { custom, .. } => gemini::FunctionDeclaration {
                name: custom.name,
                description: custom.description.unwrap_or_default(),
                behavior: None,
                parameters: None,
                parameters_json_schema: None,
                response: None,
                response_json_schema: None,
                extra: Default::default(),
            },
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

pub(super) fn chat_tool_choice_to_gemini(
    choice: Option<openai::ChatToolChoice>,
) -> Option<gemini::ToolConfig> {
    let (mode, allowed_function_names) = match choice? {
        openai::ChatToolChoice::Mode(openai::ToolChoiceMode::Auto) => (
            gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Auto),
            Vec::new(),
        ),
        openai::ChatToolChoice::Mode(openai::ToolChoiceMode::Required) => (
            gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Any),
            Vec::new(),
        ),
        openai::ChatToolChoice::Mode(openai::ToolChoiceMode::None) => (
            gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::None),
            Vec::new(),
        ),
        openai::ChatToolChoice::Allowed(_) => return None,
        openai::ChatToolChoice::Named(named) => {
            let name = match named {
                openai::ChatNamedToolChoice::Function { function, .. } => function.name,
                openai::ChatNamedToolChoice::Custom { custom, .. } => custom.name,
            };
            (
                gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Any),
                vec![name],
            )
        }
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

fn json_schema_value(schema: openai::JsonSchema) -> Value {
    serde_json::to_value(schema).unwrap_or(Value::Object(Default::default()))
}
