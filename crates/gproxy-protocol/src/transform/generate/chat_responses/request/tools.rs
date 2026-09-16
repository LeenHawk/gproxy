use crate::{
    transform::TransformError,
    wire::openai::{
        chat as c,
        responses::{input as i, tools as r},
    },
};
pub(super) fn to_responses(tools: Vec<c::ChatTool>) -> Result<Vec<r::Tool>, TransformError> {
    tools
        .into_iter()
        .map(|tool| {
            Ok(match tool {
                c::ChatTool::Function(tool) => {
                    let mut out = r::FunctionTool::builder(
                        tool.function.name,
                        tool.function.parameters,
                        tool.function.strict.flatten(),
                    )
                    .build();
                    out.description = Some(tool.function.description);
                    out.allowed_callers = None;
                    r::Tool::Function(out)
                }
                c::ChatTool::Custom(tool) => {
                    let mut out = r::CustomTool::builder(tool.custom.name).build();
                    out.description = tool.custom.description;
                    out.format = tool.custom.format.map(|format| match format {
                        c::CustomToolFormat::Text(_) => r::CustomToolFormat::Text(
                            r::CustomTextFormat::builder(r::TextFormatType::Text).build(),
                        ),
                        c::CustomToolFormat::Grammar(format) => r::CustomToolFormat::Grammar(
                            r::GrammarFormat::builder(
                                format.grammar.definition,
                                match format.grammar.syntax {
                                    c::GrammarSyntax::Lark => r::GrammarSyntax::Lark,
                                    c::GrammarSyntax::Regex => r::GrammarSyntax::Regex,
                                },
                                r::GrammarFormatType::Grammar,
                            )
                            .build(),
                        ),
                    });
                    r::Tool::Custom(out)
                }
            })
        })
        .filter_map(|value| crate::transform::optional(value).transpose())
        .collect()
}
pub(super) fn to_chat(tools: Vec<r::Tool>) -> Result<Vec<c::ChatTool>, TransformError> {
    tools
        .into_iter()
        .map(|tool| {
            Ok(match tool {
                r::Tool::Function(tool) => {
                    let mut function = c::FunctionDefinition::builder(tool.name).build();
                    function.parameters = tool.parameters;
                    function.description = tool.description.flatten();
                    function.strict = Some(tool.strict);
                    c::ChatTool::Function(
                        c::FunctionTool::builder(c::FunctionToolType::Function, function).build(),
                    )
                }
                r::Tool::Custom(tool) => {
                    let mut out = c::CustomTool::builder(tool.name).build();
                    out.description = tool.description;
                    out.format = tool.format.map(|format| match format {
                        r::CustomToolFormat::Text(_) => c::CustomToolFormat::Text(
                            c::TextFormat::builder(c::TextFormatType::Text).build(),
                        ),
                        r::CustomToolFormat::Grammar(format) => c::CustomToolFormat::Grammar(
                            c::GrammarFormat::builder(
                                c::GrammarFormatType::Grammar,
                                c::Grammar::builder(
                                    format.definition,
                                    match format.syntax {
                                        r::GrammarSyntax::Lark => c::GrammarSyntax::Lark,
                                        r::GrammarSyntax::Regex => c::GrammarSyntax::Regex,
                                    },
                                )
                                .build(),
                            )
                            .build(),
                        ),
                    });
                    c::ChatTool::Custom(
                        c::CustomToolDefinition::builder(c::CustomToolDefinitionType::Custom, out)
                            .build(),
                    )
                }
                r::Tool::FileSearch(_)
                | r::Tool::Computer(_)
                | r::Tool::ComputerHosted(_)
                | r::Tool::WebSearch(_)
                | r::Tool::WebSearchPreview20250311(_)
                | r::Tool::WebSearchLegacy(_)
                | r::Tool::WebSearch2025(_)
                | r::Tool::CodeInterpreter(_)
                | r::Tool::Namespace(_)
                | r::Tool::LocalShell(_)
                | r::Tool::Shell(_)
                | r::Tool::ImageGeneration(_)
                | r::Tool::Mcp(_)
                | r::Tool::Programmatic(_)
                | r::Tool::ApplyPatch(_)
                | r::Tool::ToolSearch(_) => {
                    return Err(TransformError::unsupported(
                        "tools",
                        "Responses hosted/namespaced tools require an explicit Chat host adapter",
                    ));
                }
            })
        })
        .filter_map(|value| crate::transform::optional(value).transpose())
        .collect()
}
pub(super) fn choice_to_responses(choice: c::ToolChoice) -> Result<i::ToolChoice, TransformError> {
    Ok(match choice {
        c::ToolChoice::Mode(mode) => i::ToolChoice::Mode(match mode {
            c::ToolChoiceMode::None => i::ToolChoiceMode::None,
            c::ToolChoiceMode::Auto => i::ToolChoiceMode::Auto,
            c::ToolChoiceMode::Required => i::ToolChoiceMode::Required,
        }),
        c::ToolChoice::Function(choice) => i::ToolChoice::Function(
            i::ToolChoiceFunction::builder(
                i::ToolChoiceFunctionType::ToolChoiceFunction,
                choice.function.name,
            )
            .build(),
        ),
        c::ToolChoice::Custom(choice) => i::ToolChoice::Custom(
            i::ToolChoiceCustom::builder(
                i::ToolChoiceCustomType::ToolChoiceCustom,
                choice.custom.name,
            )
            .build(),
        ),
        c::ToolChoice::Allowed(choice) => i::ToolChoice::Allowed(
            i::ToolChoiceAllowed::builder(
                i::ToolChoiceAllowedType::ToolChoiceAllowed,
                match choice.allowed_tools.mode {
                    c::AllowedToolsMode::Auto => i::AllowedToolChoiceMode::Auto,
                    c::AllowedToolsMode::Required => i::AllowedToolChoiceMode::Required,
                },
                selectors(choice.allowed_tools.tools, true)?,
            )
            .build(),
        ),
    })
}
pub(super) fn choice_to_chat(choice: i::ToolChoice) -> Result<c::ToolChoice, TransformError> {
    Ok(match choice {
        i::ToolChoice::Mode(mode) => c::ToolChoice::Mode(match mode {
            i::ToolChoiceMode::None => c::ToolChoiceMode::None,
            i::ToolChoiceMode::Auto => c::ToolChoiceMode::Auto,
            i::ToolChoiceMode::Required => c::ToolChoiceMode::Required,
        }),
        i::ToolChoice::Function(choice) => c::ToolChoice::Function(
            c::NamedFunctionChoice::builder(
                c::NamedFunctionChoiceType::Function,
                c::NamedFunction::builder(choice.name).build(),
            )
            .build(),
        ),
        i::ToolChoice::Custom(choice) => c::ToolChoice::Custom(
            c::NamedCustomChoice::builder(
                c::NamedCustomChoiceType::Custom,
                c::NamedCustom::builder(choice.name).build(),
            )
            .build(),
        ),
        i::ToolChoice::Allowed(choice) => c::ToolChoice::Allowed(
            c::AllowedToolChoice::builder(
                c::AllowedToolChoiceType::AllowedTools,
                c::AllowedTools::builder(
                    match choice.mode {
                        i::AllowedToolChoiceMode::Auto => c::AllowedToolsMode::Auto,
                        i::AllowedToolChoiceMode::Required => c::AllowedToolsMode::Required,
                    },
                    selectors(choice.tools, false)?,
                )
                .build(),
            )
            .build(),
        ),
        i::ToolChoice::Hosted(_)
        | i::ToolChoice::Mcp(_)
        | i::ToolChoice::Programmatic(_)
        | i::ToolChoice::ApplyPatch(_)
        | i::ToolChoice::Shell(_) => {
            return Err(TransformError::unsupported(
                "tool_choice",
                "hosted tool choice requires Chat host adapter",
            ));
        }
    })
}

