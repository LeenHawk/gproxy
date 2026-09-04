use gproxy_protocol::openai::generate_content::responses::{ResponseItem, TypedResponseItem};

use super::{InputKind, ItemState};

impl ItemState {
    pub(in crate::codex::sse::lifecycle) fn note_typed_item(&mut self) {
        let Some(ResponseItem::Typed(item)) = self.item.as_ref() else {
            return;
        };
        match item.as_ref() {
            TypedResponseItem::FunctionCall {
                arguments,
                call_id,
                name,
                ..
            } => {
                self.input_kind = Some(InputKind::Function);
                arguments.clone_into(&mut self.input);
                self.call_id = Some(call_id.clone());
                self.name = Some(name.clone());
            }
            TypedResponseItem::CustomToolCall {
                input,
                call_id,
                name,
                ..
            } => {
                self.input_kind = Some(InputKind::Custom);
                input.clone_into(&mut self.input);
                self.call_id = Some(call_id.clone());
                self.name = Some(name.clone());
            }
            TypedResponseItem::FileSearchCall { .. }
            | TypedResponseItem::ComputerCall { .. }
            | TypedResponseItem::ComputerCallOutput { .. }
            | TypedResponseItem::WebSearchCall { .. }
            | TypedResponseItem::FunctionCallOutput { .. }
            | TypedResponseItem::ToolSearchCall { .. }
            | TypedResponseItem::ToolSearchOutput { .. }
            | TypedResponseItem::AdditionalTools { .. }
            | TypedResponseItem::Reasoning { .. }
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
            | TypedResponseItem::CustomToolCallOutput { .. }
            | TypedResponseItem::Program { .. }
            | TypedResponseItem::ProgramOutput { .. }
            | TypedResponseItem::MultiAgentCall { .. }
            | TypedResponseItem::MultiAgentCallOutput { .. }
            | TypedResponseItem::AgentMessage { .. }
            | TypedResponseItem::ConfigurationUpdate { .. }
            | TypedResponseItem::CompactionTrigger { .. }
            | TypedResponseItem::ItemReference { .. } => {}
        }
    }

    pub(in crate::codex::sse::lifecycle) fn apply_input(&mut self) {
        let Some(ResponseItem::Typed(item)) = self.item.as_mut() else {
            return;
        };
        match item.as_mut() {
            TypedResponseItem::FunctionCall { arguments, .. } => self.input.clone_into(arguments),
            TypedResponseItem::CustomToolCall { input, .. } => self.input.clone_into(input),
            TypedResponseItem::FileSearchCall { .. }
            | TypedResponseItem::ComputerCall { .. }
            | TypedResponseItem::ComputerCallOutput { .. }
            | TypedResponseItem::WebSearchCall { .. }
            | TypedResponseItem::FunctionCallOutput { .. }
            | TypedResponseItem::ToolSearchCall { .. }
            | TypedResponseItem::ToolSearchOutput { .. }
            | TypedResponseItem::AdditionalTools { .. }
            | TypedResponseItem::Reasoning { .. }
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
            | TypedResponseItem::CustomToolCallOutput { .. }
            | TypedResponseItem::Program { .. }
            | TypedResponseItem::ProgramOutput { .. }
            | TypedResponseItem::MultiAgentCall { .. }
            | TypedResponseItem::MultiAgentCallOutput { .. }
            | TypedResponseItem::AgentMessage { .. }
            | TypedResponseItem::ConfigurationUpdate { .. }
            | TypedResponseItem::CompactionTrigger { .. }
            | TypedResponseItem::ItemReference { .. } => {}
        }
    }
}
