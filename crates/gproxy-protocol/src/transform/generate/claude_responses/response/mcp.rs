use super::content::id;
use crate::{
    Dialect,
    transform::{
        Report, TransformError,
        identity::{IdentityFlow, IdentityRole, TargetIdPolicy},
    },
    wire::{claude::generate_content as c, openai::responses as r},
};

pub(super) fn call(
    source: c::ResponseMcpToolUseBlock,
    index: u64,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<r::McpCall, TransformError> {
    if source.name.is_empty() || source.server_name.is_empty() {
        return Err(TransformError::invalid_result(
            "mcp_tool_use",
            "missing name or server",
        ));
    }
    // MCP exposes a single call identity. Unlike FunctionCall it has no
    // separate call_id/item-id pair to conflate.
    let id = id(
        flow,
        policy,
        Dialect::Claude,
        IdentityRole::ToolCall,
        IdentityRole::ToolCall,
        Some(source.id),
        index,
    )?;
    Ok(r::McpCall {
        type_: r::McpCallType::McpCall,
        id,
        arguments: serde_json::to_string(&source.input)?,
        name: source.name,
        server_label: source.server_name,
        approval_request_id: None,
        error: None,
        output: None,
        status: None,
        rest: Default::default(),
    })
}
pub(super) fn result(
    source: c::ResponseMcpToolResultBlock,
    call: &mut r::McpCall,
    report: &mut Report,
) -> Result<(), TransformError> {
    if call.output.is_some() || call.error.is_some() {
        return Err(TransformError::invalid_result(
            "mcp_tool_result",
            "duplicate result for call",
        ));
    }
    let text = match source.content {
        c::ResponseMcpResultContent::Text(text) => text,
        c::ResponseMcpResultContent::Blocks(blocks) => {
            let mut result = String::new();
            for block in blocks {
                if block.citations.is_some() {
                    report.omitted(
                        "mcp_tool_result.citations",
                        "MCP text output has no citations field",
                    );
                }
                result.push_str(&block.text);
            }
            result
        }
    };
    if source.is_error {
        call.error = Some(Some(text));
        call.status = Some(r::McpCallStatus::Failed);
    } else {
        call.output = Some(Some(text));
        call.status = Some(r::McpCallStatus::Completed);
    }
    Ok(())
}
pub(super) fn to_claude(
    source: r::McpCall,
    index: u64,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    report: &mut Report,
) -> Result<Vec<c::ResponseContentBlock>, TransformError> {
    match source.status {
        Some(
            r::McpCallStatus::Calling | r::McpCallStatus::InProgress | r::McpCallStatus::Incomplete,
        ) => {
            return Err(TransformError::invalid_result(
                "mcp_call.status",
                "MCP execution is not terminal",
            ));
        }
        Some(r::McpCallStatus::Completed | r::McpCallStatus::Failed) | None => {}
    }
    if source.name.is_empty() || source.server_label.is_empty() {
        return Err(TransformError::invalid_result(
            "mcp_call",
            "missing name or server",
        ));
    }
    let args: serde_json::Value = serde_json::from_str(&source.arguments)
        .map_err(|e| TransformError::invalid_result("mcp_call.arguments", e.to_string()))?;
    let serde_json::Value::Object(input) = args else {
        return Err(TransformError::unsupported(
            "mcp_call.arguments",
            "Claude requires an object",
        ));
    };
    let error = source.error.flatten();
    let output = source.output.flatten();
    if error.is_some() && output.is_some() {
        return Err(TransformError::unsupported(
            "mcp_call",
            "simultaneous output and error need separate invocation result mapping",
        ));
    }
    if matches!(source.status, Some(r::McpCallStatus::Failed)) && error.is_none() {
        return Err(TransformError::missing_metadata("mcp_call.error"));
    }
    if matches!(source.status, Some(r::McpCallStatus::Completed)) && error.is_some() {
        return Err(TransformError::invalid_result(
            "mcp_call.status",
            "completed call contains error",
        ));
    }
    let id = id(
        flow,
        policy,
        Dialect::OpenAi,
        IdentityRole::ToolCall,
        IdentityRole::ToolCall,
        Some(source.id),
        index,
    )?;
    let mut blocks = vec![c::ResponseContentBlock::McpToolUse(
        c::ResponseMcpToolUseBlock {
            type_: c::ResponseMcpToolUseBlockType::Tag,
            id: id.clone(),
            input,
            name: source.name,
            server_name: source.server_label,
            rest: Default::default(),
        },
    )];
    let is_error = error.is_some();
    if let Some(text) = error.or(output) {
        blocks.push(c::ResponseContentBlock::McpToolResult(
            c::ResponseMcpToolResultBlock {
                type_: c::ResponseMcpToolResultBlockType::Tag,
                content: c::ResponseMcpResultContent::Text(text),
                is_error,
                tool_use_id: id,
                rest: Default::default(),
            },
        ));
    }
    if source.approval_request_id.is_some() {
        report.omitted(
            "mcp_call.approval_request_id",
            "Claude MCP result has no approval-request field",
        );
    }
    Ok(blocks)
}
