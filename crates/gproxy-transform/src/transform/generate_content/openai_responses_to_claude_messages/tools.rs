use serde_json::Value;

use crate::protocol::{claude, openai};

pub(super) struct ClaudeTools {
    pub tools: Option<Vec<claude::Tool>>,
    pub mcp_servers: Option<Vec<claude::McpServer>>,
    pub programmatic: bool,
}

pub(super) fn response_tools_to_claude(tools: Option<Vec<openai::ResponseTool>>) -> ClaudeTools {
    let mut output = Vec::new();
    let mut mcp_servers = Vec::new();
    let mut web_search = false;
    let mut programmatic_tool_calling = false;
    for tool in tools.into_iter().flatten() {
        match tool {
            openai::ResponseTool::Function {
                name,
                parameters,
                strict,
                defer_loading,
                description,
                allowed_callers,
                ..
            } => output.push(custom_tool(
                name,
                description,
                parameters,
                strict,
                defer_loading,
                response_callers_to_claude(allowed_callers),
            )),
            openai::ResponseTool::Custom {
                name,
                description,
                defer_loading,
                allowed_callers,
                ..
            } => {
                output.push(custom_tool(
                    name,
                    description,
                    Default::default(),
                    None,
                    defer_loading,
                    response_callers_to_claude(allowed_callers),
                ));
            }
            openai::ResponseTool::Namespace { tools, .. } => {
                output.extend(tools.into_iter().filter_map(namespace_tool_to_claude))
            }
            openai::ResponseTool::WebSearch {
                filters,
                user_location,
                ..
            }
            | openai::ResponseTool::WebSearch20250826 {
                filters,
                user_location,
                ..
            } => {
                crate::transform::context::report_lossy(
                    "tools[].web_search",
                    "one OpenAI web_search tool is expanded into Claude WebSearch and WebFetch definitions",
                );
                web_search = true;
                output.push(web_search_tool(
                    filters.and_then(|filters| filters.allowed_domains),
                    user_location.map(response_location_to_claude),
                ));
                output.push(web_fetch_tool());
            }
            openai::ResponseTool::WebSearchPreview { user_location, .. }
            | openai::ResponseTool::WebSearchPreview20250311 { user_location, .. } => {
                crate::transform::context::report_lossy(
                    "tools[].web_search_preview",
                    "one OpenAI web_search preview tool is expanded into Claude WebSearch and WebFetch definitions",
                );
                web_search = true;
                output.push(web_search_tool(
                    None,
                    user_location.map(preview_location_to_claude),
                ));
                output.push(web_fetch_tool());
            }
            openai::ResponseTool::XSearch { .. } => {
                web_search = true;
                output.push(web_search_tool(None, None));
            }
            openai::ResponseTool::CodeInterpreter { .. }
            | openai::ResponseTool::CodeExecution { .. } => {
                output.push(default_code_execution_tool())
            }
            openai::ResponseTool::ComputerUsePreview {
                display_height,
                display_width,
                ..
            } => output.push(computer_tool(display_width, display_height)),
            openai::ResponseTool::LocalShell { .. } => output.push(bash_tool(None)),
            openai::ResponseTool::Shell {
                allowed_callers, ..
            } => output.push(bash_tool(response_callers_to_claude(allowed_callers))),
            openai::ResponseTool::ApplyPatch {
                allowed_callers, ..
            } => output.push(text_editor_tool(response_callers_to_claude(
                allowed_callers,
            ))),
            openai::ResponseTool::ToolSearch { execution, .. } => {
                output.push(tool_search_tool(execution))
            }
            openai::ResponseTool::ProgrammaticToolCalling { .. } => {
                programmatic_tool_calling = true
            }
            openai::ResponseTool::Mcp {
                server_label,
                allowed_tools,
                authorization,
                server_url,
                ..
            } => {
                if let Some(url) = server_url {
                    mcp_servers.push(crate::protocol::wire!(claude::McpServer {
                        name: server_label,
                        type_: claude::McpServerType::Known(claude::McpServerTypeKnown::Url),
                        url,
                        authorization_token: authorization,
                        tool_configuration: allowed_tools.and_then(mcp_allowed_tools_to_claude),
                        extra: Default::default(),
                    }));
                } else {
                    output.push(claude::Tool::McpToolset(crate::protocol::wire!(
                        claude::McpToolset {
                            mcp_server_name: server_label,
                            type_: claude::McpToolsetType::McpToolset,
                            cache_control: None,
                            configs: Default::default(),
                            default_config: None,
                        }
                    )));
                }
            }
            _ => {}
        }
    }
    let programmatic = programmatic_tool_calling
        || web_search
        || output.iter().any(tool_activates_programmatic_calling);
    ClaudeTools {
        tools: (!output.is_empty()).then_some(output),
        mcp_servers: (!mcp_servers.is_empty()).then_some(mcp_servers),
        programmatic,
    }
}

