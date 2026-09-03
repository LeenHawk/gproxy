use gproxy_protocol::{claude, gemini};

use crate::TransformError;

pub(crate) fn request_bash_result(
    block: claude::BashCodeExecutionToolResultBlock,
) -> Result<gemini::Part, TransformError> {
    let (outcome, output) = request_content(block.content)?;
    Ok(code_result(block.tool_use_id, outcome, output))
}

pub(crate) fn response_bash_result(
    block: claude::ResponseBashCodeExecutionToolResultBlock,
) -> Result<gemini::Part, TransformError> {
    let (outcome, output) = match block.content {
        claude::ResponseBashCodeExecutionToolResultContent::Result(result) => (
            result_outcome(result.return_code),
            command_output(result.stdout, result.stderr),
        ),
        claude::ResponseBashCodeExecutionToolResultContent::Error(error) => (
            gemini::CodeExecutionOutcomeKnown::OutcomeFailed,
            error.error_message,
        ),
        claude::ResponseBashCodeExecutionToolResultContent::Raw(raw) => {
            return Err(TransformError::unsupported(
                "Claude bash result",
                raw.to_string(),
            ));
        }
        _ => {
            return Err(TransformError::unsupported(
                "Claude bash result",
                "future content",
            ));
        }
    };
    Ok(code_result(block.tool_use_id, outcome, output))
}

fn request_content(
    content: claude::BashCodeExecutionToolResultContent,
) -> Result<(gemini::CodeExecutionOutcomeKnown, Option<String>), TransformError> {
    match content {
        claude::BashCodeExecutionToolResultContent::Result(result) => Ok((
            result_outcome(result.return_code),
            command_output(result.stdout, result.stderr),
        )),
        claude::BashCodeExecutionToolResultContent::Error(error) => Ok((
            gemini::CodeExecutionOutcomeKnown::OutcomeFailed,
            error.error_message,
        )),
        claude::BashCodeExecutionToolResultContent::Raw(raw) => Err(TransformError::unsupported(
            "Claude bash result",
            raw.to_string(),
        )),
        _ => Err(TransformError::unsupported(
            "Claude bash result",
            "future content",
        )),
    }
}

fn code_result(
    id: String,
    outcome: gemini::CodeExecutionOutcomeKnown,
    output: Option<String>,
) -> gemini::Part {
    gemini::Part {
        data: Some(gemini::PartData::CodeExecutionResult {
            code_execution_result: gemini::CodeExecutionResult {
                id: Some(id),
                outcome: gemini::CodeExecutionOutcome::Known(outcome),
                output,
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        ..Default::default()
    }
}

fn result_outcome(return_code: i64) -> gemini::CodeExecutionOutcomeKnown {
    if return_code == 0 {
        gemini::CodeExecutionOutcomeKnown::OutcomeOk
    } else {
        gemini::CodeExecutionOutcomeKnown::OutcomeFailed
    }
}

fn command_output(stdout: String, stderr: String) -> Option<String> {
    let output = [stdout, stderr]
        .into_iter()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    (!output.is_empty()).then_some(output)
}
