use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::super::ContentConverter;
use super::correlated;
use crate::generate_content::gemini_generate_content_to_openai_responses::ids;

impl ContentConverter {
    pub(in crate::generate_content) fn executable_code(
        &mut self,
        code: gemini::ExecutableCode,
    ) -> Result<openai::ResponseItem, TransformError> {
        match code.language {
            gemini::ExecutableCodeLanguage::Known(gemini::ExecutableCodeLanguageKnown::Python) => {}
            gemini::ExecutableCodeLanguage::Known(
                gemini::ExecutableCodeLanguageKnown::LanguageUnspecified,
            ) => {
                return Err(TransformError::shape(
                    "Gemini executableCode",
                    "language is unspecified",
                ));
            }
            gemini::ExecutableCodeLanguage::Unknown(value) => {
                return Err(TransformError::unsupported(
                    "Gemini executableCode language",
                    value,
                ));
            }
            _ => {
                return Err(TransformError::unsupported(
                    "Gemini executableCode language",
                    "future language",
                ));
            }
        }
        let call_id = self.allocate_call(code.id);
        self.code_calls.push_back(call_id.clone());
        Ok(openai::ResponseItem::Typed(Box::new(
            openai::TypedResponseItem::ShellCall {
                action: openai::ShellAction {
                    commands: vec![code.code],
                    max_output_length: None,
                    timeout_ms: None,
                    rest: Default::default(),
                },
                call_id: call_id.clone(),
                id: Some(ids::item_id("sh", &call_id)),
                caller: None,
                environment: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                created_by: None,
                rest: Default::default(),
            },
        )))
    }

    pub(in crate::generate_content) fn code_result(
        &mut self,
        result: gemini::CodeExecutionResult,
    ) -> Result<openai::ResponseItem, TransformError> {
        let call_id = correlated(result.id, Some(&mut self.code_calls)).ok_or_else(|| {
            TransformError::shape(
                "Gemini codeExecutionResult",
                "id missing and no matching executableCode was seen",
            )
        })?;
        let (outcome, failed) = code_outcome(&result.outcome)?;
        let text = result.output.ok_or_else(|| {
            TransformError::shape("Gemini codeExecutionResult", "output is missing")
        })?;
        Ok(openai::ResponseItem::Typed(Box::new(
            openai::TypedResponseItem::ShellCallOutput {
                call_id,
                output: vec![openai::ShellCallOutputContent {
                    outcome,
                    stderr: if failed { text.clone() } else { String::new() },
                    stdout: if failed { String::new() } else { text },
                    created_by: None,
                    rest: Default::default(),
                }],
                id: None,
                caller: None,
                max_output_length: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                created_by: None,
                rest: Default::default(),
            },
        )))
    }
}

fn code_outcome(
    value: &gemini::CodeExecutionOutcome,
) -> Result<(openai::ShellCallOutcome, bool), TransformError> {
    Ok(match value {
        gemini::CodeExecutionOutcome::Known(gemini::CodeExecutionOutcomeKnown::OutcomeOk) => (
            openai::ShellCallOutcome::Exit {
                exit_code: 0,
                rest: Default::default(),
            },
            false,
        ),
        gemini::CodeExecutionOutcome::Known(gemini::CodeExecutionOutcomeKnown::OutcomeFailed) => (
            openai::ShellCallOutcome::Exit {
                exit_code: 1,
                rest: Default::default(),
            },
            true,
        ),
        gemini::CodeExecutionOutcome::Known(
            gemini::CodeExecutionOutcomeKnown::OutcomeDeadlineExceeded,
        ) => (
            openai::ShellCallOutcome::Timeout {
                rest: Default::default(),
            },
            true,
        ),
        gemini::CodeExecutionOutcome::Known(
            gemini::CodeExecutionOutcomeKnown::OutcomeUnspecified,
        ) => {
            return Err(TransformError::shape(
                "Gemini codeExecutionResult",
                "outcome is unspecified",
            ));
        }
        gemini::CodeExecutionOutcome::Unknown(value) => {
            return Err(TransformError::unsupported(
                "Gemini code execution outcome",
                value,
            ));
        }
        _ => {
            return Err(TransformError::unsupported(
                "Gemini code execution outcome",
                "future outcome",
            ));
        }
    })
}
