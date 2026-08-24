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
            rest,
        } => claude::Tool::Custom(claude::CustomTool {
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
                },
                ..Default::default()
            },
            rest,
        }),
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
        openai::ResponseTool::LocalShell { rest } => bash(None, rest),
        openai::ResponseTool::Shell {
            allowed_callers,
            rest,
            ..
        } => bash(allowed_callers, rest),
        openai::ResponseTool::ApplyPatch {
            allowed_callers,
            max_characters,
            rest,
        } => text_editor(allowed_callers, max_characters, rest),
        openai::ResponseTool::CodeExecution { rest } => code_execution(None, rest),
        openai::ResponseTool::CodeInterpreter {
            allowed_callers,
            rest,
            ..
        } => code_execution(allowed_callers, rest),
        openai::ResponseTool::ComputerUsePreview {
            display_height,
            display_width,
            rest,
            ..
        } => computer(display_width, display_height, rest),
        openai::ResponseTool::WebSearch {
            filters,
            max_uses,
            user_location,
            rest,
            ..
        }
        | openai::ResponseTool::WebSearch20250826 {
            filters,
            max_uses,
            user_location,
            rest,
            ..
        } => web_search(filters, max_uses, user_location, rest),
        openai::ResponseTool::WebSearchPreview {
            user_location,
            rest,
            ..
        }
        | openai::ResponseTool::WebSearchPreview20250311 {
            user_location,
            rest,
            ..
        } => preview_search(user_location, rest),
        openai::ResponseTool::WebFetch {
            allowed_domains,
            blocked_domains,
            max_content_tokens,
            max_uses,
            rest,
        } => web_fetch(
            allowed_domains,
            blocked_domains,
            max_content_tokens,
            max_uses,
            rest,
        ),
        openai::ResponseTool::Memory { rest } => memory(rest),
        openai::ResponseTool::ToolSearch {
            execution, rest, ..
        } => tool_search(execution, rest),
        openai::ResponseTool::Mcp {
            server_label,
            server_url: None,
            rest,
            ..
        } => claude::Tool::McpToolset(claude::McpToolset {
            mcp_server_name: server_label,
            type_: claude::McpToolsetType::McpToolset,
            cache_control: None,
            configs: Default::default(),
            default_config: None,
            rest,
        }),
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
    })
}

fn bash(callers: Option<Vec<openai::ToolCaller>>, rest: openai::Rest) -> claude::Tool {
    claude::Tool::Command(claude::CommandTool::Bash20250124(
        claude::BashTool20250124 {
            name: claude::BashToolName::Bash,
            type_: claude::BashTool20250124Type::Bash20250124,
            common: claude::ToolCommon {
                allowed_callers: callers_to_claude(callers),
                ..Default::default()
            },
            rest,
        },
    ))
}

fn text_editor(
    callers: Option<Vec<openai::ToolCaller>>,
    max_characters: Option<u64>,
    rest: openai::Rest,
) -> claude::Tool {
    claude::Tool::TextEditor(claude::TextEditorTool::TextEditor20250728(
        claude::TextEditorTool20250728 {
            name: claude::StrReplaceBasedEditToolName::StrReplaceBasedEditTool,
            type_: claude::TextEditorTool20250728Type::TextEditor20250728,
            max_characters,
            common: claude::ToolCommon {
                allowed_callers: callers_to_claude(callers),
                ..Default::default()
            },
            rest,
        },
    ))
}

fn code_execution(callers: Option<Vec<openai::ToolCaller>>, rest: openai::Rest) -> claude::Tool {
    claude::Tool::Command(claude::CommandTool::CodeExecution20260120(
        claude::CodeExecutionTool20260120 {
            name: claude::CodeExecutionToolName::CodeExecution,
            type_: claude::CodeExecutionTool20260120Type::CodeExecution20260120,
            common: claude::ToolCommonWithoutInputExamples {
                allowed_callers: callers_to_claude(callers),
                ..Default::default()
            },
            rest,
        },
    ))
}

fn computer(width: u32, height: u32, rest: openai::Rest) -> claude::Tool {
    claude::Tool::Computer(claude::ComputerTool::Computer20250124(
        claude::ComputerTool20250124 {
            display_height_px: u64::from(height),
            display_width_px: u64::from(width),
            name: claude::ComputerToolName::Computer,
            type_: claude::ComputerTool20250124Type::Computer20250124,
            display_number: None,
            common: Default::default(),
            rest,
        },
    ))
}

