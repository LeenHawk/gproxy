use gproxy_protocol::{claude, openai};

use crate::TransformError;

use super::ids::preserve_item_id;
use super::{ClaudeCall, execution, hosted};

pub(crate) fn openai_call(
    item: openai::TypedResponseItem,
) -> Result<Option<ClaudeCall>, TransformError> {
    Ok(match item {
        openai::TypedResponseItem::LocalShellCall {
            id,
            action,
            call_id,
            rest,
            ..
        } => Some(execution::local_shell(id, action, call_id, rest)?),
        openai::TypedResponseItem::ShellCall {
            action,
            call_id,
            id,
            environment,
            rest,
            ..
        } => Some(execution::shell(action, call_id, id, environment, rest)?),
        openai::TypedResponseItem::ApplyPatchCall {
            call_id,
            operation,
            id,
            rest,
            ..
        } => Some(execution::apply_patch(call_id, operation, id, rest)),
        openai::TypedResponseItem::ComputerCall {
            id,
            call_id,
            action,
            actions,
            rest,
            ..
        } => Some(execution::computer(id, call_id, action, actions, rest)?),
        openai::TypedResponseItem::WebSearchCall {
            id, action, rest, ..
        } => Some(hosted::web_search(id, action, rest)?),
        openai::TypedResponseItem::CodeInterpreterCall {
            id,
            code,
            container_id,
            rest,
            ..
        } => Some(hosted::code_interpreter(id, code, container_id, rest)),
        openai::TypedResponseItem::ToolSearchCall {
            arguments,
            id,
            call_id,
            execution,
            rest,
            ..
        } => Some(hosted::tool_search(
            arguments, id, call_id, execution, rest,
        )?),
        openai::TypedResponseItem::McpCall {
            id,
            arguments,
            name,
            rest,
            ..
        } => Some(hosted::mcp(id, arguments, name, rest)?),
        openai::TypedResponseItem::Program {
            id,
            call_id,
            code,
            fingerprint,
            rest,
        } => Some(hosted::program(id, call_id, code, fingerprint, rest)),
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
        | openai::TypedResponseItem::CompactionTrigger { .. }
        | openai::TypedResponseItem::ItemReference { .. } => None,
    })
}

pub(crate) fn request_block(mut call: ClaudeCall) -> claude::ContentBlockParam {
    preserve_item_id(&mut call.rest, call.item_id);
    claude::ContentBlockParam::ToolUse(claude::ToolUseBlock {
        id: call.id,
        input: call.input,
        name: call.name,
        type_: claude::ToolUseBlockType::ToolUse,
        cache_control: None,
        caller: None,
        rest: call.rest,
    })
}

pub(crate) fn response_block(mut call: ClaudeCall) -> claude::ResponseContentBlock {
    preserve_item_id(&mut call.rest, call.item_id);
    claude::ResponseContentBlock::ToolUse(claude::ResponseToolUseBlock {
        id: call.id,
        input: call.input,
        name: call.name,
        type_: claude::ToolUseBlockType::ToolUse,
        caller: None,
        rest: call.rest,
    })
}
