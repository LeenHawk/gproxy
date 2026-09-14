//! Chat chunks currently declare only function-call deltas. Reject an active
//! custom tool before dispatch; completed custom calls remain valid history.
use crate::{transform::TransformError, wire::openai::chat as c};

pub(crate) fn check(input: &c::GenerateContentRequestBody) -> Result<(), TransformError> {
    let names: Vec<_> = input
        .tools
        .iter()
        .flatten()
        .filter_map(|tool| match tool {
            c::ChatTool::Custom(tool) => Some(tool.custom.name.as_str()),
            _ => None,
        })
        .collect();
    let may_call_custom = match input.tool_choice.as_ref() {
        Some(c::ToolChoice::Mode(c::ToolChoiceMode::None) | c::ToolChoice::Function(_)) => false,
        Some(c::ToolChoice::Custom(_)) => true,
        Some(c::ToolChoice::Allowed(choice)) => choice
            .allowed_tools
            .tools
            .iter()
            .any(|tool| tool.get("type").and_then(serde_json::Value::as_str) == Some("custom")),
        _ => !names.is_empty(),
    };
    if may_call_custom {
        return Err(TransformError::unsupported(
            "stream.custom_tools",
            "Chat chunk schema has no custom-tool input delta; use a buffered Chat result or native Responses streaming",
        ));
    }
    Ok(())
}
