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
            let mut search = false;
            for tool in tools {
                if matches!(tool,
                    ct::ToolUnion::WebSearch20250305(_)
                    | ct::ToolUnion::WebSearch20260209(_)
                    | ct::ToolUnion::WebSearch20260318(_)
                ) {
                    search = true;
                    report.changed("tools.web_search", "executed by Gemini Google Search; Claude search controls have no direct equivalent");
                    continue;
                }
                let ct::ToolUnion::Custom(tool) = tool else {
                    continue;
                };
                if tool.allowed_callers.as_ref().is_some_and(|v| {
                    v.iter()
                        .any(|caller| !matches!(caller, ct::AllowedCaller::Direct))
                }) || tool.defer_loading == Some(true)
                {
                    report.omitted(
                        "tools.allowed_callers/defer_loading",
                        "controls have no target representation",
                    );
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
            let mut tools = Vec::new();
            if !functions.is_empty() {
                tools.push(g::Tool::builder().function_declarations(functions).build());
            }
            if search {
                tools.push(g::Tool::builder().google_search(g::GoogleSearch::builder().build()).build());
            }
            Ok::<_, TransformError>(tools)
        })
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    let calling = match choice {
        Some(c::ToolChoice::Auto(_v)) => Some(
            g::FunctionCallingConfig::builder()
                .mode(if strict {
                    g::FunctionCallingMode::Validated
                } else {
                    g::FunctionCallingMode::Auto
                })
                .build(),
        ),
        Some(c::ToolChoice::Any(_v)) => Some(
            g::FunctionCallingConfig::builder()
                .mode(g::FunctionCallingMode::Any)
                .build(),
        ),
        Some(c::ToolChoice::Tool(v)) => Some(
            g::FunctionCallingConfig::builder()
                .mode(g::FunctionCallingMode::Any)
                .allowed_function_names(vec![v.name])
                .build(),
        ),
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

pub(crate) fn to_claude(
    input: Option<Vec<g::Tool>>,
    config: Option<g::ToolConfig>,
    report: &mut Report,
) -> Result<(Option<Vec<ct::ToolUnion>>, Option<c::ToolChoice>), TransformError> {
    let calling = if let Some(config) = config {
        if config.retrieval_config.is_some()
            || config.include_server_side_tool_invocations == Some(true)
        {
            report.omitted(
                "tool_config.retrieval",
                "control has no target representation",
            );
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

    let tools = input
        .map(|tools| {
            let mut functions = Vec::new();
            for tool in tools {
                if tool.google_search_retrieval.is_some() || tool.google_search.is_some() {
                    functions.push(ct::ToolUnion::WebSearch20250305(ct::WebSearchTool20250305::builder(
                        ct::WebSearchTool20250305Name::Name, ct::WebSearchTool20250305Type::Tag,
                    ).build()));
                    report.changed("tools.google_search", "executed by Claude web search; provider-specific search controls are omitted");
                }
                if tool.code_execution.is_some()
                    || tool.computer_use.is_some()
                    || tool.url_context.is_some()
                    || tool.file_search.is_some()
                    || tool.mcp_servers.is_some()
                    || tool.google_maps.is_some()
                {
                    report.omitted("tools.hosted", "hosted tools have no target representation");
                }
                for function in tool.function_declarations.unwrap_or_default() {
                    if matches!(function.behavior, Some(g::Behavior::NonBlocking))
                        || function.response.is_some()
                        || function.response_json_schema.is_some()
                    {
                        report.omitted(
                            "tools.function_declarations.behavior/response",
                            "controls have no target representation",
                        );
                    }

                    if allowed.is_some_and(|v| !v.contains(&function.name)) {
                        continue;
                    }
                    let schema = match (function.parameters, function.parameters_json_schema) {
                        (Some(typed), _raw) => {
                            let converted = crate::transform::generate::gemini_schema::to_json(
                                &typed,
                                Default::default(),
                            )?;
                            report.diagnostics.extend(converted.report.diagnostics);

                            converted.value
                        }
                        (None, Some(serde_json::Value::Object(raw))) => raw,
                        (None, Some(_)) => {
                            continue;
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
            Ok::<_, TransformError>(functions)
        })
        .map(crate::transform::optional)
        .transpose()?
        .flatten();

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
