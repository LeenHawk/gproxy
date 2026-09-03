use gproxy_protocol::openai;

pub(crate) fn item_id(item: &openai::TypedResponseItem) -> Option<String> {
    match item {
        openai::TypedResponseItem::LocalShellCall { id, .. } => Some(id.clone()),
        openai::TypedResponseItem::ShellCall { id, .. }
        | openai::TypedResponseItem::ApplyPatchCall { id, .. } => id.clone(),
        openai::TypedResponseItem::ComputerCall { id, .. }
        | openai::TypedResponseItem::WebSearchCall { id, .. }
        | openai::TypedResponseItem::CodeInterpreterCall { id, .. }
        | openai::TypedResponseItem::McpCall { id, .. }
        | openai::TypedResponseItem::Program { id, .. } => Some(id.clone()),
        openai::TypedResponseItem::ToolSearchCall { id, .. } => id.clone(),
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
    }
}
