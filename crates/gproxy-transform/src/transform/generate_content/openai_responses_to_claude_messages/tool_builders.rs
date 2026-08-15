use serde_json::Value;

use crate::protocol::{claude, openai};

pub(super) fn custom_tool(
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

pub(super) fn json_object(value: Value) -> Option<openai::JsonSchema> {
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

pub(super) fn response_callers_to_claude(
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

pub(super) fn mcp_allowed_tools_to_claude(
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

pub(super) fn default_code_execution_tool() -> claude::Tool {
    claude::Tool::Command(claude::CommandTool::CodeExecution20260120(
        crate::protocol::wire!(claude::CodeExecutionTool20260120 {
            name: claude::CodeExecutionToolName::CodeExecution,
            type_: claude::CodeExecutionTool20260120Type::CodeExecution20260120,
            common: Default::default(),
        }),
    ))
}

pub(super) fn bash_tool(allowed_callers: Option<Vec<claude::ToolCaller>>) -> claude::Tool {
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

pub(super) fn text_editor_tool(
    allowed_callers: Option<Vec<claude::ToolCaller>>,
    max_characters: Option<u64>,
) -> claude::Tool {
    claude::Tool::TextEditor(claude::TextEditorTool::TextEditor20250728(
        crate::protocol::wire!(claude::TextEditorTool20250728 {
            name: claude::StrReplaceBasedEditToolName::StrReplaceBasedEditTool,
            type_: claude::TextEditorTool20250728Type::TextEditor20250728,
            max_characters,
            common: crate::protocol::wire!(claude::ToolCommon {
                allowed_callers,
                ..Default::default()
            }),
        }),
    ))
}

pub(super) fn memory_tool() -> claude::Tool {
    claude::Tool::Command(claude::CommandTool::Memory20250818(crate::protocol::wire!(
        claude::MemoryTool20250818 {
            name: claude::MemoryToolName::Memory,
            type_: claude::MemoryTool20250818Type::Memory20250818,
            common: Default::default(),
        }
    )))
}

pub(super) fn tool_search_tool(execution: Option<openai::ToolSearchExecution>) -> claude::Tool {
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

pub(super) fn computer_tool(display_width: u32, display_height: u32) -> claude::Tool {
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

pub(super) fn tool_activates_programmatic_calling(tool: &claude::Tool) -> bool {
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

pub(super) fn web_search_tool(
    allowed_domains: Option<Vec<String>>,
    blocked_domains: Option<Vec<String>>,
    max_uses: Option<u64>,
    user_location: Option<claude::UserLocation>,
) -> claude::Tool {
    claude::Tool::WebSearch(claude::WebSearchTool::WebSearch20260209(
        crate::protocol::wire!(claude::WebSearchTool20260209 {
            name: claude::WebSearchToolName::WebSearch,
            type_: claude::WebSearchTool20260209Type::WebSearch20260209,
            params: crate::protocol::wire!(claude::WebSearchToolParams {
                allowed_domains,
                blocked_domains,
                max_uses,
                user_location,
            }),
            common: Default::default(),
        }),
    ))
}

pub(super) fn web_fetch_tool(
    allowed_domains: Option<Vec<String>>,
    blocked_domains: Option<Vec<String>>,
    max_content_tokens: Option<u64>,
    max_uses: Option<u64>,
) -> claude::Tool {
    claude::Tool::WebFetch(claude::WebFetchTool::WebFetch20250910(
        crate::protocol::wire!(claude::WebFetchTool20250910 {
            name: claude::WebFetchToolName::WebFetch,
            type_: claude::WebFetchTool20250910Type::WebFetch20250910,
            params: crate::protocol::wire!(claude::WebFetchToolParams {
                allowed_domains,
                blocked_domains,
                citations: None,
                max_content_tokens,
                max_uses,
            }),
            common: Default::default(),
        }),
    ))
}

pub(super) fn response_location_to_claude(
    location: openai::WebSearchUserLocation,
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

pub(super) fn preview_location_to_claude(
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