pub(super) fn response_tool_choice_to_claude(
    choice: Option<openai::ResponseToolChoice>,
    parallel_tool_calls: Option<bool>,
) -> Option<claude::ToolChoice> {
    let disable_parallel_tool_use = parallel_tool_calls.map(|value| !value);
    match choice? {
        openai::ResponseToolChoice::Mode(openai::ToolChoiceMode::Auto) => Some(
            claude::ToolChoice::Auto(crate::protocol::wire!(claude::ToolChoiceAuto {
                type_: claude::ToolChoiceAutoType::Auto,
                disable_parallel_tool_use,
                extra: Default::default(),
            })),
        ),
        openai::ResponseToolChoice::Mode(openai::ToolChoiceMode::Required) => Some(
            claude::ToolChoice::Any(crate::protocol::wire!(claude::ToolChoiceAny {
                type_: claude::ToolChoiceAnyType::Any,
                disable_parallel_tool_use,
                extra: Default::default(),
            })),
        ),
        openai::ResponseToolChoice::Mode(openai::ToolChoiceMode::None) => Some(
            claude::ToolChoice::None(crate::protocol::wire!(claude::ToolChoiceNone {
                type_: claude::ToolChoiceNoneType::None,
                extra: Default::default(),
            })),
        ),
        openai::ResponseToolChoice::Function(choice) => {
            named_choice(choice.name, disable_parallel_tool_use)
        }
        openai::ResponseToolChoice::Custom(choice) => {
            named_choice(choice.name, disable_parallel_tool_use)
        }
        openai::ResponseToolChoice::Allowed(choice) => {
            let mut names = choice.tools.into_iter().filter_map(|tool| match tool {
                openai::ResponseAllowedTool::Function { name, .. }
                | openai::ResponseAllowedTool::Custom { name, .. } => Some(name),
                _ => None,
            });
            let first = names.next();
            if first.is_some() && names.next().is_none() {
                named_choice(first.unwrap_or_default(), disable_parallel_tool_use)
            } else {
                Some(claude::ToolChoice::Any(crate::protocol::wire!(
                    claude::ToolChoiceAny {
                        type_: claude::ToolChoiceAnyType::Any,
                        disable_parallel_tool_use,
                        extra: Default::default(),
                    }
                )))
            }
        }
        _ => None,
    }
}

fn named_choice(
    name: String,
    disable_parallel_tool_use: Option<bool>,
) -> Option<claude::ToolChoice> {
    Some(claude::ToolChoice::Tool(crate::protocol::wire!(
        claude::ToolChoiceTool {
            name,
            type_: claude::ToolChoiceToolType::Tool,
            disable_parallel_tool_use,
            extra: Default::default(),
        }
    )))
}

fn namespace_tool_to_claude(tool: openai::ResponseNamespaceTool) -> Option<claude::Tool> {
    match tool {
        openai::ResponseNamespaceTool::Function {
            name,
            description,
            parameters,
            strict,
            defer_loading,
            allowed_callers,
            ..
        } => Some(custom_tool(
            name,
            description,
            parameters.and_then(json_object).unwrap_or_default(),
            strict,
            defer_loading,
            response_callers_to_claude(allowed_callers),
        )),
        openai::ResponseNamespaceTool::Custom {
            name,
            description,
            defer_loading,
            allowed_callers,
            ..
        } => Some(custom_tool(
            name,
            description,
            Default::default(),
            None,
            defer_loading,
            response_callers_to_claude(allowed_callers),
        )),
        _ => None,
    }
}

