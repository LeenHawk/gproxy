use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::super::ContentConverter;

pub(super) fn convert(
    state: &mut ContentConverter,
    item: &openai::TypedResponseItem,
) -> Result<Option<gemini::Content>, TransformError> {
    let (call_id, outcome, text) = match item {
        openai::TypedResponseItem::ShellCallOutput {
            call_id, output, ..
        } => {
            if output.is_empty() {
                return Err(TransformError::shape(
                    "Responses shellCallOutput",
                    "output is empty",
                ));
            }
            let failed = shell_failed(output);
            let text = output
                .iter()
                .flat_map(|part| [&part.stdout, &part.stderr])
                .filter(|value| !value.is_empty())
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            (
                call_id.clone(),
                outcome(failed),
                (!text.is_empty()).then_some(text),
            )
        }
        openai::TypedResponseItem::LocalShellCallOutput {
            id, output, status, ..
        } => {
            let failed = lifecycle_failed(status.as_ref(), "Responses localShellCallOutput")?;
            (
                state
                    .native_ids
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| id.clone()),
                outcome(failed),
                (!output.is_empty()).then_some(output.clone()),
            )
        }
        openai::TypedResponseItem::ApplyPatchCallOutput {
            call_id,
            status,
            output,
            ..
        } => {
            let failed = match status {
                openai::ResponseApplyPatchCallOutputStatus::Completed => false,
                openai::ResponseApplyPatchCallOutputStatus::Failed => true,
                openai::ResponseApplyPatchCallOutputStatus::Unknown(value) => {
                    return Err(TransformError::unsupported(
                        "Responses applyPatchCallOutput status",
                        value,
                    ));
                }
            };
            (call_id.clone(), outcome(failed), output.clone())
        }
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
        | openai::TypedResponseItem::ShellCall { .. }
        | openai::TypedResponseItem::ApplyPatchCall { .. }
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
        | openai::TypedResponseItem::ItemReference { .. } => return Ok(None),
    };
    Ok(Some(super::user_content(vec![gemini::Part {
        data: Some(gemini::PartData::CodeExecutionResult {
            code_execution_result: gemini::CodeExecutionResult {
                id: Some(call_id),
                outcome,
                output: text,
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        rest: Default::default(),
        ..Default::default()
    }])))
}

fn shell_failed(output: &[openai::ShellCallOutputContent]) -> bool {
    let mut failed = false;
    for part in output {
        match &part.outcome {
            openai::ShellCallOutcome::Exit { exit_code: 0, .. } => {}
            openai::ShellCallOutcome::Exit { .. } | openai::ShellCallOutcome::Timeout { .. } => {
                failed = true
            }
        }
    }
    failed
}

fn lifecycle_failed(
    status: Option<&openai::ResponseItemLifecycleStatus>,
    wire: &'static str,
) -> Result<bool, TransformError> {
    match status {
        Some(openai::ResponseItemLifecycleStatus::Completed) => Ok(false),
        Some(openai::ResponseItemLifecycleStatus::Incomplete) => Ok(true),
        Some(openai::ResponseItemLifecycleStatus::InProgress) => {
            Err(TransformError::shape(wire, "status is in_progress"))
        }
        Some(openai::ResponseItemLifecycleStatus::Unknown(value)) => {
            Err(TransformError::unsupported(wire, value))
        }
        None => Err(TransformError::shape(wire, "status is missing")),
    }
}

fn outcome(failed: bool) -> gemini::CodeExecutionOutcome {
    gemini::CodeExecutionOutcome::Known(if failed {
        gemini::CodeExecutionOutcomeKnown::OutcomeFailed
    } else {
        gemini::CodeExecutionOutcomeKnown::OutcomeOk
    })
}
