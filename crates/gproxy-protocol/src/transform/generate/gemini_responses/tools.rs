use crate::{
    transform::{Report, TransformError},
    wire::{
        gemini as g,
        openai::responses::{input as i, tools as r},
    },
};

pub(crate) fn to_responses(
    input: Vec<g::Tool>,
    validated: bool,
    report: &mut Report,
) -> Result<Vec<r::Tool>, TransformError> {
    let mut out = Vec::new();
    for tool in input {
        if tool.google_search_retrieval.is_some() || tool.google_search.is_some() {
            out.push(r::Tool::WebSearchLegacy(
                r::WebSearchTool::builder().build(),
            ));
            report.changed(
                "tools.google_search",
                "executed by Responses web search; provider-specific search controls are omitted",
            );
        }
        if tool.code_execution.is_some()
            || tool.computer_use.is_some()
            || tool.url_context.is_some()
            || tool.file_search.is_some()
            || tool.mcp_servers.is_some()
            || tool.google_maps.is_some()
        {
            report.omitted("tools.hosted", "hosted tool has no target representation");
        }
        for function in tool.function_declarations.unwrap_or_default() {
            if function.behavior.is_some() {
                report.omitted("function.behavior", "control has no target representation");
            }
            let parameters = schema(
                function.parameters.as_ref(),
                function.parameters_json_schema.as_ref(),
                report,
            )?;
            let response = schema(
                function.response.as_ref(),
                function.response_json_schema.as_ref(),
                report,
            )?;
            let mut target =
                r::FunctionTool::builder(function.name, parameters, validated.then_some(true))
                    .build();
            target.description = Some(Some(function.description));
            target.output_schema = response.map(Some);
            out.push(r::Tool::Function(target));
        }
    }
    Ok(out)
}

pub(crate) fn to_gemini(input: Vec<r::Tool>) -> Result<(Vec<g::Tool>, bool), TransformError> {
    let mut functions = Vec::new();
    let mut strict = false;
    let mut search = false;
    for tool in input {
        if matches!(
            tool,
            r::Tool::WebSearch(_)
                | r::Tool::WebSearchPreview20250311(_)
                | r::Tool::WebSearchLegacy(_)
                | r::Tool::WebSearch2025(_)
        ) {
            search = true;
            continue;
        }
        let r::Tool::Function(tool) = tool else {
            continue;
        };

        strict |= tool.strict == Some(true);
        let mut out = g::FunctionDeclaration::builder(
            tool.name,
            tool.description.flatten().unwrap_or_default(),
        )
        .build();
        out.parameters_json_schema = tool.parameters.map(serde_json::Value::Object);
        out.response_json_schema = tool.output_schema.flatten().map(serde_json::Value::Object);
        functions.push(out);
    }
    let mut tools = Vec::new();
    if !functions.is_empty() {
        tools.push(g::Tool::builder().function_declarations(functions).build());
    }
    if search {
        tools.push(
            g::Tool::builder()
                .google_search(g::GoogleSearch::builder().build())
                .build(),
        );
    }
    if search {
        // Search grounding alone does not read caller-supplied URLs.
        tools.push(
            g::Tool::builder()
                .url_context(g::UrlContext::builder().build())
                .build(),
        );
    }
    Ok((tools, strict))
}

fn schema(
    typed: Option<&g::Schema>,
    raw: Option<&serde_json::Value>,
    report: &mut Report,
) -> Result<Option<crate::Rest>, TransformError> {
    let typed = typed
        .map(super::super::gemini_schema::to_json)
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    if let Some(v) = &typed {
        report.diagnostics.extend(v.report.diagnostics.clone());
    }
    let raw = raw.and_then(|value| value.as_object().cloned());

    Ok(typed.map(|v| v.value).or(raw))
}

