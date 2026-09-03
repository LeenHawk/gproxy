use gproxy_protocol::{claude, openai};

use crate::TransformError;
use crate::common::tools::callers_to_claude;

pub(super) fn convert(tool: openai::ResponseTool) -> Result<claude::Tool, TransformError> {
    Ok(match tool {
        openai::ResponseTool::Function {
            name,
            parameters,
            strict,
            defer_loading,
            description,
            output_schema: _,
            allowed_callers,
            ..
        } => claude::Tool::Custom(crate::wire!(claude::CustomTool {
            input_schema: schema(parameters)?,
            name,
            type_: Some(claude::CustomToolType::Custom),
            description,
            eager_input_streaming: None,
            common: claude::ToolCommon {
                allowed_callers: callers_to_claude(allowed_callers),
                defer_loading,
                strict: match strict {
                    openai::ResponseFunctionStrict::Value(strict) => Some(strict),
                    openai::ResponseFunctionStrict::Null
                    | openai::ResponseFunctionStrict::Absent => None,
                    #[cfg(not(feature = "exhaustive"))]
                    _ =>
                        return Err(crate::TransformError::unsupported(
                            "protocol enum",
                            "unrecognized external variant"
                        )),
                },
                ..Default::default()
            },
            rest: Default::default(),
        })),
        ref custom @ openai::ResponseTool::Custom {
            ref name,
            ref description,
            defer_loading,
            ref allowed_callers,
            ..
        } => fallback(
            custom,
            name.clone(),
            description.clone(),
            defer_loading,
            allowed_callers.clone(),
        )?,
        ref namespace @ openai::ResponseTool::Namespace {
            ref description,
            ref name,
            ..
        } => fallback(
            namespace,
            name.clone(),
            Some(description.clone()),
            None,
            None,
        )?,
        openai::ResponseTool::LocalShell { .. } => bash(None),
        openai::ResponseTool::Shell {
            allowed_callers, ..
        } => bash(allowed_callers),
        openai::ResponseTool::ApplyPatch {
            allowed_callers,
            max_characters,
            ..
        } => text_editor(allowed_callers, max_characters),
        openai::ResponseTool::CodeExecution { .. } => code_execution(None),
        openai::ResponseTool::CodeInterpreter {
            allowed_callers, ..
        } => code_execution(allowed_callers),
        openai::ResponseTool::ComputerUsePreview {
            display_height,
            display_width,
            ..
        } => computer(display_width, display_height),
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
        } => web_search(filters, max_uses, user_location),
        openai::ResponseTool::WebSearchPreview { user_location, .. }
        | openai::ResponseTool::WebSearchPreview20250311 { user_location, .. } => {
            preview_search(user_location)
        }
        openai::ResponseTool::WebFetch {
            allowed_domains,
            blocked_domains,
            max_content_tokens,
            max_uses,
            ..
        } => web_fetch(
            allowed_domains,
            blocked_domains,
            max_content_tokens,
            max_uses,
        ),
        openai::ResponseTool::Memory { .. } => memory(),
        openai::ResponseTool::ToolSearch { execution, .. } => tool_search(execution),
        openai::ResponseTool::Mcp {
            server_label,
            server_url: None,
            ..
        } => claude::Tool::McpToolset(crate::wire!(claude::McpToolset {
            mcp_server_name: server_label,
            type_: claude::McpToolsetType::McpToolset,
            cache_control: None,
            configs: Default::default(),
            default_config: None,
            rest: Default::default(),
        })),
        unsupported @ (openai::ResponseTool::FileSearch { .. }
        | openai::ResponseTool::Computer { .. }
        | openai::ResponseTool::XSearch { .. }
        | openai::ResponseTool::CollectionsSearch { .. }
        | openai::ResponseTool::Mcp {
            server_url: Some(_),
            ..
        }
        | openai::ResponseTool::ImageGeneration { .. }
        | openai::ResponseTool::ProgrammaticToolCalling { .. }) => {
            return Err(TransformError::unsupported(
                "OpenAI Responses tool",
                serde_json::to_string(&unsupported)?,
            ));
        }
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    })
}

