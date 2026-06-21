use crate::protocol::{claude, gemini, openai};

pub(in crate::transform::count_tokens) fn openai_tool_config_to_gemini(
    tool_choice: Option<openai::ResponseToolChoice>,
) -> Option<gemini::ToolConfig> {
    let function_calling_config = match tool_choice? {
        openai::ResponseToolChoice::Mode(mode) => Some(gemini::FunctionCallingConfig {
            mode: Some(openai_tool_choice_mode_to_gemini(mode)),
            allowed_function_names: Vec::new(),
            extra: Default::default(),
        }),
        openai::ResponseToolChoice::Allowed(choice) => Some(gemini::FunctionCallingConfig {
            mode: Some(openai_allowed_tools_mode_to_gemini(choice.mode)),
            allowed_function_names: choice
                .tools
                .into_iter()
                .filter_map(openai_allowed_tool_name)
                .collect(),
            extra: Default::default(),
        }),
        openai::ResponseToolChoice::Function(choice) => Some(gemini::FunctionCallingConfig {
            mode: Some(gemini::FunctionCallingMode::Known(
                gemini::FunctionCallingModeKnown::Any,
            )),
            allowed_function_names: vec![choice.name],
            extra: Default::default(),
        }),
        _ => None,
    }?;

    Some(gemini::ToolConfig {
        function_calling_config: Some(function_calling_config),
        retrieval_config: None,
        include_server_side_tool_invocations: None,
        extra: Default::default(),
    })
}

fn openai_tool_choice_mode_to_gemini(mode: openai::ToolChoiceMode) -> gemini::FunctionCallingMode {
    match mode {
        openai::ToolChoiceMode::None => {
            gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::None)
        }
        openai::ToolChoiceMode::Auto => {
            gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Auto)
        }
        openai::ToolChoiceMode::Required => {
            gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Any)
        }
    }
}

fn openai_allowed_tools_mode_to_gemini(
    mode: openai::AllowedToolsMode,
) -> gemini::FunctionCallingMode {
    match mode {
        openai::AllowedToolsMode::Auto => {
            gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Auto)
        }
        openai::AllowedToolsMode::Required => {
            gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Any)
        }
    }
}

fn openai_allowed_tool_name(tool: openai::ResponseAllowedTool) -> Option<String> {
    match tool {
        openai::ResponseAllowedTool::Function { name, .. } => Some(name),
        _ => None,
    }
}

pub(in crate::transform::count_tokens) fn claude_tool_config_to_gemini(
    tool_choice: Option<claude::ToolChoice>,
) -> Option<gemini::ToolConfig> {
    let function_calling_config = match tool_choice? {
        claude::ToolChoice::Auto(_) => Some(gemini::FunctionCallingConfig {
            mode: Some(gemini::FunctionCallingMode::Known(
                gemini::FunctionCallingModeKnown::Auto,
            )),
            allowed_function_names: Vec::new(),
            extra: Default::default(),
        }),
        claude::ToolChoice::Any(_) => Some(gemini::FunctionCallingConfig {
            mode: Some(gemini::FunctionCallingMode::Known(
                gemini::FunctionCallingModeKnown::Any,
            )),
            allowed_function_names: Vec::new(),
            extra: Default::default(),
        }),
        claude::ToolChoice::Tool(choice) => Some(gemini::FunctionCallingConfig {
            mode: Some(gemini::FunctionCallingMode::Known(
                gemini::FunctionCallingModeKnown::Any,
            )),
            allowed_function_names: vec![choice.name],
            extra: Default::default(),
        }),
        claude::ToolChoice::None(_) => Some(gemini::FunctionCallingConfig {
            mode: Some(gemini::FunctionCallingMode::Known(
                gemini::FunctionCallingModeKnown::None,
            )),
            allowed_function_names: Vec::new(),
            extra: Default::default(),
        }),
        claude::ToolChoice::Unknown(_) => None,
    }?;

    Some(gemini::ToolConfig {
        function_calling_config: Some(function_calling_config),
        retrieval_config: None,
        include_server_side_tool_invocations: None,
        extra: Default::default(),
    })
}

