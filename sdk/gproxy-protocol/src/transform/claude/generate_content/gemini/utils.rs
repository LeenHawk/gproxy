use crate::claude::count_tokens::types::{
    BetaOutputEffort, BetaSystemPrompt, BetaThinkingConfigParam, BetaToolChoice, BetaToolUnion,
};
use crate::gemini::count_tokens::types::{
    GeminiCodeExecution, GeminiComputerUse, GeminiContent, GeminiEnvironment, GeminiFileSearch,
    GeminiFunctionCallingConfig, GeminiFunctionCallingMode, GeminiFunctionDeclaration,
    GeminiGoogleSearch, GeminiPart, GeminiThinkingConfig, GeminiThinkingLevel, GeminiTool,
    GeminiToolConfig, GeminiUrlContext,
};
use crate::transform::claude::utils::beta_system_prompt_to_text;

pub fn gemini_system_instruction_from_claude(
    system: Option<BetaSystemPrompt>,
) -> Option<GeminiContent> {
    beta_system_prompt_to_text(system).map(|text| GeminiContent {
        parts: vec![GeminiPart {
            text: Some(text),
            ..GeminiPart::default()
        }],
        role: None,
    })
}

pub fn gemini_tools_from_claude(
    tools: Option<Vec<BetaToolUnion>>,
    include_custom_schema: bool,
) -> Option<Vec<GeminiTool>> {
    let mut converted_tools = Vec::new();

    if let Some(tools) = tools {
        for tool in tools {
            match tool {
                BetaToolUnion::Custom(tool) => {
                    let parameters_json_schema = if include_custom_schema {
                        serde_json::to_value(tool.input_schema).ok()
                    } else {
                        None
                    };
                    converted_tools.push(GeminiTool {
                        function_declarations: Some(vec![GeminiFunctionDeclaration {
                            name: tool.name,
                            description: tool.description.unwrap_or_default(),
                            behavior: None,
                            parameters: None,
                            parameters_json_schema,
                            response: None,
                            response_json_schema: None,
                        }]),
                        ..GeminiTool::default()
                    });
                }
                BetaToolUnion::CodeExecution20250522(_)
                | BetaToolUnion::CodeExecution20250825(_) => {
                    converted_tools.push(GeminiTool {
                        code_execution: Some(GeminiCodeExecution {}),
                        ..GeminiTool::default()
                    });
                }
                BetaToolUnion::ComputerUse20241022(_)
                | BetaToolUnion::ComputerUse20250124(_)
                | BetaToolUnion::ComputerUse20251124(_) => {
                    converted_tools.push(GeminiTool {
                        computer_use: Some(GeminiComputerUse {
                            environment: GeminiEnvironment::EnvironmentBrowser,
                            excluded_predefined_functions: None,
                        }),
                        ..GeminiTool::default()
                    });
                }
                BetaToolUnion::WebSearch20250305(_) => {
                    converted_tools.push(GeminiTool {
                        google_search: Some(GeminiGoogleSearch::default()),
                        ..GeminiTool::default()
                    });
                }
                BetaToolUnion::WebFetch20250910(_) => {
                    converted_tools.push(GeminiTool {
                        url_context: Some(GeminiUrlContext {}),
                        ..GeminiTool::default()
                    });
                }
                BetaToolUnion::ToolSearchBm25_20251119(_)
                | BetaToolUnion::ToolSearchRegex20251119(_) => {
                    converted_tools.push(GeminiTool {
                        file_search: Some(GeminiFileSearch::default()),
                        ..GeminiTool::default()
                    });
                }
                BetaToolUnion::Bash20241022(_)
                | BetaToolUnion::Bash20250124(_)
                | BetaToolUnion::TextEditor20241022(_)
                | BetaToolUnion::TextEditor20250124(_)
                | BetaToolUnion::TextEditor20250429(_)
                | BetaToolUnion::TextEditor20250728(_) => {
                    converted_tools.push(GeminiTool {
                        code_execution: Some(GeminiCodeExecution {}),
                        ..GeminiTool::default()
                    });
                }
                BetaToolUnion::McpToolset(_) | BetaToolUnion::Memory20250818(_) => {}
            }
        }
    }

    if converted_tools.is_empty() {
        None
    } else {
        Some(converted_tools)
    }
}

pub fn gemini_tool_config_from_claude(
    tool_choice: Option<BetaToolChoice>,
) -> Option<GeminiToolConfig> {
    let function_calling_config = match tool_choice {
        Some(BetaToolChoice::Auto(_)) => Some(GeminiFunctionCallingConfig {
            mode: Some(GeminiFunctionCallingMode::Auto),
            allowed_function_names: None,
        }),
        Some(BetaToolChoice::Any(_)) => Some(GeminiFunctionCallingConfig {
            mode: Some(GeminiFunctionCallingMode::Any),
            allowed_function_names: None,
        }),
        Some(BetaToolChoice::None(_)) => Some(GeminiFunctionCallingConfig {
            mode: Some(GeminiFunctionCallingMode::None),
            allowed_function_names: None,
        }),
        Some(BetaToolChoice::Tool(choice)) => Some(GeminiFunctionCallingConfig {
            mode: Some(GeminiFunctionCallingMode::Any),
            allowed_function_names: Some(vec![choice.name]),
        }),
        None => None,
    };

    function_calling_config.map(|config| GeminiToolConfig {
        function_calling_config: Some(config),
        retrieval_config: None,
    })
}

pub fn gemini_thinking_config_from_claude(
    thinking: Option<BetaThinkingConfigParam>,
    output_effort: Option<&BetaOutputEffort>,
) -> Option<GeminiThinkingConfig> {
    let mut thinking_config = GeminiThinkingConfig::default();
    let mut has_thinking_config = false;

    match thinking {
        Some(BetaThinkingConfigParam::Enabled(config)) => {
            thinking_config.include_thoughts = Some(true);
            thinking_config.thinking_budget =
                Some(i64::try_from(config.budget_tokens).unwrap_or(i64::MAX));
            has_thinking_config = true;
        }
        Some(BetaThinkingConfigParam::Disabled(_)) => {
            thinking_config.include_thoughts = Some(false);
            has_thinking_config = true;
        }
        Some(BetaThinkingConfigParam::Adaptive(_)) => {
            thinking_config.thinking_level = Some(GeminiThinkingLevel::Medium);
            has_thinking_config = true;
        }
        None => {}
    }

    if let Some(effort) = output_effort {
        thinking_config.thinking_level = Some(match effort {
            BetaOutputEffort::Low => GeminiThinkingLevel::Low,
            BetaOutputEffort::Medium => GeminiThinkingLevel::Medium,
            BetaOutputEffort::High => GeminiThinkingLevel::High,
            BetaOutputEffort::XHigh => GeminiThinkingLevel::High,
            BetaOutputEffort::Max => GeminiThinkingLevel::High,
        });
        has_thinking_config = true;
    }

    if has_thinking_config {
        Some(thinking_config)
    } else {
        None
    }
}
