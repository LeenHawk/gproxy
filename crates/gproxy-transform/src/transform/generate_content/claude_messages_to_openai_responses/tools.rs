use serde_json::Value;

use crate::protocol::{claude, openai};

pub(super) fn claude_tools_to_responses(
    tools: Option<Vec<claude::Tool>>,
    mcp_servers: Option<Vec<claude::McpServer>>,
) -> Option<Vec<openai::ResponseTool>> {
    let mut output = tools
        .into_iter()
        .flatten()
        .filter_map(|tool| match tool {
            claude::Tool::Custom(tool) => {
                let common = tool.common;
                Some(openai::ResponseTool::Function {
                    name: tool.name,
                    parameters: json_schema(tool.input_schema),
                    strict: common.strict,
                    defer_loading: common.defer_loading,
                    description: tool.description,
                    allowed_callers: claude_callers_to_responses(common.allowed_callers),
                    async_: None,
                    extra: Default::default(),
                })
            }
            claude::Tool::WebSearch(tool) => Some(claude_web_search_to_response(tool)),
            claude::Tool::WebFetch(tool) => Some(claude_web_fetch_to_response(tool)),
            claude::Tool::Computer(_) => Some(openai::ResponseTool::Computer {
                extra: Default::default(),
            }),
            claude::Tool::TextEditor(tool) => Some(claude_text_editor_to_response(tool)),
            claude::Tool::Command(command) => claude_command_to_response(command),
            claude::Tool::McpToolset(toolset) => Some(openai::ResponseTool::Mcp {
                server_label: toolset.mcp_server_name,
                allowed_tools: None,
                authorization: None,
                connector_id: None,
                defer_loading: None,
                headers: None,
                require_approval: None,
                server_description: None,
                server_url: None,
                tunnel_id: None,
                allowed_callers: None,
                extra: Default::default(),
            }),
            _ => None,
        })
        .collect::<Vec<_>>();
    merge_duplicate_web_search_tools(&mut output);
    for server in mcp_servers.into_iter().flatten() {
        let allowed_tools = server
            .tool_configuration
            .and_then(|config| config.allowed_tools)
            .map(openai::McpAllowedTools::Names);
        if let Some(openai::ResponseTool::Mcp {
            allowed_tools: existing_allowed_tools,
            authorization,
            server_url,
            ..
        }) = output.iter_mut().find(|tool| {
            matches!(
                tool,
                openai::ResponseTool::Mcp { server_label, .. }
                    if server_label == &server.name
            )
        }) {
            *existing_allowed_tools = allowed_tools;
            *authorization = server.authorization_token;
            *server_url = Some(server.url);
        } else {
            output.push(openai::ResponseTool::Mcp {
                server_label: server.name,
                allowed_tools,
                authorization: server.authorization_token,
                connector_id: None,
                defer_loading: None,
                headers: None,
                require_approval: None,
                server_description: None,
                server_url: Some(server.url),
                tunnel_id: None,
                allowed_callers: None,
                extra: Default::default(),
            });
        }
    }
    (!output.is_empty()).then_some(output)
}

