use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

use super::{State, ToolCall, events};

impl State {
    pub(super) fn item_added(
        &mut self,
        event: openai::ResponseOutputItemEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        let item = *event.item;
        match item {
            openai::ResponseItem::Typed(item) => match *item {
                openai::TypedResponseItem::FunctionCall {
                    arguments,
                    call_id,
                    name,
                    id,
                    ..
                } => {
                    let item_id = id.ok_or_else(|| {
                        TransformError::shape("Responses stream", "output item id is missing")
                    })?;
                    self.calls.insert(
                        item_id.clone(),
                        ToolCall {
                            call_id,
                            name,
                            arguments,
                            custom: false,
                        },
                    );
                    self.call_indices.insert(event.output_index, item_id);
                }
                openai::TypedResponseItem::CustomToolCall {
                    call_id,
                    input,
                    name,
                    id,
                    ..
                } => {
                    let item_id = id.ok_or_else(|| {
                        TransformError::shape("Responses stream", "output item id is missing")
                    })?;
                    self.calls.insert(
                        item_id.clone(),
                        ToolCall {
                            call_id,
                            name,
                            arguments: input,
                            custom: true,
                        },
                    );
                    self.call_indices.insert(event.output_index, item_id);
                }
                openai::TypedResponseItem::FileSearchCall { .. }
                | openai::TypedResponseItem::ComputerCall { .. }
                | openai::TypedResponseItem::ComputerCallOutput { .. }
                | openai::TypedResponseItem::WebSearchCall { .. }
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
                | openai::TypedResponseItem::CompactionTrigger { .. }
                | openai::TypedResponseItem::ItemReference { .. } => {}
            },
            openai::ResponseItem::Message(
                openai::ResponseMessageItem::Output(_)
                | openai::ResponseMessageItem::Input(_)
                | openai::ResponseMessageItem::EasyInput(_)
                | openai::ResponseMessageItem::Unknown(_),
            )
            | openai::ResponseItem::Unknown(_) => {}
        }
        Ok(Vec::new())
    }

    pub(super) fn item_done(
        &mut self,
        event: openai::ResponseOutputItemEvent,
    ) -> Result<Vec<Bytes>, TransformError> {
        let item = *event.item;
        let key = item_id(&item).unwrap_or_else(|| format!("index:{}", event.output_index));
        self.emit_item(item, key)
    }

    pub(super) fn emit_item(
        &mut self,
        mut item: openai::ResponseItem,
        key: String,
    ) -> Result<Vec<Bytes>, TransformError> {
        if self.emitted.contains(&key) {
            return Ok(Vec::new());
        }
        if self.text_items.contains(&key) {
            item = match item {
                openai::ResponseItem::Message(
                    openai::ResponseMessageItem::Output(_)
                    | openai::ResponseMessageItem::Input(_)
                    | openai::ResponseMessageItem::EasyInput(_)
                    | openai::ResponseMessageItem::Unknown(_),
                ) => return Ok(Vec::new()),
                openai::ResponseItem::Typed(item) => match *item {
                    openai::TypedResponseItem::Reasoning {
                        id,
                        encrypted_content: Some(encrypted_content),
                        status,
                        ..
                    } => openai::ResponseItem::Typed(Box::new(
                        openai::TypedResponseItem::Reasoning {
                            id,
                            summary: Vec::new(),
                            content: None,
                            encrypted_content: Some(encrypted_content),
                            status,
                            rest: Default::default(),
                        },
                    )),
                    openai::TypedResponseItem::Reasoning { .. } => return Ok(Vec::new()),
                    other @ (openai::TypedResponseItem::FileSearchCall { .. }
                    | openai::TypedResponseItem::ComputerCall { .. }
                    | openai::TypedResponseItem::ComputerCallOutput { .. }
                    | openai::TypedResponseItem::WebSearchCall { .. }
                    | openai::TypedResponseItem::FunctionCall { .. }
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
                    | openai::TypedResponseItem::CustomToolCall { .. }
                    | openai::TypedResponseItem::CustomToolCallOutput { .. }
                    | openai::TypedResponseItem::Program { .. }
                    | openai::TypedResponseItem::ProgramOutput { .. }
                    | openai::TypedResponseItem::MultiAgentCall { .. }
                    | openai::TypedResponseItem::MultiAgentCallOutput { .. }
                    | openai::TypedResponseItem::AgentMessage { .. }
                    | openai::TypedResponseItem::CompactionTrigger { .. }
                    | openai::TypedResponseItem::ItemReference { .. }) => {
                        openai::ResponseItem::Typed(Box::new(other))
                    }
                },
                openai::ResponseItem::Unknown(_) => return Ok(Vec::new()),
            };
        }
        let content = self.content.item(item)?;
        self.emitted.insert(key);
        content
            .map(|content| {
                self.emit(events::chunk(
                    Some(content),
                    None,
                    None,
                    self.response_id.clone(),
                    self.model.clone(),
                ))
            })
            .transpose()
            .map(|value| value.into_iter().collect())
    }
}

pub(super) fn item_id(item: &openai::ResponseItem) -> Option<String> {
    match item {
        openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
            Some(message.id.clone())
        }
        openai::ResponseItem::Typed(item) => match item.as_ref() {
            openai::TypedResponseItem::FunctionCall { id, .. }
            | openai::TypedResponseItem::CustomToolCall { id, .. }
            | openai::TypedResponseItem::ShellCall { id, .. }
            | openai::TypedResponseItem::ApplyPatchCall { id, .. }
            | openai::TypedResponseItem::ShellCallOutput { id, .. }
            | openai::TypedResponseItem::ApplyPatchCallOutput { id, .. }
            | openai::TypedResponseItem::Reasoning { id, .. } => id.clone(),
            openai::TypedResponseItem::LocalShellCall { id, .. }
            | openai::TypedResponseItem::LocalShellCallOutput { id, .. } => Some(id.clone()),
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
            | openai::TypedResponseItem::CompactionTrigger { .. }
            | openai::TypedResponseItem::ItemReference { .. } => None,
        },
        openai::ResponseItem::Message(
            openai::ResponseMessageItem::Input(_)
            | openai::ResponseMessageItem::EasyInput(_)
            | openai::ResponseMessageItem::Unknown(_),
        )
        | openai::ResponseItem::Unknown(_) => None,
    }
}