fn web_search(
    filters: Option<openai::WebSearchFilters>,
    max_uses: Option<u64>,
    user_location: Option<openai::WebSearchUserLocation>,
    rest: openai::Rest,
) -> claude::Tool {
    let (allowed_domains, blocked_domains) = filters
        .map(|filters| (filters.allowed_domains, filters.blocked_domains))
        .unwrap_or_default();
    claude_search(
        allowed_domains,
        blocked_domains,
        max_uses,
        user_location.map(location),
        rest,
    )
}

fn preview_search(
    user_location: Option<openai::WebSearchPreviewUserLocation>,
    rest: openai::Rest,
) -> claude::Tool {
    claude_search(None, None, None, user_location.map(preview_location), rest)
}

fn claude_search(
    allowed_domains: Option<Vec<String>>,
    blocked_domains: Option<Vec<String>>,
    max_uses: Option<u64>,
    user_location: Option<claude::UserLocation>,
    rest: openai::Rest,
) -> claude::Tool {
    claude::Tool::WebSearch(claude::WebSearchTool::WebSearch20260209(
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
            rest,
        },
    ))
}

fn location(location: openai::WebSearchUserLocation) -> claude::UserLocation {
    claude::UserLocation {
        type_: claude::UserLocationType::Approximate,
        city: location.city,
        country: location.country,
        region: location.region,
        timezone: location.timezone,
        rest: location.rest,
    }
}

fn preview_location(location: openai::WebSearchPreviewUserLocation) -> claude::UserLocation {
    claude::UserLocation {
        type_: claude::UserLocationType::Approximate,
        city: location.city,
        country: location.country,
        region: location.region,
        timezone: location.timezone,
        rest: location.rest,
    }
}

fn web_fetch(
    allowed_domains: Option<Vec<String>>,
    blocked_domains: Option<Vec<String>>,
    max_content_tokens: Option<u64>,
    max_uses: Option<u64>,
    rest: openai::Rest,
) -> claude::Tool {
    claude::Tool::WebFetch(claude::WebFetchTool::WebFetch20260209(
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
            rest,
        },
    ))
}

fn memory(rest: openai::Rest) -> claude::Tool {
    claude::Tool::Command(claude::CommandTool::Memory20250818(
        claude::MemoryTool20250818 {
            name: claude::MemoryToolName::Memory,
            type_: claude::MemoryTool20250818Type::Memory20250818,
            common: Default::default(),
            rest,
        },
    ))
}

fn tool_search(execution: Option<openai::ToolSearchExecution>, rest: openai::Rest) -> claude::Tool {
    if matches!(execution, Some(openai::ToolSearchExecution::Client)) {
        claude::Tool::Command(claude::CommandTool::ToolSearchRegex(
            claude::ToolSearchRegexTool {
                name: claude::ToolSearchRegexToolName::ToolSearchRegex,
                type_: claude::ToolSearchRegexToolType::ToolSearchRegex,
                common: Default::default(),
                rest,
            },
        ))
    } else {
        claude::Tool::Command(claude::CommandTool::ToolSearchBm25(
            claude::ToolSearchBm25Tool {
                name: claude::ToolSearchBm25ToolName::ToolSearchBm25,
                type_: claude::ToolSearchBm25ToolType::ToolSearchBm25,
                common: Default::default(),
                rest,
            },
        ))
    }
}

fn fallback(
    tool: &openai::ResponseTool,
    name: String,
    description: Option<String>,
    defer_loading: Option<bool>,
    allowed_callers: Option<Vec<openai::ToolCaller>>,
) -> Result<claude::Tool, TransformError> {
    let mut rest = claude::JsonObject::new();
    rest.insert("openai_native_tool".into(), serde_json::to_value(tool)?);
    Ok(claude::Tool::Custom(claude::CustomTool {
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
        rest,
    }))
}

fn schema(
    parameters: openai::ResponseFunctionParameters,
) -> Result<claude::JsonSchema, TransformError> {
    let openai::ResponseFunctionParameters::Schema(schema) = parameters else {
        return Err(TransformError::shape(
            "OpenAI function tool",
            "parameters are null",
        ));
    };
    Ok(serde_json::from_value(serde_json::Value::Object(schema))?)
}