pub(super) fn claude_tool_choice_to_responses(
    choice: Option<claude::ToolChoice>,
) -> Option<openai::ResponseToolChoice> {
    match choice? {
        claude::ToolChoice::Auto(_) => Some(openai::ResponseToolChoice::Mode(
            openai::ToolChoiceMode::Auto,
        )),
        claude::ToolChoice::Any(_) => Some(openai::ResponseToolChoice::Mode(
            openai::ToolChoiceMode::Required,
        )),
        claude::ToolChoice::None(_) => Some(openai::ResponseToolChoice::Mode(
            openai::ToolChoiceMode::None,
        )),
        claude::ToolChoice::Tool(choice) => Some(openai::ResponseToolChoice::Function(
            crate::protocol::wire!(openai::ResponseFunctionToolChoice {
                type_: openai::FunctionToolChoiceType::Function,
                name: choice.name,
                extra: Default::default(),
            }),
        )),
        claude::ToolChoice::Unknown(_) => None,
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

fn json_schema(schema: claude::JsonSchema) -> openai::JsonSchema {
    match serde_json::to_value(schema).unwrap_or(Value::Object(Default::default())) {
        Value::Object(map) => map.into_iter().collect(),
        _ => Default::default(),
    }
}

fn claude_callers_to_responses(
    callers: Option<Vec<claude::ToolCaller>>,
) -> Option<Vec<openai::ToolCaller>> {
    let callers = callers?
        .into_iter()
        .map(|caller| match caller {
            claude::ToolCaller::Direct => openai::ToolCaller::Direct,
            claude::ToolCaller::CodeExecution20250825
            | claude::ToolCaller::CodeExecution20260120
            | claude::ToolCaller::CodeExecution20260521 => openai::ToolCaller::Programmatic,
            _ => unreachable!(
                "new non-exhaustive protocol variant requires a lockstep transform update"
            ),
        })
        .collect::<Vec<_>>();
    (!callers.is_empty()).then_some(callers)
}

fn claude_web_search_to_response(tool: claude::WebSearchTool) -> openai::ResponseTool {
    let params = match tool {
        claude::WebSearchTool::WebSearch20250305(tool) => tool.params,
        claude::WebSearchTool::WebSearch20260209(tool) => tool.params,
        claude::WebSearchTool::WebSearch20260318(tool) => tool.params,
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    };
    if params.blocked_domains.is_some() {
        crate::transform::context::report_unsupported(
            "tools[].web_search.blocked_domains",
            "OpenAI Responses web_search filters only support allowed_domains",
        );
    }
    if params.max_uses.is_some() {
        crate::transform::context::report_unsupported(
            "tools[].web_search.max_uses",
            "OpenAI Responses web_search has no per-tool max_uses field",
        );
    }
    openai::ResponseTool::WebSearch {
        filters: params.allowed_domains.map(|allowed_domains| {
            crate::protocol::wire!(openai::WebSearchFilters {
                allowed_domains: Some(allowed_domains),
                extra: Default::default(),
            })
        }),
        max_uses: None,
        search_context_size: None,
        user_location: params.user_location.map(claude_location_to_response),
        extra: Default::default(),
    }
}

fn claude_web_fetch_to_response(tool: claude::WebFetchTool) -> openai::ResponseTool {
    let (params, use_cache, response_inclusion) = match tool {
        claude::WebFetchTool::WebFetch20250910(tool) => (tool.params, None, None),
        claude::WebFetchTool::WebFetch20260209(tool) => (tool.params, None, None),
        claude::WebFetchTool::WebFetch20260309(tool) => (tool.params, tool.use_cache, None),
        claude::WebFetchTool::WebFetch20260318(tool) => {
            (tool.params, tool.use_cache, tool.response_inclusion)
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    };
    crate::transform::context::report_lossy(
        "tools[].web_fetch",
        "OpenAI Responses has no WebFetch tool definition; it is approximated as WebSearch",
    );
    report_web_fetch_unsupported_fields(&params, use_cache, response_inclusion);
    openai::ResponseTool::WebSearch {
        filters: params.allowed_domains.map(|allowed_domains| {
            crate::protocol::wire!(openai::WebSearchFilters {
                allowed_domains: Some(allowed_domains),
                extra: Default::default(),
            })
        }),
        max_uses: None,
        search_context_size: None,
        user_location: None,
        extra: Default::default(),
    }
}

fn report_web_fetch_unsupported_fields(
    params: &claude::WebFetchToolParams,
    use_cache: Option<bool>,
    response_inclusion: Option<claude::ResponseInclusion>,
) {
    let fields = [
        (
            params.blocked_domains.is_some(),
            "tools[].web_fetch.blocked_domains",
        ),
        (params.citations.is_some(), "tools[].web_fetch.citations"),
        (
            params.max_content_tokens.is_some(),
            "tools[].web_fetch.max_content_tokens",
        ),
        (params.max_uses.is_some(), "tools[].web_fetch.max_uses"),
        (use_cache.is_some(), "tools[].web_fetch.use_cache"),
        (
            response_inclusion.is_some(),
            "tools[].web_fetch.response_inclusion",
        ),
    ];
    for (present, field) in fields {
        if present {
            crate::transform::context::report_unsupported(
                field,
                "OpenAI Responses web_search has no corresponding field",
            );
        }
    }
}

fn claude_location_to_response(location: claude::UserLocation) -> openai::WebSearchUserLocation {
    crate::protocol::wire!(openai::WebSearchUserLocation {
        city: location.city,
        country: location.country,
        region: location.region,
        timezone: location.timezone,
        type_: Some(openai::ApproximateLocationType::Approximate),
        extra: Default::default(),
    })
}

fn claude_command_to_response(command: claude::CommandTool) -> Option<openai::ResponseTool> {
    match command {
        claude::CommandTool::Bash20241022(tool) => Some(response_shell(tool.common)),
        claude::CommandTool::Bash20250124(tool) => Some(response_shell(tool.common)),
        claude::CommandTool::CodeExecution20250522(tool) => {
            Some(response_code_interpreter(tool.common.allowed_callers))
        }
        claude::CommandTool::CodeExecution20250825(tool) => {
            Some(response_code_interpreter(tool.common.allowed_callers))
        }
        claude::CommandTool::CodeExecution20260120(tool) => {
            Some(response_code_interpreter(tool.common.allowed_callers))
        }
        claude::CommandTool::CodeExecution20260521(tool) => {
            Some(response_code_interpreter(tool.common.allowed_callers))
        }
        claude::CommandTool::ToolSearchBm25(_) => Some(response_tool_search(
            openai::ToolSearchExecution::Server,
            "Search deferred tools using natural-language relevance",
        )),
        claude::CommandTool::ToolSearchRegex(_) => Some(response_tool_search(
            openai::ToolSearchExecution::Client,
            "Search deferred tools using a regular expression",
        )),
        claude::CommandTool::Memory20250818(tool) => {
            crate::transform::context::report_lossy(
                "tools[].memory",
                "OpenAI Responses has no native Memory tool; it is approximated as a function",
            );
            Some(openai::ResponseTool::Function {
                name: "memory".to_owned(),
                parameters: Default::default(),
                strict: tool.common.strict,
                defer_loading: tool.common.defer_loading,
                description: Some("Read or update persistent agent memory".to_owned()),
                allowed_callers: claude_callers_to_responses(tool.common.allowed_callers),
                async_: None,
                extra: Default::default(),
            })
        }
        _ => None,
    }
}

fn response_shell(common: claude::ToolCommon) -> openai::ResponseTool {
    openai::ResponseTool::Shell {
        environment: None,
        allowed_callers: claude_callers_to_responses(common.allowed_callers),
        extra: Default::default(),
    }
}

fn response_code_interpreter(
    allowed_callers: Option<Vec<claude::ToolCaller>>,
) -> openai::ResponseTool {
    openai::ResponseTool::CodeInterpreter {
        container: openai::CodeInterpreterContainer::Auto(crate::protocol::wire!(
            openai::CodeInterpreterAutoContainer {
                type_: openai::CodeInterpreterContainerType::Auto,
                file_ids: None,
                memory_limit: None,
                network_policy: None,
                extra: Default::default(),
            }
        )),
        allowed_callers: claude_callers_to_responses(allowed_callers),
        extra: Default::default(),
    }
}

fn response_tool_search(
    execution: openai::ToolSearchExecution,
    description: &str,
) -> openai::ResponseTool {
    openai::ResponseTool::ToolSearch {
        description: Some(description.to_owned()),
        execution: Some(execution),
        parameters: None,
        extra: Default::default(),
    }
}

fn claude_text_editor_to_response(tool: claude::TextEditorTool) -> openai::ResponseTool {
    let (allowed_callers, max_characters) = match tool {
        claude::TextEditorTool::TextEditor20241022(tool) => (tool.common.allowed_callers, None),
        claude::TextEditorTool::TextEditor20250124(tool) => (tool.common.allowed_callers, None),
        claude::TextEditorTool::TextEditor20250429(tool) => (tool.common.allowed_callers, None),
        claude::TextEditorTool::TextEditor20250728(tool) => {
            (tool.common.allowed_callers, tool.max_characters)
        }
        _ => (None, None),
    };
    if max_characters.is_some() {
        crate::transform::context::report_unsupported(
            "tools[].text_editor.max_characters",
            "OpenAI Responses apply_patch has no max_characters field",
        );
    }
    openai::ResponseTool::ApplyPatch {
        allowed_callers: claude_callers_to_responses(allowed_callers),
        max_characters: None,
        extra: Default::default(),
    }
}

fn merge_duplicate_web_search_tools(tools: &mut Vec<openai::ResponseTool>) {
    let mut first = None;
    let mut index = 0;
    while index < tools.len() {
        if matches!(tools[index], openai::ResponseTool::WebSearch { .. }) {
            if let Some(first_index) = first {
                let duplicate = tools.remove(index);
                crate::transform::context::report_lossy(
                    "tools[].web_search",
                    "multiple Claude WebSearch/WebFetch definitions collapse into one OpenAI web_search tool",
                );
                merge_web_search_tool(&mut tools[first_index], duplicate);
                continue;
            }
            first = Some(index);
        }
        index += 1;
    }
}

fn merge_web_search_tool(target: &mut openai::ResponseTool, source: openai::ResponseTool) {
    let (
        openai::ResponseTool::WebSearch {
            filters,
            user_location,
            ..
        },
        openai::ResponseTool::WebSearch {
            filters: source_filters,
            user_location: source_location,
            ..
        },
    ) = (target, source)
    else {
        return;
    };
    if filters.is_none() {
        *filters = source_filters;
    }
    if user_location.is_none() {
        *user_location = source_location;
    }
}