fn custom_tool(
    name: String,
    description: Option<String>,
    parameters: openai::JsonSchema,
    strict: Option<bool>,
    defer_loading: Option<bool>,
    allowed_callers: Option<Vec<claude::ToolCaller>>,
) -> claude::Tool {
    claude::Tool::Custom(crate::protocol::wire!(claude::CustomTool {
        input_schema: claude_schema(parameters),
        name,
        type_: Some(claude::CustomToolType::Custom),
        description,
        eager_input_streaming: None,
        common: crate::protocol::wire!(claude::ToolCommon {
            strict,
            defer_loading,
            allowed_callers,
            ..Default::default()
        }),
    }))
}

fn json_object(value: Value) -> Option<openai::JsonSchema> {
    match value {
        Value::Object(map) => Some(map.into_iter().collect()),
        _ => None,
    }
}

fn claude_schema(schema: openai::JsonSchema) -> claude::JsonSchema {
    serde_json::from_value(Value::Object(schema.into_iter().collect())).unwrap_or_else(|_| {
        crate::protocol::wire!(claude::JsonSchema {
            type_: claude::JsonSchemaObjectType::Known(claude::JsonSchemaObjectTypeKnown::Object,),
            properties: Default::default(),
            required: Vec::new(),
            extra: Default::default(),
        })
    })
}

fn response_callers_to_claude(
    callers: Option<Vec<openai::ToolCaller>>,
) -> Option<Vec<claude::ToolCaller>> {
    let callers = callers?
        .into_iter()
        .map(|caller| match caller {
            openai::ToolCaller::Direct => claude::ToolCaller::Direct,
            openai::ToolCaller::Programmatic => claude::ToolCaller::CodeExecution20260120,
            _ => unreachable!(
                "new non-exhaustive protocol variant requires a lockstep transform update"
            ),
        })
        .collect::<Vec<_>>();
    (!callers.is_empty()).then_some(callers)
}

fn mcp_allowed_tools_to_claude(
    allowed_tools: openai::McpAllowedTools,
) -> Option<claude::McpToolConfiguration> {
    let names = match allowed_tools {
        openai::McpAllowedTools::Names(names) => names,
        openai::McpAllowedTools::Filter(filter) => filter.tool_names?,
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    };
    Some(crate::protocol::wire!(claude::McpToolConfiguration {
        allowed_tools: Some(names),
        enabled: None,
        extra: Default::default(),
    }))
}

fn default_code_execution_tool() -> claude::Tool {
    claude::Tool::Command(claude::CommandTool::CodeExecution20260120(
        crate::protocol::wire!(claude::CodeExecutionTool20260120 {
            name: claude::CodeExecutionToolName::CodeExecution,
            type_: claude::CodeExecutionTool20260120Type::CodeExecution20260120,
            common: Default::default(),
        }),
    ))
}

fn bash_tool(allowed_callers: Option<Vec<claude::ToolCaller>>) -> claude::Tool {
    claude::Tool::Command(claude::CommandTool::Bash20250124(crate::protocol::wire!(
        claude::BashTool20250124 {
            name: claude::BashToolName::Bash,
            type_: claude::BashTool20250124Type::Bash20250124,
            common: crate::protocol::wire!(claude::ToolCommon {
                allowed_callers,
                ..Default::default()
            }),
        }
    )))
}

fn text_editor_tool(allowed_callers: Option<Vec<claude::ToolCaller>>) -> claude::Tool {
    claude::Tool::TextEditor(claude::TextEditorTool::TextEditor20250728(
        crate::protocol::wire!(claude::TextEditorTool20250728 {
            name: claude::StrReplaceBasedEditToolName::StrReplaceBasedEditTool,
            type_: claude::TextEditorTool20250728Type::TextEditor20250728,
            max_characters: None,
            common: crate::protocol::wire!(claude::ToolCommon {
                allowed_callers,
                ..Default::default()
            }),
        }),
    ))
}

