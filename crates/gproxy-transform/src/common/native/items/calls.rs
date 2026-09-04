use gproxy_protocol::{claude, openai};

use crate::TransformError;

use super::{ClaudeCall, execution, hosted};

pub(crate) fn openai_call(
    item: openai::TypedResponseItem,
) -> Result<Option<ClaudeCall>, TransformError> {
    Ok(match item {
        openai::TypedResponseItem::LocalShellCall {
            id,
            action,
            call_id,
            ..
        } => Some(execution::local_shell(id, action, call_id)?),
        openai::TypedResponseItem::ShellCall {
            action,
            call_id,
            id,
            environment,
            ..
        } => Some(execution::shell(action, call_id, id, environment)?),
        openai::TypedResponseItem::ApplyPatchCall {
            call_id,
            operation,
            id,
            ..
        } => Some(execution::apply_patch(call_id, operation, id)?),
        openai::TypedResponseItem::ComputerCall {
            id,
            call_id,
            action,
            actions,
            ..
        } => Some(execution::computer(id, call_id, action, actions)?),
        openai::TypedResponseItem::WebSearchCall { id, action, .. } => {
            Some(hosted::web_search(id, action)?)
        }
        openai::TypedResponseItem::CodeInterpreterCall {
            id,
            code,
            container_id,
            ..
        } => Some(hosted::code_interpreter(id, code, container_id)),
        openai::TypedResponseItem::ToolSearchCall {
            arguments,
            id,
            call_id,
            execution,
            ..
        } => Some(hosted::tool_search(arguments, id, call_id, execution)?),
        openai::TypedResponseItem::McpCall {
            id,
            arguments,
            name,
            ..
        } => Some(hosted::mcp(id, arguments, name)?),
        openai::TypedResponseItem::Program {
            id,
            call_id,
            code,
            fingerprint,
            ..
        } => Some(hosted::program(id, call_id, code, fingerprint)),
        openai::TypedResponseItem::FileSearchCall { .. }
        | openai::TypedResponseItem::ComputerCallOutput { .. }
        | openai::TypedResponseItem::FunctionCall { .. }
        | openai::TypedResponseItem::FunctionCallOutput { .. }
        | openai::TypedResponseItem::ToolSearchOutput { .. }
        | openai::TypedResponseItem::AdditionalTools { .. }
        | openai::TypedResponseItem::Reasoning { .. }
        | openai::TypedResponseItem::Compaction { .. }
        | openai::TypedResponseItem::ImageGenerationCall { .. }
        | openai::TypedResponseItem::LocalShellCallOutput { .. }
        | openai::TypedResponseItem::ShellCallOutput { .. }
        | openai::TypedResponseItem::ApplyPatchCallOutput { .. }
        | openai::TypedResponseItem::McpListTools { .. }
        | openai::TypedResponseItem::McpApprovalRequest { .. }
        | openai::TypedResponseItem::McpApprovalResponse { .. }
        | openai::TypedResponseItem::CustomToolCall { .. }
        | openai::TypedResponseItem::CustomToolCallOutput { .. }
        | openai::TypedResponseItem::ProgramOutput { .. }
        | openai::TypedResponseItem::MultiAgentCall { .. }
        | openai::TypedResponseItem::MultiAgentCallOutput { .. }
        | openai::TypedResponseItem::AgentMessage { .. }
        | openai::TypedResponseItem::ConfigurationUpdate { .. }
        | openai::TypedResponseItem::CompactionTrigger { .. }
        | openai::TypedResponseItem::ItemReference { .. } => None,
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    })
}

pub(crate) fn request_block(call: ClaudeCall) -> claude::ContentBlockParam {
    claude::ContentBlockParam::ToolUse(crate::wire!(claude::ToolUseBlock {
        id: call.id,
        input: call.input,
        name: call.name,
        type_: claude::ToolUseBlockType::ToolUse,
        cache_control: None,
        caller: None,
        rest: Default::default(),
    }))
}

pub(crate) fn response_block(call: ClaudeCall) -> claude::ResponseContentBlock {
    claude::ResponseContentBlock::ToolUse(crate::wire!(claude::ResponseToolUseBlock {
        id: call.id,
        input: call.input,
        name: call.name,
        type_: claude::ToolUseBlockType::ToolUse,
        caller: None,
        rest: Default::default(),
    }))
}