fn bash(callers: Option<Vec<openai::ToolCaller>>) -> claude::Tool {
    claude::Tool::Command(claude::CommandTool::Bash20250124(crate::wire!(
        claude::BashTool20250124 {
            name: claude::BashToolName::Bash,
            type_: claude::BashTool20250124Type::Bash20250124,
            common: claude::ToolCommon {
                allowed_callers: callers_to_claude(callers),
                ..Default::default()
            },
            rest: Default::default(),
        }
    )))
}

fn text_editor(
    callers: Option<Vec<openai::ToolCaller>>,
    max_characters: Option<u64>,
) -> claude::Tool {
    claude::Tool::TextEditor(claude::TextEditorTool::TextEditor20250728(crate::wire!(
        claude::TextEditorTool20250728 {
            name: claude::StrReplaceBasedEditToolName::StrReplaceBasedEditTool,
            type_: claude::TextEditorTool20250728Type::TextEditor20250728,
            max_characters,
            common: claude::ToolCommon {
                allowed_callers: callers_to_claude(callers),
                ..Default::default()
            },
            rest: Default::default(),
        }
    )))
}

fn code_execution(callers: Option<Vec<openai::ToolCaller>>) -> claude::Tool {
    claude::Tool::Command(claude::CommandTool::CodeExecution20260120(crate::wire!(
        claude::CodeExecutionTool20260120 {
            name: claude::CodeExecutionToolName::CodeExecution,
            type_: claude::CodeExecutionTool20260120Type::CodeExecution20260120,
            common: claude::ToolCommonWithoutInputExamples {
                allowed_callers: callers_to_claude(callers),
                ..Default::default()
            },
            rest: Default::default(),
        }
    )))
}

fn computer(width: u32, height: u32) -> claude::Tool {
    claude::Tool::Computer(claude::ComputerTool::Computer20250124(crate::wire!(
        claude::ComputerTool20250124 {
            display_height_px: u64::from(height),
            display_width_px: u64::from(width),
            name: claude::ComputerToolName::Computer,
            type_: claude::ComputerTool20250124Type::Computer20250124,
            display_number: None,
            common: Default::default(),
            rest: Default::default(),
        }
    )))
}

fn web_search(
    filters: Option<openai::WebSearchFilters>,
    max_uses: Option<u64>,
    user_location: Option<openai::WebSearchUserLocation>,
) -> claude::Tool {
    let (allowed_domains, blocked_domains) = filters
        .map(|filters| (filters.allowed_domains, filters.blocked_domains))
        .unwrap_or_default();
    claude_search(
        allowed_domains,
        blocked_domains,
        max_uses,
        user_location.map(location),
    )
}

fn preview_search(user_location: Option<openai::WebSearchPreviewUserLocation>) -> claude::Tool {
    claude_search(None, None, None, user_location.map(preview_location))
}

fn claude_search(
    allowed_domains: Option<Vec<String>>,
    blocked_domains: Option<Vec<String>>,
    max_uses: Option<u64>,
    user_location: Option<claude::UserLocation>,
) -> claude::Tool {
    claude::Tool::WebSearch(claude::WebSearchTool::WebSearch20260209(crate::wire!(
        claude::WebSearchTool20260209 {
            name: claude::WebSearchToolName::WebSearch,
            type_: claude::WebSearchTool20260209Type::WebSearch20260209,
            params: claude::WebSearchToolParams {
                allowed_domains,
                blocked_domains,
                max_uses,
                user_location,
                rest: Default::default(),
            },
            common: Default::default(),
            rest: Default::default(),
        }
    )))
}

