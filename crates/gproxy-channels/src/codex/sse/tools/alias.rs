use gproxy_channel_api::ChannelError;
use gproxy_protocol::openai::common::{ResponseApplyPatchCallStatus, ResponseItemLifecycleStatus};
use gproxy_protocol::openai::generate_content::responses::{
    ResponseItem, ShellEnvironment, TypedResponseItem,
};

use super::patch::patch_operation;
use super::shell_action;

pub(super) struct Alias {
    pub(super) kind: AliasKind,
    pub(super) id: Option<String>,
    pub(super) call_id: String,
    pub(super) input: String,
    pub(super) item: Option<ResponseItem>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AliasKind {
    Shell,
    ApplyPatch,
}

pub(super) fn alias(item: &ResponseItem) -> Option<Alias> {
    let ResponseItem::Typed(item) = item else {
        return None;
    };
    match item.as_ref() {
        TypedResponseItem::FunctionCall {
            id,
            call_id,
            name,
            arguments,
            ..
        } if name == "shell_command" => Some(Alias {
            kind: AliasKind::Shell,
            id: id.clone(),
            call_id: call_id.clone(),
            input: arguments.clone(),
            item: None,
        }),
        TypedResponseItem::CustomToolCall {
            id,
            call_id,
            name,
            input,
            ..
        } if name == "apply_patch" => Some(Alias {
            kind: AliasKind::ApplyPatch,
            id: id.clone(),
            call_id: call_id.clone(),
            input: input.clone(),
            item: None,
        }),
        TypedResponseItem::FileSearchCall { .. }
        | TypedResponseItem::ComputerCall { .. }
        | TypedResponseItem::ComputerCallOutput { .. }
        | TypedResponseItem::WebSearchCall { .. }
        | TypedResponseItem::FunctionCall { .. }
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
        | TypedResponseItem::CustomToolCall { .. }
        | TypedResponseItem::CustomToolCallOutput { .. }
        | TypedResponseItem::Program { .. }
        | TypedResponseItem::ProgramOutput { .. }
        | TypedResponseItem::MultiAgentCall { .. }
        | TypedResponseItem::MultiAgentCallOutput { .. }
        | TypedResponseItem::AgentMessage { .. }
        | TypedResponseItem::CompactionTrigger { .. }
        | TypedResponseItem::ItemReference { .. } => None,
    }
}

pub(super) fn canonical_existing(
    item: &ResponseItem,
) -> Result<Option<ResponseItem>, ChannelError> {
    let Some(alias) = alias(item) else {
        return Ok(None);
    };
    canonical(alias.kind, alias.id, &alias.call_id, &alias.input).map(Some)
}

pub(super) fn canonical(
    kind: AliasKind,
    id: Option<String>,
    call_id: &str,
    input: &str,
) -> Result<ResponseItem, ChannelError> {
    let item = match kind {
        AliasKind::Shell => TypedResponseItem::ShellCall {
            action: shell_action(input),
            call_id: call_id.into(),
            id,
            caller: None,
            environment: Some(ShellEnvironment::Local {
                skills: None,
                rest: Default::default(),
            }),
            status: Some(ResponseItemLifecycleStatus::Completed),
            created_by: None,
            rest: Default::default(),
        },
        AliasKind::ApplyPatch => TypedResponseItem::ApplyPatchCall {
            call_id: call_id.into(),
            operation: patch_operation(input)?,
            status: ResponseApplyPatchCallStatus::Completed,
            id,
            caller: None,
            created_by: None,
            rest: Default::default(),
        },
    };
    Ok(ResponseItem::Typed(Box::new(item)))
}
