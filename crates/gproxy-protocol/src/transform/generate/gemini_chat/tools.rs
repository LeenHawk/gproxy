use crate::{
    Rest,
    transform::{Report, TransformError},
    wire::{gemini, openai::chat},
};

pub(crate) fn gemini_tools_to_chat(
    tools: Option<&Vec<gemini::Tool>>,
    report: &mut Report,
) -> Result<Option<Vec<chat::ChatTool>>, TransformError> {
    let Some(tools) = tools else {
        return Ok(None);
    };
    let mut output = Vec::new();
    for tool in tools {
        if tool.google_search_retrieval.is_some()
            || tool.code_execution.is_some()
            || tool.google_search.is_some()
            || tool.computer_use.is_some()
            || tool.url_context.is_some()
            || tool.file_search.is_some()
            || tool.mcp_servers.is_some()
            || tool.google_maps.is_some()
        {
            return Err(TransformError::unsupported(
                "tools",
                "Gemini hosted tools require host adapter",
            ));
        }
        let Some(functions) = &tool.function_declarations else {
            continue;
        };
        for function in functions {
            if function.behavior.is_some()
                || function.response.is_some()
                || function.response_json_schema.is_some()
            {
                return Err(TransformError::unsupported(
                    "function_declaration",
                    "Chat lacks asynchronous behavior/output schema",
                ));
            }
            let parameters = match (
                function.parameters.as_ref(),
                &function.parameters_json_schema,
            ) {
                (Some(schema), raw) => {
                    let converted = crate::transform::generate::gemini_schema::to_json(
                        schema,
                        Default::default(),
                    )?;
                    if raw
                        .as_ref()
                        .is_some_and(|raw| raw.as_object() != Some(&converted.value))
                    {
                        return Err(TransformError::shape(
                            "parameters",
                            "typed/raw schemas conflict",
                        ));
                    }
                    report.diagnostics.extend(converted.report.diagnostics);
                    Some(converted.value)
                }
                (None, Some(schema)) => Some(schema.as_object().cloned().ok_or_else(|| {
                    TransformError::unsupported(
                        "parameters_json_schema",
                        "Chat function schema must be an object",
                    )
                })?),
                (None, None) => None,
            };
            output.push(chat::ChatTool::Function(chat::FunctionTool {
                type_: chat::FunctionToolType::Function,
                function: chat::FunctionDefinition {
                    name: function.name.clone(),
                    description: Some(function.description.clone()),
                    parameters,
                    strict: None,
                    rest: Rest::new(),
                },
                rest: Rest::new(),
            }));
        }
    }
    Ok(Some(output))
}

pub(crate) fn chat_tools_to_gemini(
    tools: Option<&Vec<chat::ChatTool>>,
    report: &mut Report,
) -> Result<Option<Vec<gemini::Tool>>, TransformError> {
    let Some(tools) = tools else {
        return Ok(None);
    };
    let mut declarations = Vec::new();
    for tool in tools {
        let chat::ChatTool::Function(tool) = tool else {
            return Err(TransformError::unsupported(
                "tools",
                "custom Chat tools have no Gemini function declaration",
            ));
        };
        if tool.function.strict.flatten() == Some(true) {
            report.changed(
                "tools.strict",
                "Gemini schema enforcement depends on selected function-calling endpoint",
            );
        }
        let schema = None;
        declarations.push(gemini::FunctionDeclaration {
            name: tool.function.name.clone(),
            description: tool.function.description.clone().unwrap_or_default(),
            behavior: None,
            parameters: schema,
            parameters_json_schema: tool
                .function
                .parameters
                .clone()
                .map(serde_json::Value::Object),
            response: None,
            response_json_schema: None,
            rest: Rest::new(),
        });
    }
    Ok(Some(vec![gemini::Tool {
        function_declarations: Some(declarations),
        google_search_retrieval: None,
        code_execution: None,
        google_search: None,
        computer_use: None,
        url_context: None,
        file_search: None,
        mcp_servers: None,
        google_maps: None,
        rest: Rest::new(),
    }]))
}

pub(super) fn legacy(functions: &[chat::LegacyFunction]) -> Vec<gemini::Tool> {
    let declarations = functions
        .iter()
        .map(|function| {
            let mut out = gemini::FunctionDeclaration::builder(
                function.name.clone(),
                function.description.clone().unwrap_or_default(),
            )
            .build();
            out.parameters_json_schema = function.parameters.clone().map(serde_json::Value::Object);
            out
        })
        .collect::<Vec<_>>();
    vec![
        gemini::Tool::builder()
            .function_declarations(declarations)
            .build(),
    ]
}