fn selectors(
    input: Vec<crate::Rest>,
    to_responses: bool,
) -> Result<Vec<crate::Rest>, TransformError> {
    input
        .into_iter()
        .map(|value| {
            let kind = value
                .get("type")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| {
                    TransformError::shape("tool_choice.allowed", "missing selector type")
                })?;
            if kind != "function" && kind != "custom" {
                return Err(TransformError::unsupported(
                    "tool_choice.allowed",
                    "selector names a hosted tool unavailable in Chat",
                ));
            }
            let name = if to_responses {
                value.get(kind).and_then(|v| v.get("name"))
            } else {
                value.get("name")
            }
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| TransformError::shape("tool_choice.allowed", "missing selector name"))?;
            let mut output = crate::Rest::new();
            output.insert("type".into(), kind.into());
            if to_responses {
                output.insert("name".into(), name.into());
            } else {
                output.insert(kind.into(), serde_json::json!({"name":name}));
            }
            Ok(output)
        })
        .filter_map(|value| crate::transform::optional(value).transpose())
        .collect()
}

pub(super) fn legacy(functions: Vec<c::LegacyFunction>) -> Result<Vec<r::Tool>, TransformError> {
    Ok(functions
        .into_iter()
        .map(|function| {
            let mut target =
                r::FunctionTool::builder(function.name, function.parameters, None).build();
            target.description = Some(function.description);
            r::Tool::Function(target)
        })
        .collect())
}
pub(super) fn legacy_choice(choice: c::FunctionCallChoice) -> i::ToolChoice {
    match choice {
        c::FunctionCallChoice::Mode(c::FunctionCallMode::Auto) => {
            i::ToolChoice::Mode(i::ToolChoiceMode::Auto)
        }
        c::FunctionCallChoice::Mode(c::FunctionCallMode::None) => {
            i::ToolChoice::Mode(i::ToolChoiceMode::None)
        }
        c::FunctionCallChoice::Name(choice) => i::ToolChoice::Function(
            i::ToolChoiceFunction::builder(
                i::ToolChoiceFunctionType::ToolChoiceFunction,
                choice.name,
            )
            .build(),
        ),
    }
}
