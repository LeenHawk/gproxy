use crate::{
    transform::{Report, TransformError},
    wire::{
        claude::{count_tokens as c, tools as ct},
        gemini as g,
    },
};
pub(crate) fn to_gemini(
    input: Option<Vec<ct::ToolUnion>>,
    choice: Option<c::ToolChoice>,
    report: &mut Report,
) -> Result<(Option<Vec<g::Tool>>, Option<g::ToolConfig>), TransformError> {
    let mut strict = false;
    let tools = input
        .map(|tools| {
            let mut functions = Vec::new();
            for tool in tools {
                let ct::ToolUnion::Custom(tool) = tool else {
                    return Err(TransformError::unsupported(
                        "tools",
                        "native Claude tools require Gemini host bindings",
                    ));
                };
                if tool.allowed_callers.as_ref().is_some_and(|v| {
                    v.iter()
                        .any(|caller| !matches!(caller, ct::AllowedCaller::Direct))
                }) || tool.defer_loading == Some(true)
                {
                    return Err(TransformError::unsupported(
                        "tools",
                        "programmatic callers and deferred lookup need an invocation adapter",
                    ));
                }
                strict |= tool.strict == Some(true);
                let schema = super::schema::to_json(&tool.input_schema)?;
                let mut function = g::FunctionDeclaration::builder(
                    tool.name,
                    tool.description.unwrap_or_default(),
                )
                .build();
                function.parameters_json_schema = Some(serde_json::Value::Object(schema));
                functions.push(function);
                if tool.cache_control.is_some()
                    || tool.eager_input_streaming.is_some()
                    || tool.input_examples.is_some()
                {
                    report.omitted(
                        "tools.advisory",
                        "Gemini has no tool cache/example/eager-streaming fields",
                    );
                }
            }
            Ok(vec![
                g::Tool::builder().function_declarations(functions).build(),
            ])
        })
        .transpose()?;
    let calling = match choice {
        Some(c::ToolChoice::Auto(v)) => {
            parallel(v.disable_parallel_tool_use)?;
            Some(
                g::FunctionCallingConfig::builder()
                    .mode(if strict {
                        g::FunctionCallingMode::Validated
                    } else {
                        g::FunctionCallingMode::Auto
                    })
                    .build(),
            )
        }
        Some(c::ToolChoice::Any(v)) => {
            parallel(v.disable_parallel_tool_use)?;
            Some(
                g::FunctionCallingConfig::builder()
                    .mode(g::FunctionCallingMode::Any)
                    .build(),
            )
        }
        Some(c::ToolChoice::Tool(v)) => {
            parallel(v.disable_parallel_tool_use)?;
            Some(
                g::FunctionCallingConfig::builder()
                    .mode(g::FunctionCallingMode::Any)
                    .allowed_function_names(vec![v.name])
                    .build(),
            )
        }
        Some(c::ToolChoice::None(_)) => Some(
            g::FunctionCallingConfig::builder()
                .mode(g::FunctionCallingMode::None)
                .build(),
        ),
        None if strict => Some(
            g::FunctionCallingConfig::builder()
                .mode(g::FunctionCallingMode::Validated)
                .build(),
        ),
        None => None,
    };
    Ok((
        tools,
        calling.map(|config| {
            g::ToolConfig::builder()
                .function_calling_config(config)
                .build()
        }),
    ))
}
fn parallel(disabled: Option<bool>) -> Result<(), TransformError> {
    if disabled == Some(true) {
        Err(TransformError::unsupported(
            "tool_choice.disable_parallel_tool_use",
            "Gemini has no equivalent per-turn parallel-call cap",
        ))
    } else {
        Ok(())
    }
}
pub(crate) fn to_claude(
    input: Option<Vec<g::Tool>>,
    config: Option<g::ToolConfig>,
    report: &mut Report,
) -> Result<(Option<Vec<ct::ToolUnion>>, Option<c::ToolChoice>), TransformError> {
    let calling = if let Some(config) = config {
        if config.retrieval_config.is_some()
            || config.include_server_side_tool_invocations == Some(true)
        {
            return Err(TransformError::unsupported(
                "tool_config",
                "native retrieval/server execution requires host binding",
            ));
        }
        config.function_calling_config
    } else {
        None
    };
    let mode = calling.as_ref().and_then(|v| v.mode.clone());
    let strict = matches!(
        mode,
        Some(g::FunctionCallingMode::Any | g::FunctionCallingMode::Validated)
    );
    let allowed = calling
        .as_ref()
        .and_then(|v| v.allowed_function_names.as_ref());
    if allowed.is_some_and(Vec::is_empty) {
        return Err(TransformError::shape(
            "allowed_function_names",
            "empty allowed function list",
        ));
    }
    let mut names = std::collections::BTreeSet::new();
    let tools = input
        .map(|tools| {
            let mut functions = Vec::new();
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
                        "native Gemini tools require Claude host bindings",
                    ));
                }
                for function in tool.function_declarations.unwrap_or_default() {
                    if matches!(function.behavior, Some(g::Behavior::NonBlocking))
                        || function.response.is_some()
                        || function.response_json_schema.is_some()
                    {
                        return Err(TransformError::unsupported(
                            "tools.function_declarations",
                            "nonblocking behavior and output schema lack Claude equivalents",
                        ));
                    }
                    if function.name.is_empty() || !names.insert(function.name.clone()) {
                        return Err(TransformError::shape(
                            "tools.name",
                            "empty or duplicate function name",
                        ));
                    }
                    if allowed.is_some_and(|v| !v.contains(&function.name)) {
                        continue;
                    }
                    let schema = match (function.parameters, function.parameters_json_schema) {
                        (Some(typed), raw) => {
                            let converted = crate::transform::generate::gemini_schema::to_json(
                                &typed,
                                Default::default(),
                            )?;
                            report.diagnostics.extend(converted.report.diagnostics);
                            if raw
                                .as_ref()
                                .is_some_and(|v| v.as_object() != Some(&converted.value))
                            {
                                return Err(TransformError::shape(
                                    "parameters",
                                    "typed and raw schema conflict",
                                ));
                            }
                            converted.value
                        }
                        (None, Some(serde_json::Value::Object(raw))) => raw,
                        (None, Some(_)) => {
                            return Err(TransformError::unsupported(
                                "parameters_json_schema",
                                "Claude input schema requires a JSON object",
                            ));
                        }
                        (None, None) => serde_json::Map::new(),
                    };
                    let schema = super::schema::from_json(&schema)?;
                    functions.push(ct::ToolUnion::Custom(
                        ct::Tool::builder(schema, function.name)
                            .description(function.description)
                            .strict(strict)
                            .build(),
                    ));
                }
            }
            Ok(functions)
        })
        .transpose()?;
    if let Some(allowed) = allowed
        && allowed.iter().any(|name| !names.contains(name))
    {
        return Err(TransformError::missing_metadata(
            "allowed function declaration",
        ));
    }
    let choice = match mode {
        Some(g::FunctionCallingMode::Unspecified) | None => None,
        Some(g::FunctionCallingMode::Auto | g::FunctionCallingMode::Validated) => {
            Some(c::ToolChoice::Auto(c::ToolChoiceAuto::builder().build()))
        }
        Some(g::FunctionCallingMode::None) => {
            Some(c::ToolChoice::None(c::ToolChoiceNone::builder().build()))
        }
        Some(g::FunctionCallingMode::Any) => {
            Some(c::ToolChoice::Any(c::ToolChoiceAny::builder().build()))
        }
    };
    Ok((tools, choice))
}
