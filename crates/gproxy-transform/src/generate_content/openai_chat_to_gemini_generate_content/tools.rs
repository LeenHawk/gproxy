use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::generate_content::gemini_generate_content_to_openai_chat::tools::CODE_EXECUTION_NAME;

pub(crate) fn transform(
    tools: Option<Vec<openai::ChatTool>>,
    web_search: bool,
) -> Result<Option<Vec<gemini::Tool>>, TransformError> {
    let mut declarations = Vec::new();
    let mut output = Vec::new();
    let mut code_execution = false;
    for tool in tools.into_iter().flatten() {
        match tool {
            openai::ChatTool::Function(tool) if tool.function.name == CODE_EXECUTION_NAME => {
                code_execution = true;
            }
            openai::ChatTool::Function(tool) => {
                declarations.push(gemini::FunctionDeclaration {
                    name: tool.function.name,
                    description: tool.function.description.ok_or_else(|| {
                        TransformError::shape("Chat function tool", "description is missing")
                    })?,
                    behavior: None,
                    parameters: None,
                    parameters_json_schema: tool.function.parameters.map(serde_json::Value::Object),
                    response: None,
                    response_json_schema: None,
                    rest: merge(tool.rest, tool.function.rest),
                });
            }
            openai::ChatTool::Custom(tool) => {
                declarations.push(gemini::FunctionDeclaration {
                    name: tool.custom.name,
                    description: tool.custom.description.ok_or_else(|| {
                        TransformError::shape("Chat custom tool", "description is missing")
                    })?,
                    behavior: None,
                    parameters: None,
                    parameters_json_schema: None,
                    response: None,
                    response_json_schema: None,
                    rest: merge(tool.rest, tool.custom.rest),
                });
            }
            openai::ChatTool::Unknown(raw) => {
                return Err(TransformError::unsupported("Chat tool", raw.to_string()));
            }
        }
    }
    if !declarations.is_empty() {
        output.push(gemini::Tool {
            function_declarations: Some(declarations),
            ..Default::default()
        });
    }
    if code_execution {
        output.push(gemini::Tool {
            code_execution: Some(gemini::CodeExecution::default()),
            ..Default::default()
        });
    }
    if web_search {
        output.push(gemini::Tool {
            google_search: Some(gemini::GoogleSearch::default()),
            ..Default::default()
        });
    }
    Ok((!output.is_empty()).then_some(output))
}

pub(crate) fn choice(
    choice: Option<openai::ChatToolChoice>,
) -> Result<Option<gemini::ToolConfig>, TransformError> {
    let Some(choice) = choice else {
        return Ok(None);
    };
    let (mode, names) = match choice {
        openai::ChatToolChoice::Mode(openai::ToolChoiceMode::Auto) => (
            gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Auto),
            None,
        ),
        openai::ChatToolChoice::Mode(openai::ToolChoiceMode::Required) => (
            gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Any),
            None,
        ),
        openai::ChatToolChoice::Mode(openai::ToolChoiceMode::None) => (
            gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::None),
            None,
        ),
        openai::ChatToolChoice::Mode(openai::ToolChoiceMode::Unknown(value)) => {
            (gemini::FunctionCallingMode::Unknown(value), None)
        }
        openai::ChatToolChoice::Unknown(raw) => {
            return Err(TransformError::unsupported(
                "Chat tool choice",
                raw.to_string(),
            ));
        }
        openai::ChatToolChoice::Named(openai::ChatNamedToolChoice::Function(value)) => (
            gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Any),
            Some(vec![value.function.name]),
        ),
        openai::ChatToolChoice::Named(openai::ChatNamedToolChoice::Custom(value)) => (
            gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Any),
            Some(vec![value.custom.name]),
        ),
        openai::ChatToolChoice::Named(openai::ChatNamedToolChoice::Unknown(raw)) => {
            return Err(TransformError::unsupported(
                "Chat named tool choice",
                raw.to_string(),
            ));
        }
        openai::ChatToolChoice::Allowed(value) => {
            let mode = match value.allowed_tools.mode {
                openai::AllowedToolsMode::Auto => {
                    gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Validated)
                }
                openai::AllowedToolsMode::Required => {
                    gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Any)
                }
                openai::AllowedToolsMode::Unknown(value) => {
                    gemini::FunctionCallingMode::Unknown(value)
                }
            };
            let names = value
                .allowed_tools
                .tools
                .into_iter()
                .map(tool_name)
                .collect::<Result<Vec<_>, _>>()?;
            (mode, Some(names))
        }
    };
    Ok(Some(gemini::ToolConfig {
        function_calling_config: Some(gemini::FunctionCallingConfig {
            mode: Some(mode),
            allowed_function_names: names,
            rest: Default::default(),
        }),
        retrieval_config: None,
        include_server_side_tool_invocations: None,
        rest: Default::default(),
    }))
}

fn tool_name(mut tool: openai::Rest) -> Result<String, TransformError> {
    tool.remove("function")
        .or_else(|| tool.remove("custom"))
        .and_then(|value| {
            value
                .get("name")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .or_else(|| {
            tool.remove("name")
                .and_then(|value| value.as_str().map(str::to_owned))
        })
        .ok_or_else(|| TransformError::shape("Chat allowed tool", "name is missing"))
}

fn merge(mut left: openai::Rest, right: openai::Rest) -> openai::Rest {
    left.extend(right);
    left
}