fn tool_search_tool(execution: Option<openai::ToolSearchExecution>) -> claude::Tool {
    let common = claude::ToolCommonWithoutInputExamples::default();
    if matches!(execution, Some(openai::ToolSearchExecution::Client)) {
        claude::Tool::Command(claude::CommandTool::ToolSearchRegex(
            crate::protocol::wire!(claude::ToolSearchRegexTool {
                name: claude::ToolSearchRegexToolName::ToolSearchRegex,
                type_: claude::ToolSearchRegexToolType::ToolSearchRegex,
                common,
            }),
        ))
    } else {
        claude::Tool::Command(claude::CommandTool::ToolSearchBm25(crate::protocol::wire!(
            claude::ToolSearchBm25Tool {
                name: claude::ToolSearchBm25ToolName::ToolSearchBm25,
                type_: claude::ToolSearchBm25ToolType::ToolSearchBm25,
                common,
            }
        )))
    }
}

fn computer_tool(display_width: u32, display_height: u32) -> claude::Tool {
    claude::Tool::Computer(claude::ComputerTool::Computer20250124(
        crate::protocol::wire!(claude::ComputerTool20250124 {
            display_height_px: u64::from(display_height),
            display_width_px: u64::from(display_width),
            name: claude::ComputerToolName::Computer,
            type_: claude::ComputerTool20250124Type::Computer20250124,
            display_number: None,
            common: Default::default(),
        }),
    ))
}

fn tool_activates_programmatic_calling(tool: &claude::Tool) -> bool {
    match tool {
        claude::Tool::Command(claude::CommandTool::CodeExecution20260120(_)) => true,
        claude::Tool::Command(claude::CommandTool::Bash20250124(tool)) => tool
            .common
            .allowed_callers
            .as_ref()
            .is_some_and(|callers| !callers.is_empty()),
        claude::Tool::TextEditor(claude::TextEditorTool::TextEditor20250728(tool)) => tool
            .common
            .allowed_callers
            .as_ref()
            .is_some_and(|callers| !callers.is_empty()),
        claude::Tool::Custom(custom) => custom
            .common
            .allowed_callers
            .as_ref()
            .is_some_and(|callers| !callers.is_empty()),
        _ => false,
    }
}

fn web_search_tool(
    allowed_domains: Option<Vec<String>>,
    user_location: Option<claude::UserLocation>,
) -> claude::Tool {
    claude::Tool::WebSearch(claude::WebSearchTool::WebSearch20260209(
        crate::protocol::wire!(claude::WebSearchTool20260209 {
            name: claude::WebSearchToolName::WebSearch,
            type_: claude::WebSearchTool20260209Type::WebSearch20260209,
            params: crate::protocol::wire!(claude::WebSearchToolParams {
                allowed_domains,
                blocked_domains: None,
                max_uses: None,
                user_location,
            }),
            common: Default::default(),
        }),
    ))
}

fn web_fetch_tool() -> claude::Tool {
    claude::Tool::WebFetch(claude::WebFetchTool::WebFetch20250910(
        crate::protocol::wire!(claude::WebFetchTool20250910 {
            name: claude::WebFetchToolName::WebFetch,
            type_: claude::WebFetchTool20250910Type::WebFetch20250910,
            params: crate::protocol::wire!(claude::WebFetchToolParams {
                allowed_domains: None,
                blocked_domains: None,
                citations: None,
                max_content_tokens: None,
                max_uses: None,
            }),
            common: Default::default(),
        }),
    ))
}

fn response_location_to_claude(location: openai::WebSearchUserLocation) -> claude::UserLocation {
    crate::protocol::wire!(claude::UserLocation {
        type_: claude::UserLocationType::Approximate,
        city: location.city,
        country: location.country,
        region: location.region,
        timezone: location.timezone,
        extra: Default::default(),
    })
}

fn preview_location_to_claude(
    location: openai::WebSearchPreviewUserLocation,
) -> claude::UserLocation {
    crate::protocol::wire!(claude::UserLocation {
        type_: claude::UserLocationType::Approximate,
        city: location.city,
        country: location.country,
        region: location.region,
        timezone: location.timezone,
        extra: Default::default(),
    })
}
