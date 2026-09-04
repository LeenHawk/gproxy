use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::super::ContentConverter;

pub(super) fn convert(
    state: &mut ContentConverter,
    item: &openai::TypedResponseItem,
) -> Result<Option<gemini::Content>, TransformError> {
    let (call_id, code) = match item {
        openai::TypedResponseItem::ShellCall {
            action, call_id, ..
        } => (call_id.clone(), action.commands.join("\n")),
        openai::TypedResponseItem::LocalShellCall {
            action,
            call_id,
            id,
            ..
        } => {
            state.native_ids.insert(id.clone(), call_id.clone());
            (call_id.clone(), action.command.join("\n"))
        }
        openai::TypedResponseItem::ApplyPatchCall {
            call_id, operation, ..
        } => (
            call_id.clone(),
            match operation {
                openai::ApplyPatchOperation::CreateFile { diff, .. }
                | openai::ApplyPatchOperation::UpdateFile { diff, .. } => diff.clone(),
                openai::ApplyPatchOperation::DeleteFile { path, .. } => {
                    format!("delete_file {path}")
                }
                #[cfg(not(feature = "exhaustive"))]
                _ => {
                    return Err(crate::TransformError::unsupported(
                        "protocol enum",
                        "unrecognized external variant",
                    ));
                }
            },
        ),
        openai::TypedResponseItem::CodeInterpreterCall { id, code, .. } => (
            id.clone(),
            code.clone().ok_or_else(|| {
                TransformError::shape("Responses code interpreter call", "code is missing")
            })?,
        ),
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
        | openai::TypedResponseItem::LocalShellCallOutput { .. }
        | openai::TypedResponseItem::ShellCallOutput { .. }
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
        | openai::TypedResponseItem::ConfigurationUpdate { .. }
        | openai::TypedResponseItem::CompactionTrigger { .. }
        | openai::TypedResponseItem::ItemReference { .. } => return Ok(None),
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    };
    Ok(Some(super::model_content(vec![crate::wire!(
        gemini::Part {
            data: Some(gemini::PartData::ExecutableCode {
                executable_code: gemini::ExecutableCode {
                    id: Some(call_id),
                    language: gemini::ExecutableCodeLanguage::Known(
                        gemini::ExecutableCodeLanguageKnown::Python,
                    ),
                    code,
                    rest: Default::default(),
                },
                rest: Default::default(),
            }),
            rest: Default::default(),
            ..Default::default()
        }
    )])))
}
