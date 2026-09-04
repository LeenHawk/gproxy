use gproxy_protocol::{claude, openai};

pub(crate) struct ClaudeResult {
    call_id: String,
    content: Option<claude::ToolResultContent>,
    is_error: Option<bool>,
}

pub(crate) fn openai_result(item: openai::TypedResponseItem) -> Option<ClaudeResult> {
    match item {
        openai::TypedResponseItem::ShellCallOutput {
            call_id, output, ..
        } => {
            let failed = output.iter().any(|part| match &part.outcome {
                openai::ShellCallOutcome::Exit { exit_code, .. } => *exit_code != 0,
                openai::ShellCallOutcome::Timeout { .. } => true,
                #[cfg(not(feature = "exhaustive"))]
                _ => true,
            });
            let text = output
                .into_iter()
                .flat_map(|part| [part.stdout, part.stderr])
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            Some(ClaudeResult {
                call_id,
                content: (!text.is_empty()).then_some(claude::ToolResultContent::Text(text)),
                is_error: Some(failed),
            })
        }
        openai::TypedResponseItem::ApplyPatchCallOutput {
            call_id,
            status,
            id: _,
            output,
            ..
        } => Some(ClaudeResult {
            call_id,
            content: output.map(claude::ToolResultContent::Text),
            is_error: Some(matches!(
                status,
                openai::ResponseApplyPatchCallOutputStatus::Failed
            )),
        }),
        openai::TypedResponseItem::ProgramOutput {
            id: _,
            call_id,
            result,
            status,
            ..
        } => Some(ClaudeResult {
            call_id,
            content: Some(claude::ToolResultContent::Text(result)),
            is_error: Some(!matches!(
                status,
                openai::ResponseItemLifecycleStatus::Completed
            )),
        }),
        openai::TypedResponseItem::FileSearchCall { .. }
        | openai::TypedResponseItem::ComputerCall { .. }
        | openai::TypedResponseItem::ComputerCallOutput { .. }
        | openai::TypedResponseItem::WebSearchCall { .. }
        | openai::TypedResponseItem::FunctionCall { .. }
        | openai::TypedResponseItem::FunctionCallOutput { .. }
        | openai::TypedResponseItem::ToolSearchCall { .. }
        | openai::TypedResponseItem::ToolSearchOutput { .. }
        | openai::TypedResponseItem::AdditionalTools { .. }
        | openai::TypedResponseItem::Reasoning { .. }
        | openai::TypedResponseItem::Compaction { .. }
        | openai::TypedResponseItem::ImageGenerationCall { .. }
        | openai::TypedResponseItem::CodeInterpreterCall { .. }
        | openai::TypedResponseItem::LocalShellCall { .. }
        | openai::TypedResponseItem::LocalShellCallOutput { .. }
        | openai::TypedResponseItem::ShellCall { .. }
        | openai::TypedResponseItem::ApplyPatchCall { .. }
        | openai::TypedResponseItem::McpListTools { .. }
        | openai::TypedResponseItem::McpApprovalRequest { .. }
        | openai::TypedResponseItem::McpApprovalResponse { .. }
        | openai::TypedResponseItem::McpCall { .. }
        | openai::TypedResponseItem::CustomToolCall { .. }
        | openai::TypedResponseItem::CustomToolCallOutput { .. }
        | openai::TypedResponseItem::Program { .. }
        | openai::TypedResponseItem::MultiAgentCall { .. }
        | openai::TypedResponseItem::MultiAgentCallOutput { .. }
        | openai::TypedResponseItem::AgentMessage { .. }
        | openai::TypedResponseItem::ConfigurationUpdate { .. }
        | openai::TypedResponseItem::CompactionTrigger { .. }
        | openai::TypedResponseItem::ItemReference { .. } => None,
        #[cfg(not(feature = "exhaustive"))]
        _ => None,
    }
}

pub(crate) fn result_block(result: ClaudeResult) -> claude::ContentBlockParam {
    claude::ContentBlockParam::ToolResult(crate::wire!(claude::ToolResultBlock {
        tool_use_id: result.call_id,
        type_: claude::ToolResultBlockType::ToolResult,
        cache_control: None,
        content: result.content,
        is_error: result.is_error,
        rest: Default::default(),
    }))
}
