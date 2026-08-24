use gproxy_protocol::openai::common::ResponseItemLifecycleStatus;
use gproxy_protocol::openai::generate_content::responses::{
    ResponseItem, ResponseMessageItem, TypedResponseItem,
};

use super::ItemState;

impl ItemState {
    pub(in crate::codex::sse::lifecycle) fn complete_item(&mut self) {
        let Some(item) = self.item.as_mut() else {
            return;
        };
        match item {
            ResponseItem::Message(ResponseMessageItem::Output(message)) => {
                message.status = ResponseItemLifecycleStatus::Completed
            }
            ResponseItem::Typed(item) => match item.as_mut() {
                TypedResponseItem::FunctionCall { status, .. }
                | TypedResponseItem::Reasoning { status, .. } => {
                    *status = Some(ResponseItemLifecycleStatus::Completed)
                }
                TypedResponseItem::FileSearchCall { .. }
                | TypedResponseItem::ComputerCall { .. }
                | TypedResponseItem::ComputerCallOutput { .. }
                | TypedResponseItem::WebSearchCall { .. }
                | TypedResponseItem::FunctionCallOutput { .. }
                | TypedResponseItem::ToolSearchCall { .. }
                | TypedResponseItem::ToolSearchOutput { .. }
                | TypedResponseItem::AdditionalTools { .. }
                | TypedResponseItem::Compaction { .. }
                | TypedResponseItem::ImageGenerationCall { .. }
                | TypedResponseItem::CodeInterpreterCall { .. }
                | TypedResponseItem::LocalShellCall { .. }
                | TypedResponseItem::LocalShellCallOutput { .. }
                | TypedResponseItem::ShellCall { .. }
                | TypedResponseItem::ShellCallOutput { .. }
                | TypedResponseItem::ApplyPatchCall { .. }
                | TypedResponseItem::ApplyPatchCallOutput { .. }
                | TypedResponseItem::McpListTools { .. }
                | TypedResponseItem::McpApprovalRequest { .. }
                | TypedResponseItem::McpApprovalResponse { .. }
                | TypedResponseItem::McpCall { .. }
                | TypedResponseItem::CustomToolCall { .. }
                | TypedResponseItem::CustomToolCallOutput { .. }
                | TypedResponseItem::Program { .. }
                | TypedResponseItem::ProgramOutput { .. }
                | TypedResponseItem::MultiAgentCall { .. }
                | TypedResponseItem::MultiAgentCallOutput { .. }
                | TypedResponseItem::AgentMessage { .. }
                | TypedResponseItem::CompactionTrigger { .. }
                | TypedResponseItem::ItemReference { .. } => {}
            },
            ResponseItem::Message(
                ResponseMessageItem::Input(_)
                | ResponseMessageItem::EasyInput(_)
                | ResponseMessageItem::Unknown(_),
            )
            | ResponseItem::Unknown(_) => {}
        }
    }
}