pub(crate) fn choice_to_responses(
    config: g::ToolConfig,
) -> Result<Option<i::ToolChoice>, TransformError> {
    let Some(config) = config.function_calling_config else {
        return Ok(None);
    };
    let mode = config.mode.unwrap_or(g::FunctionCallingMode::Auto);
    let required = mode == g::FunctionCallingMode::Any;
    let mode = match mode {
        g::FunctionCallingMode::Auto | g::FunctionCallingMode::Validated => i::ToolChoiceMode::Auto,
        g::FunctionCallingMode::Any => i::ToolChoiceMode::Required,
        g::FunctionCallingMode::None => i::ToolChoiceMode::None,
        g::FunctionCallingMode::Unspecified => {
            return Ok(None);
        }
    };
    Ok(Some(if let Some(names) = config.allowed_function_names {
        if mode == i::ToolChoiceMode::None {
            return Ok(Some(i::ToolChoice::Mode(mode)));
        }
        let tools = names
            .into_iter()
            .map(|name| {
                serde_json::json!({"type":"function","name":name})
                    .as_object()
                    .unwrap()
                    .clone()
            })
            .collect();
        i::ToolChoice::Allowed(
            i::ToolChoiceAllowed::builder(
                i::ToolChoiceAllowedType::ToolChoiceAllowed,
                if required {
                    i::AllowedToolChoiceMode::Required
                } else {
                    i::AllowedToolChoiceMode::Auto
                },
                tools,
            )
            .build(),
        )
    } else {
        i::ToolChoice::Mode(mode)
    }))
}

pub(crate) fn choice_to_gemini(
    choice: Option<i::ToolChoice>,
    strict: bool,
) -> Result<Option<g::ToolConfig>, TransformError> {
    if choice.is_none() && !strict {
        return Ok(None);
    }
    let mut config = g::FunctionCallingConfig::builder().build();
    match choice.unwrap_or(i::ToolChoice::Mode(i::ToolChoiceMode::Auto)) {
        i::ToolChoice::Mode(mode) => {
            config.mode = Some(match mode {
                i::ToolChoiceMode::Auto => {
                    if strict {
                        g::FunctionCallingMode::Validated
                    } else {
                        g::FunctionCallingMode::Auto
                    }
                }
                i::ToolChoiceMode::Required => g::FunctionCallingMode::Any,
                i::ToolChoiceMode::None => g::FunctionCallingMode::None,
            })
        }
        i::ToolChoice::Function(choice) => {
            config.mode = Some(g::FunctionCallingMode::Any);
            config.allowed_function_names = Some(vec![choice.name]);
        }
        i::ToolChoice::Allowed(choice) => {
            config.mode = Some(match choice.mode {
                i::AllowedToolChoiceMode::Auto => {
                    if strict {
                        g::FunctionCallingMode::Validated
                    } else {
                        g::FunctionCallingMode::Auto
                    }
                }
                i::AllowedToolChoiceMode::Required => g::FunctionCallingMode::Any,
            });
            config.allowed_function_names = Some(
                choice
                    .tools
                    .into_iter()
                    .map(|v| {
                        if v.get("type").and_then(|v| v.as_str()) != Some("function") {
                            return Err(TransformError::unsupported(
                                "allowed_tools",
                                "Gemini accepts function selectors only",
                            ));
                        }
                        v.get("name")
                            .and_then(|v| v.as_str())
                            .map(str::to_owned)
                            .ok_or_else(|| TransformError::shape("allowed_tools", "missing name"))
                    })
                    .filter_map(|value| crate::transform::optional(value).transpose())
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        i::ToolChoice::Hosted(_)
        | i::ToolChoice::Mcp(_)
        | i::ToolChoice::Custom(_)
        | i::ToolChoice::Programmatic(_)
        | i::ToolChoice::ApplyPatch(_)
        | i::ToolChoice::Shell(_) => {
            return Err(TransformError::unsupported(
                "tool_choice",
                "hosted choice needs Gemini adapter",
            ));
        }
    }
    Ok(Some(
        g::ToolConfig::builder()
            .function_calling_config(config)
            .build(),
    ))
}
