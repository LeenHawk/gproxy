use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(crate) const CODE_EXECUTION_NAME: &str = "gemini_code_execution";

pub(crate) fn transform(
    tools: Option<Vec<gemini::Tool>>,
) -> Result<Option<Vec<openai::ChatTool>>, TransformError> {
    let mut output = Vec::new();
    let mut code_execution = false;
    for tool in tools.into_iter().flatten() {
        output.extend(
            tool.function_declarations
                .into_iter()
                .flatten()
                .map(function_declaration)
                .collect::<Result<Vec<_>, _>>()?,
        );
        code_execution |= tool.code_execution.is_some();
    }
    if code_execution {
        output.push(code_execution_tool());
    }
    Ok((!output.is_empty()).then_some(output))
}

pub(crate) fn choice(
    config: Option<gemini::ToolConfig>,
) -> Result<Option<openai::ChatToolChoice>, TransformError> {
    let Some(config) = config.and_then(|config| config.function_calling_config) else {
        return Ok(None);
    };
    let Some(mode) = config.mode else {
        return Ok(None);
    };
    Ok(match mode {
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Auto)
        | gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::ModeUnspecified) => {
            if config.allowed_function_names.is_some() {
                return Err(TransformError::shape(
                    "Gemini tool config",
                    "allowedFunctionNames requires ANY or VALIDATED mode",
                ));
            }
            Some(openai::ChatToolChoice::Mode(openai::ToolChoiceMode::Auto))
        }
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Any) => {
            choice_with_names(
                openai::AllowedToolsMode::Required,
                config.allowed_function_names,
            )
        }
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Validated) => {
            choice_with_names(
                openai::AllowedToolsMode::Auto,
                config.allowed_function_names,
            )
        }
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::None) => {
            if config.allowed_function_names.is_some() {
                return Err(TransformError::shape(
                    "Gemini tool config",
                    "allowedFunctionNames requires ANY or VALIDATED mode",
                ));
            }
            Some(openai::ChatToolChoice::Mode(openai::ToolChoiceMode::None))
        }
        gemini::FunctionCallingMode::Unknown(value) => Some(openai::ChatToolChoice::Mode(
            openai::ToolChoiceMode::Unknown(value),
        )),
        _ => {
            return Err(TransformError::unsupported(
                "Gemini function calling mode",
                "future mode",
            ));
        }
    })
}

fn choice_with_names(
    mode: openai::AllowedToolsMode,
    names: Option<Vec<String>>,
) -> Option<openai::ChatToolChoice> {
    let Some(names) = names else {
        return Some(openai::ChatToolChoice::Mode(match mode {
            openai::AllowedToolsMode::Required => openai::ToolChoiceMode::Required,
            openai::AllowedToolsMode::Auto | openai::AllowedToolsMode::Unknown(_) => {
                openai::ToolChoiceMode::Auto
            }
        }));
    };
    Some(openai::ChatToolChoice::Allowed(
        openai::ChatAllowedToolChoice {
            allowed_tools: openai::ChatAllowedTools {
                mode,
                tools: names.into_iter().map(allowed_function).collect(),
                rest: Default::default(),
            },
            type_: openai::AllowedToolsType::AllowedTools,
            rest: Default::default(),
        },
    ))
}

fn allowed_function(name: String) -> openai::Rest {
    let mut function = openai::Rest::new();
    function.insert("name".into(), name.into());
    let mut tool = openai::Rest::new();
    tool.insert("type".into(), "function".into());
    tool.insert("function".into(), serde_json::Value::Object(function));
    tool
}

fn function_declaration(
    declaration: gemini::FunctionDeclaration,
) -> Result<openai::ChatTool, TransformError> {
    let parameters = match (declaration.parameters_json_schema, declaration.parameters) {
        (Some(value), Some(_)) | (Some(value), None) => Some(json_object(value)?),
        (None, Some(schema)) => Some(json_object(serde_json::to_value(schema)?)?),
        (None, None) => None,
    };
    Ok(openai::ChatTool::Function(openai::ChatFunctionTool {
        type_: openai::FunctionToolChoiceType::Function,
        function: openai::FunctionDefinition {
            name: declaration.name,
            description: Some(declaration.description),
            parameters,
            strict: None,
            rest: declaration.rest,
        },
        rest: Default::default(),
    }))
}

fn code_execution_tool() -> openai::ChatTool {
    let mut properties = serde_json::Map::new();
    properties.insert("language".into(), serde_json::json!({ "type": "string" }));
    properties.insert("code".into(), serde_json::json!({ "type": "string" }));
    let mut parameters = serde_json::Map::new();
    parameters.insert("type".into(), "object".into());
    parameters.insert("properties".into(), properties.into());
    parameters.insert("required".into(), serde_json::json!(["language", "code"]));
    openai::ChatTool::Function(openai::ChatFunctionTool {
        type_: openai::FunctionToolChoiceType::Function,
        function: openai::FunctionDefinition {
            name: CODE_EXECUTION_NAME.into(),
            description: Some("Execute model-generated code and return its result.".into()),
            parameters: Some(parameters),
            strict: Some(true),
            rest: Default::default(),
        },
        rest: Default::default(),
    })
}

fn json_object(value: serde_json::Value) -> Result<openai::JsonSchema, TransformError> {
    match value {
        serde_json::Value::Object(value) => Ok(value),
        _ => Err(TransformError::shape(
            "Gemini function schema",
            "expected an object",
        )),
    }
}