fn location(location: openai::WebSearchUserLocation) -> claude::UserLocation {
    crate::wire!(claude::UserLocation {
        type_: claude::UserLocationType::Approximate,
        city: location.city,
        country: location.country,
        region: location.region,
        timezone: location.timezone,
        rest: Default::default(),
    })
}

fn preview_location(location: openai::WebSearchPreviewUserLocation) -> claude::UserLocation {
    crate::wire!(claude::UserLocation {
        type_: claude::UserLocationType::Approximate,
        city: location.city,
        country: location.country,
        region: location.region,
        timezone: location.timezone,
        rest: Default::default(),
    })
}

fn web_fetch(
    allowed_domains: Option<Vec<String>>,
    blocked_domains: Option<Vec<String>>,
    max_content_tokens: Option<u64>,
    max_uses: Option<u64>,
) -> claude::Tool {
    claude::Tool::WebFetch(claude::WebFetchTool::WebFetch20260209(crate::wire!(
        claude::WebFetchTool20260209 {
            name: claude::WebFetchToolName::WebFetch,
            type_: claude::WebFetchTool20260209Type::WebFetch20260209,
            params: claude::WebFetchToolParams {
                allowed_domains,
                blocked_domains,
                citations: None,
                max_content_tokens,
                max_uses,
                rest: Default::default(),
            },
            common: Default::default(),
            rest: Default::default(),
        }
    )))
}

fn memory() -> claude::Tool {
    claude::Tool::Command(claude::CommandTool::Memory20250818(crate::wire!(
        claude::MemoryTool20250818 {
            name: claude::MemoryToolName::Memory,
            type_: claude::MemoryTool20250818Type::Memory20250818,
            common: Default::default(),
            rest: Default::default(),
        }
    )))
}

fn tool_search(execution: Option<openai::ToolSearchExecution>) -> claude::Tool {
    if matches!(execution, Some(openai::ToolSearchExecution::Client)) {
        claude::Tool::Command(claude::CommandTool::ToolSearchRegex(crate::wire!(
            claude::ToolSearchRegexTool {
                name: claude::ToolSearchRegexToolName::ToolSearchRegex,
                type_: claude::ToolSearchRegexToolType::ToolSearchRegex,
                common: Default::default(),
                rest: Default::default(),
            }
        )))
    } else {
        claude::Tool::Command(claude::CommandTool::ToolSearchBm25(crate::wire!(
            claude::ToolSearchBm25Tool {
                name: claude::ToolSearchBm25ToolName::ToolSearchBm25,
                type_: claude::ToolSearchBm25ToolType::ToolSearchBm25,
                common: Default::default(),
                rest: Default::default(),
            }
        )))
    }
}

fn fallback(
    _tool: &openai::ResponseTool,
    name: String,
    description: Option<String>,
    defer_loading: Option<bool>,
    allowed_callers: Option<Vec<openai::ToolCaller>>,
) -> Result<claude::Tool, TransformError> {
    Ok(claude::Tool::Custom(crate::wire!(claude::CustomTool {
        input_schema: claude::JsonSchema {
            type_: claude::JsonSchemaObjectType::Known(claude::JsonSchemaObjectTypeKnown::Object),
            properties: Default::default(),
            required: Vec::new(),
            rest: Default::default(),
        },
        name,
        type_: Some(claude::CustomToolType::Custom),
        description,
        eager_input_streaming: None,
        common: claude::ToolCommon {
            allowed_callers: callers_to_claude(allowed_callers),
            defer_loading,
            ..Default::default()
        },
        rest: Default::default(),
    })))
}

fn schema(
    parameters: openai::ResponseFunctionParameters,
) -> Result<claude::JsonSchema, TransformError> {
    let schema = match parameters {
        openai::ResponseFunctionParameters::Schema(schema) => schema,
        openai::ResponseFunctionParameters::Null => Default::default(),
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    };
    Ok(serde_json::from_value(serde_json::Value::Object(schema))?)
}
