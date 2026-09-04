use gproxy_protocol::openai;

pub(super) fn item_id(item: &openai::ResponseItem) -> Option<String> {
    match item {
        openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
            Some(message.id.clone())
        }
        openai::ResponseItem::Typed(item) => match item.as_ref() {
            openai::TypedResponseItem::FunctionCall { id, call_id, .. }
            | openai::TypedResponseItem::CustomToolCall { id, call_id, .. } => {
                let _ = call_id;
                id.clone()
            }
            openai::TypedResponseItem::Reasoning { id, .. } => id.clone(),
            openai::TypedResponseItem::FileSearchCall { .. }
            | openai::TypedResponseItem::ComputerCall { .. }
            | openai::TypedResponseItem::ComputerCallOutput { .. }
            | openai::TypedResponseItem::WebSearchCall { .. }
            | openai::TypedResponseItem::FunctionCallOutput { .. }
            | openai::TypedResponseItem::ToolSearchCall { .. }
            | openai::TypedResponseItem::ToolSearchOutput { .. }
            | openai::TypedResponseItem::AdditionalTools { .. }
            | openai::TypedResponseItem::Compaction { .. }
            | openai::TypedResponseItem::ImageGenerationCall { .. }
            | openai::TypedResponseItem::CodeInterpreterCall { .. }
            | openai::TypedResponseItem::LocalShellCall { .. }
            | openai::TypedResponseItem::LocalShellCallOutput { .. }
            | openai::TypedResponseItem::ShellCall { .. }
            | openai::TypedResponseItem::ShellCallOutput { .. }
            | openai::TypedResponseItem::ApplyPatchCall { .. }
            | openai::TypedResponseItem::ApplyPatchCallOutput { .. }
            | openai::TypedResponseItem::McpListTools { .. }
            | openai::TypedResponseItem::McpApprovalRequest { .. }
            | openai::TypedResponseItem::McpApprovalResponse { .. }
            | openai::TypedResponseItem::McpCall { .. }
            | openai::TypedResponseItem::CustomToolCallOutput { .. }
            | openai::TypedResponseItem::Program { .. }
            | openai::TypedResponseItem::ProgramOutput { .. }
            | openai::TypedResponseItem::MultiAgentCall { .. }
            | openai::TypedResponseItem::MultiAgentCallOutput { .. }
            | openai::TypedResponseItem::AgentMessage { .. }
            | openai::TypedResponseItem::ConfigurationUpdate { .. }
            | openai::TypedResponseItem::CompactionTrigger { .. }
            | openai::TypedResponseItem::ItemReference { .. } => None,
            #[cfg(not(feature = "exhaustive"))]
            _ => None,
        },
        openai::ResponseItem::Message(
            openai::ResponseMessageItem::Input(_)
            | openai::ResponseMessageItem::EasyInput(_)
            | openai::ResponseMessageItem::Unknown(_),
        )
        | openai::ResponseItem::Unknown(_) => None,
        #[cfg(not(feature = "exhaustive"))]
        _ => None,
    }
}