pub(in crate::transform::count_tokens) fn claude_tool_choice_to_openai(
    tool_choice: Option<claude::ToolChoice>,
) -> Option<openai::ResponseToolChoice> {
    match tool_choice? {
        claude::ToolChoice::Auto(_) => Some(openai::ResponseToolChoice::Mode(
            openai::ToolChoiceMode::Auto,
        )),
        claude::ToolChoice::Any(_) => Some(openai::ResponseToolChoice::Mode(
            openai::ToolChoiceMode::Required,
        )),
        claude::ToolChoice::Tool(choice) => Some(openai::ResponseToolChoice::Function(
            openai::ResponseFunctionToolChoice {
                type_: openai::FunctionToolChoiceType::Function,
                name: choice.name,
                extra: Default::default(),
            },
        )),
        claude::ToolChoice::None(_) => Some(openai::ResponseToolChoice::Mode(
            openai::ToolChoiceMode::None,
        )),
        claude::ToolChoice::Unknown(_) => None,
    }
}

pub(in crate::transform::count_tokens) fn gemini_tool_config_to_openai(
    tool_config: Option<gemini::ToolConfig>,
) -> Option<openai::ResponseToolChoice> {
    let config = tool_config?.function_calling_config?;
    let names = config.allowed_function_names;
    match config.mode? {
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::None) => Some(
            openai::ResponseToolChoice::Mode(openai::ToolChoiceMode::None),
        ),
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Auto) => {
            if names.is_empty() {
                Some(openai::ResponseToolChoice::Mode(
                    openai::ToolChoiceMode::Auto,
                ))
            } else {
                Some(openai::ResponseToolChoice::Allowed(
                    openai_allowed_function_choice(openai::AllowedToolsMode::Auto, names),
                ))
            }
        }
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Any)
        | gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Validated) => {
            if names.len() == 1 {
                Some(openai::ResponseToolChoice::Function(
                    openai::ResponseFunctionToolChoice {
                        type_: openai::FunctionToolChoiceType::Function,
                        name: names.into_iter().next().unwrap_or_default(),
                        extra: Default::default(),
                    },
                ))
            } else if names.is_empty() {
                Some(openai::ResponseToolChoice::Mode(
                    openai::ToolChoiceMode::Required,
                ))
            } else {
                Some(openai::ResponseToolChoice::Allowed(
                    openai_allowed_function_choice(openai::AllowedToolsMode::Required, names),
                ))
            }
        }
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::ModeUnspecified)
        | gemini::FunctionCallingMode::Unknown(_) => None,
    }
}

pub(in crate::transform::count_tokens) fn gemini_tool_config_to_claude(
    tool_config: Option<gemini::ToolConfig>,
) -> Option<claude::ToolChoice> {
    let config = tool_config?.function_calling_config?;
    let names = config.allowed_function_names;
    match config.mode? {
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::None) => {
            Some(claude::ToolChoice::None(claude::ToolChoiceNone {
                type_: claude::ToolChoiceNoneType::None,
                extra: Default::default(),
            }))
        }
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Auto) => {
            Some(claude::ToolChoice::Auto(claude::ToolChoiceAuto {
                type_: claude::ToolChoiceAutoType::Auto,
                disable_parallel_tool_use: None,
                extra: Default::default(),
            }))
        }
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Any)
        | gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::Validated) => {
            if names.len() == 1 {
                Some(claude::ToolChoice::Tool(claude::ToolChoiceTool {
                    name: names.into_iter().next().unwrap_or_default(),
                    type_: claude::ToolChoiceToolType::Tool,
                    disable_parallel_tool_use: None,
                    extra: Default::default(),
                }))
            } else {
                Some(claude::ToolChoice::Any(claude::ToolChoiceAny {
                    type_: claude::ToolChoiceAnyType::Any,
                    disable_parallel_tool_use: None,
                    extra: Default::default(),
                }))
            }
        }
        gemini::FunctionCallingMode::Known(gemini::FunctionCallingModeKnown::ModeUnspecified)
        | gemini::FunctionCallingMode::Unknown(_) => None,
    }
}

