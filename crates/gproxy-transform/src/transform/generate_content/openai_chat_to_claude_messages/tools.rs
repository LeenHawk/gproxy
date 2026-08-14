use std::collections::BTreeMap;

use serde_json::Value;

use crate::protocol::{claude, openai};

pub(super) fn chat_tool_call_to_claude(
    call: openai::ChatToolCall,
    tool_ids: &mut BTreeMap<String, String>,
) -> claude::ContentBlockParam {
    match call {
        openai::ChatToolCall::Function { id, function, .. } => tool_use_block(
            normalized_tool_id(id, tool_ids),
            function.name,
            parse_json_object(function.arguments),
        ),
        openai::ChatToolCall::Custom { id, custom, .. } => tool_use_block(
            normalized_tool_id(id, tool_ids),
            custom.name,
            parse_json_object(custom.input),
        ),
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

pub(super) fn chat_tool_call_to_claude_response(
    call: openai::ChatToolCall,
) -> claude::ContentBlock {
    match call {
        openai::ChatToolCall::Function { id, function, .. } => {
            response_tool_use_block(id, function.name, parse_json_object(function.arguments))
        }
        openai::ChatToolCall::Custom { id, custom, .. } => {
            response_tool_use_block(id, custom.name, parse_json_object(custom.input))
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

pub(super) fn tool_use_block(
    id: String,
    name: String,
    input: claude::JsonObject,
) -> claude::ContentBlockParam {
    claude::ContentBlockParam::ToolUse(crate::protocol::wire!(claude::ToolUseBlock {
        id,
        input,
        name,
        type_: claude::ToolUseBlockType::ToolUse,
        cache_control: None,
        caller: None,
    }))
}

pub(super) fn response_tool_use_block(
    id: String,
    name: String,
    input: claude::JsonObject,
) -> claude::ContentBlock {
    claude::ContentBlock::ToolUse(crate::protocol::wire!(claude::ResponseToolUseBlock {
        id,
        input,
        name,
        type_: claude::ToolUseBlockType::ToolUse,
        caller: None,
        extra: Default::default(),
    }))
}

pub(super) fn parse_json_object(text: String) -> claude::JsonObject {
    serde_json::from_str::<claude::JsonObject>(&text).unwrap_or_default()
}

pub(super) fn normalized_tool_id(
    original: String,
    mappings: &mut BTreeMap<String, String>,
) -> String {
    if let Some(mapped) = mappings.get(&original) {
        return mapped.clone();
    }
    let mapped = if original.starts_with("toolu_") {
        original.clone()
    } else {
        format!("toolu_{}", sanitize_tool_id(&original))
    };
    mappings.insert(original, mapped.clone());
    mapped
}

fn sanitize_tool_id(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_owned();
    if sanitized.is_empty() {
        "generated".to_owned()
    } else {
        sanitized
    }
}

pub(in crate::transform::generate_content) fn chat_tools_to_claude(
    tools: Vec<openai::ChatTool>,
) -> Vec<claude::Tool> {
    tools
        .into_iter()
        .map(|tool| match tool {
            openai::ChatTool::Function { function, .. } => {
                claude::Tool::Custom(crate::protocol::wire!(claude::CustomTool {
                    input_schema: openai_schema_to_claude(function.parameters),
                    name: function.name,
                    type_: Some(claude::CustomToolType::Custom),
                    description: function.description,
                    eager_input_streaming: None,
                    common: crate::protocol::wire!(claude::ToolCommon {
                        strict: function.strict,
                        ..Default::default()
                    }),
                }))
            }
            openai::ChatTool::Custom { custom, .. } => {
                claude::Tool::Custom(crate::protocol::wire!(claude::CustomTool {
                    input_schema: empty_claude_schema(),
                    name: custom.name,
                    type_: Some(claude::CustomToolType::Custom),
                    description: custom.description,
                    eager_input_streaming: None,
                    common: Default::default(),
                }))
            }
            _ => unreachable!(
                "new non-exhaustive protocol variant requires a lockstep transform update"
            ),
        })
        .collect()
}

fn openai_schema_to_claude(parameters: Option<openai::JsonSchema>) -> claude::JsonSchema {
    let Some(mut parameters) = parameters else {
        return empty_claude_schema();
    };
    let properties = parameters
        .remove("properties")
        .and_then(|value| match value {
            Value::Object(map) => Some(map.into_iter().collect()),
            _ => None,
        })
        .unwrap_or_default();
    let required = parameters
        .remove("required")
        .and_then(|value| match value {
            Value::Array(values) => Some(
                values
                    .into_iter()
                    .filter_map(|value| value.as_str().map(str::to_owned))
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default();
    parameters.remove("type");
    crate::protocol::wire!(claude::JsonSchema {
        type_: claude::JsonSchemaObjectType::Known(claude::JsonSchemaObjectTypeKnown::Object),
        properties,
        required,
        extra: Default::default(),
    })
}

fn empty_claude_schema() -> claude::JsonSchema {
    crate::protocol::wire!(claude::JsonSchema {
        type_: claude::JsonSchemaObjectType::Known(claude::JsonSchemaObjectTypeKnown::Object),
        properties: Default::default(),
        required: Vec::new(),
        extra: Default::default(),
    })
}

pub(in crate::transform::generate_content) fn default_web_search_tool() -> claude::Tool {
    claude::Tool::WebSearch(claude::WebSearchTool::WebSearch20260209(
        crate::protocol::wire!(claude::WebSearchTool20260209 {
            name: claude::WebSearchToolName::WebSearch,
            type_: claude::WebSearchTool20260209Type::WebSearch20260209,
            params: crate::protocol::wire!(claude::WebSearchToolParams {
                allowed_domains: None,
                blocked_domains: None,
                max_uses: None,
                user_location: None,
            }),
            common: Default::default(),
        }),
    ))
}

/// Anthropic rejects `tool_choice.disable_parallel_tool_use: true` when the request
/// activates programmatic tool calling: 2026-generation server tools
/// (`web_search`/`web_fetch` `2026xxxx`) and tools carrying `allowed_callers` trigger
/// it; 2025-generation server tools and plain custom tools do not, and
/// `disable_parallel_tool_use: false` is always accepted. Probed against
/// api.anthropic.com (claude-opus-4-8, 2026-07-29).
pub(in crate::transform::generate_content) fn tools_activate_programmatic_calling(
    tools: &[claude::Tool],
) -> bool {
    tools.iter().any(|tool| match tool {
        claude::Tool::WebSearch(
            claude::WebSearchTool::WebSearch20260209(_)
            | claude::WebSearchTool::WebSearch20260318(_),
        )
        | claude::Tool::WebFetch(
            claude::WebFetchTool::WebFetch20260209(_)
            | claude::WebFetchTool::WebFetch20260309(_)
            | claude::WebFetchTool::WebFetch20260318(_),
        )
        | claude::Tool::Command(
            claude::CommandTool::CodeExecution20250825(_)
            | claude::CommandTool::CodeExecution20260120(_)
            | claude::CommandTool::CodeExecution20260521(_),
        ) => true,
        claude::Tool::Custom(custom) => custom
            .common
            .allowed_callers
            .as_ref()
            .is_some_and(|callers| !callers.is_empty()),
        _ => false,
    })
}

pub(in crate::transform::generate_content) fn chat_tool_choice_to_claude(
    choice: Option<openai::ChatToolChoice>,
    parallel_tool_calls: Option<bool>,
) -> Option<claude::ToolChoice> {
    let disable_parallel_tool_use = parallel_tool_calls.map(|value| !value);
    match choice? {
        openai::ChatToolChoice::Mode(openai::ToolChoiceMode::Auto) => Some(
            claude::ToolChoice::Auto(crate::protocol::wire!(claude::ToolChoiceAuto {
                type_: claude::ToolChoiceAutoType::Auto,
                disable_parallel_tool_use,
                extra: Default::default(),
            })),
        ),
        openai::ChatToolChoice::Mode(openai::ToolChoiceMode::Required) => Some(
            claude::ToolChoice::Any(crate::protocol::wire!(claude::ToolChoiceAny {
                type_: claude::ToolChoiceAnyType::Any,
                disable_parallel_tool_use,
                extra: Default::default(),
            })),
        ),
        openai::ChatToolChoice::Mode(openai::ToolChoiceMode::None) => Some(
            claude::ToolChoice::None(crate::protocol::wire!(claude::ToolChoiceNone {
                type_: claude::ToolChoiceNoneType::None,
                extra: Default::default(),
            })),
        ),
        openai::ChatToolChoice::Allowed(_) => None,
        openai::ChatToolChoice::Named(named) => {
            let name = match named {
                openai::ChatNamedToolChoice::Function { function, .. } => function.name,
                openai::ChatNamedToolChoice::Custom { custom, .. } => custom.name,
                _ => unreachable!(
                    "new non-exhaustive protocol variant requires a lockstep transform update"
                ),
            };
            Some(claude::ToolChoice::Tool(crate::protocol::wire!(
                claude::ToolChoiceTool {
                    name,
                    type_: claude::ToolChoiceToolType::Tool,
                    disable_parallel_tool_use,
                    extra: Default::default(),
                }
            )))
        }
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}
