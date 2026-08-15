use crate::protocol::{claude, openai};

use super::tool_builders::*;

pub(super) struct ClaudeTools {
    pub tools: Option<Vec<claude::Tool>>,
    pub mcp_servers: Option<Vec<claude::McpServer>>,
    pub programmatic: bool,
}

pub(super) fn response_tools_to_claude(tools: Option<Vec<openai::ResponseTool>>) -> ClaudeTools {
    let tools: Vec<openai::ResponseTool> = tools.into_iter().flatten().collect();
    // An explicit web_fetch definition wins over the implicit expansion from
    // web_search; Claude rejects duplicate tool names.
    let explicit_web_fetch = tools
        .iter()
        .any(|tool| matches!(tool, openai::ResponseTool::WebFetch { .. }));
    let mut output = Vec::new();
    let mut mcp_servers = Vec::new();
    let mut web_search = false;
    let mut programmatic_tool_calling = false;
    for tool in tools {
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
                max_uses,
                user_location,
                ..
            }
            | openai::ResponseTool::WebSearch20250826 {
                filters,
                max_uses,
                user_location,
                ..
            } => {
                web_search = true;
                let (allowed_domains, blocked_domains) = filters
                    .map(|filters| (filters.allowed_domains, filters.blocked_domains))
                    .unwrap_or_default();
                output.push(web_search_tool(
                    allowed_domains,
                    blocked_domains,
                    max_uses,
                    user_location.map(response_location_to_claude),
                ));
                if !explicit_web_fetch {
                    crate::transform::context::report_lossy(
                        "tools[].web_search",
                        "one OpenAI web_search tool is expanded into Claude WebSearch and WebFetch definitions",
                    );
                    output.push(web_fetch_tool(None, None, None, None));
                }
            }
            openai::ResponseTool::WebSearchPreview { user_location, .. }
            | openai::ResponseTool::WebSearchPreview20250311 { user_location, .. } => {
                web_search = true;
                output.push(web_search_tool(
                    None,
                    None,
                    None,
                    user_location.map(preview_location_to_claude),
                ));
                if !explicit_web_fetch {
                    crate::transform::context::report_lossy(
                        "tools[].web_search_preview",
                        "one OpenAI web_search preview tool is expanded into Claude WebSearch and WebFetch definitions",
                    );
                    output.push(web_fetch_tool(None, None, None, None));
                }
            }
            openai::ResponseTool::WebFetch {
                allowed_domains,
                blocked_domains,
                max_content_tokens,
                max_uses,
                ..
            } => output.push(web_fetch_tool(
                allowed_domains,
                blocked_domains,
                max_content_tokens,
                max_uses,
            )),
            openai::ResponseTool::XSearch { .. } => {
                web_search = true;
                output.push(web_search_tool(None, None, None, None));
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
                allowed_callers,
                max_characters,
                ..
            } => output.push(text_editor_tool(
                response_callers_to_claude(allowed_callers),
                max_characters,
            )),
            openai::ResponseTool::Memory { .. } => output.push(memory_tool()),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 显式 web_fetch 定义要抑制 web_search 的隐式展开，否则 Claude 收到重名工具。
    #[test]
    fn explicit_web_fetch_overrides_web_search_expansion() {
        let tools = response_tools_to_claude(Some(vec![
            openai::ResponseTool::WebSearch {
                filters: Some(crate::protocol::wire!(openai::WebSearchFilters {
                    allowed_domains: Some(vec!["a.example".into()]),
                    blocked_domains: Some(vec!["b.example".into()]),
                    extra: Default::default(),
                })),
                max_uses: Some(3),
                search_context_size: None,
                user_location: None,
                extra: Default::default(),
            },
            openai::ResponseTool::WebFetch {
                allowed_domains: None,
                blocked_domains: None,
                max_content_tokens: Some(2048),
                max_uses: Some(5),
                extra: Default::default(),
            },
        ]))
        .tools
        .unwrap();
        assert_eq!(tools.len(), 2);
        let claude::Tool::WebSearch(claude::WebSearchTool::WebSearch20260209(search)) = &tools[0]
        else {
            panic!("expected web_search")
        };
        assert_eq!(search.params.max_uses, Some(3));
        assert_eq!(
            search.params.blocked_domains.as_deref(),
            Some(&["b.example".to_owned()][..])
        );
        let claude::Tool::WebFetch(claude::WebFetchTool::WebFetch20250910(fetch)) = &tools[1]
        else {
            panic!("expected web_fetch")
        };
        assert_eq!(fetch.params.max_uses, Some(5));
        assert_eq!(fetch.params.max_content_tokens, Some(2048));
    }
}