fn openai_allowed_function_choice(
    mode: openai::AllowedToolsMode,
    names: Vec<String>,
) -> openai::ResponseAllowedToolChoice {
    openai::ResponseAllowedToolChoice {
        mode,
        tools: names
            .into_iter()
            .map(|name| openai::ResponseAllowedTool::Function {
                name,
                extra: Default::default(),
            })
            .collect(),
        type_: openai::AllowedToolsType::AllowedTools,
        extra: Default::default(),
    }
}

pub(in crate::transform::count_tokens) fn openai_tool_choice_to_claude(
    tool_choice: Option<openai::ResponseToolChoice>,
    parallel_tool_calls: Option<bool>,
) -> Option<claude::ToolChoice> {
    let disable_parallel_tool_use = parallel_tool_calls.map(|value| !value);
    match tool_choice {
        Some(openai::ResponseToolChoice::Mode(openai::ToolChoiceMode::None)) => {
            Some(claude::ToolChoice::None(claude::ToolChoiceNone {
                type_: claude::ToolChoiceNoneType::None,
                extra: Default::default(),
            }))
        }
        Some(openai::ResponseToolChoice::Mode(openai::ToolChoiceMode::Auto)) | None => {
            disable_parallel_tool_use.map(|disable_parallel_tool_use| {
                claude::ToolChoice::Auto(claude::ToolChoiceAuto {
                    type_: claude::ToolChoiceAutoType::Auto,
                    disable_parallel_tool_use: Some(disable_parallel_tool_use),
                    extra: Default::default(),
                })
            })
        }
        Some(openai::ResponseToolChoice::Mode(openai::ToolChoiceMode::Required)) => {
            Some(claude::ToolChoice::Any(claude::ToolChoiceAny {
                type_: claude::ToolChoiceAnyType::Any,
                disable_parallel_tool_use,
                extra: Default::default(),
            }))
        }
        Some(openai::ResponseToolChoice::Function(choice)) => {
            Some(claude::ToolChoice::Tool(claude::ToolChoiceTool {
                name: choice.name,
                type_: claude::ToolChoiceToolType::Tool,
                disable_parallel_tool_use,
                extra: Default::default(),
            }))
        }
        Some(openai::ResponseToolChoice::Allowed(choice)) => {
            let names = choice
                .tools
                .into_iter()
                .filter_map(openai_allowed_tool_name)
                .collect::<Vec<_>>();
            if names.len() == 1 {
                Some(claude::ToolChoice::Tool(claude::ToolChoiceTool {
                    name: names.into_iter().next().unwrap_or_default(),
                    type_: claude::ToolChoiceToolType::Tool,
                    disable_parallel_tool_use,
                    extra: Default::default(),
                }))
            } else {
                Some(claude::ToolChoice::Any(claude::ToolChoiceAny {
                    type_: claude::ToolChoiceAnyType::Any,
                    disable_parallel_tool_use,
                    extra: Default::default(),
                }))
            }
        }
        _ => None,
    }
}

pub(in crate::transform::count_tokens) fn claude_parallel_tool_calls(
    tool_choice: Option<&claude::ToolChoice>,
) -> Option<bool> {
    match tool_choice? {
        claude::ToolChoice::Auto(choice) => choice.disable_parallel_tool_use.map(|value| !value),
        claude::ToolChoice::Any(choice) => choice.disable_parallel_tool_use.map(|value| !value),
        claude::ToolChoice::Tool(choice) => choice.disable_parallel_tool_use.map(|value| !value),
        claude::ToolChoice::None(_) | claude::ToolChoice::Unknown(_) => None,
    }
}
