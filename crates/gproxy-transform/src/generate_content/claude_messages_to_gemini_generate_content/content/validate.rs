use gproxy_protocol::claude;

use crate::TransformError;

pub(super) fn text(block: &claude::TextBlock) -> Result<(), TransformError> {
    reject(
        block.cache_control.is_some() || block.citations.is_some() || !block.rest.is_empty(),
        "Claude text block",
    )
}

pub(super) fn image(block: &claude::ImageBlock) -> Result<(), TransformError> {
    reject(
        block.cache_control.is_some() || !block.rest.is_empty(),
        "Claude image block",
    )
}

pub(super) fn thinking(block: &claude::ThinkingBlock) -> Result<(), TransformError> {
    reject(!block.rest.is_empty(), "Claude thinking block")
}

pub(super) fn document(block: &claude::DocumentBlock) -> Result<(), TransformError> {
    reject(
        block.cache_control.is_some()
            || block.citations.is_some()
            || block.context.is_some()
            || block.title.is_some()
            || !block.rest.is_empty(),
        "Claude document block",
    )
}

pub(super) fn tool_use(block: &claude::ToolUseBlock) -> Result<(), TransformError> {
    reject(
        block.cache_control.is_some() || !block.rest.is_empty(),
        "Claude tool-use block",
    )
}

pub(super) fn tool_result(block: &claude::ToolResultBlock) -> Result<(), TransformError> {
    reject(
        block.cache_control.is_some() || !block.rest.is_empty(),
        "Claude tool-result block",
    )
}

pub(super) fn server_tool(block: &claude::ServerToolUseBlock) -> Result<(), TransformError> {
    reject(
        block.cache_control.is_some() || block.caller.is_some() || !block.rest.is_empty(),
        "Claude server-tool block",
    )
}

pub(super) fn bash_result(
    block: &claude::BashCodeExecutionToolResultBlock,
) -> Result<(), TransformError> {
    reject(
        block.cache_control.is_some() || !block.rest.is_empty(),
        "Claude bash-result block",
    )
}

fn reject(condition: bool, wire: &'static str) -> Result<(), TransformError> {
    if condition {
        return Err(TransformError::unsupported(
            wire,
            "fields without a Gemini counterpart",
        ));
    }
    Ok(())
}
