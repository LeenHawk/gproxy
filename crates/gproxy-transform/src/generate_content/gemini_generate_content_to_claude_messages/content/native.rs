use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::Correlation;

pub(super) fn request_call(
    code: gemini::ExecutableCode,
    correlation: &mut Correlation,
    rest: claude::JsonObject,
    signature: Option<String>,
) -> Result<claude::ContentBlockParam, TransformError> {
    if !rest.is_empty() || !code.rest.is_empty() {
        return Err(TransformError::unsupported(
            "Gemini executable code",
            "rest fields",
        ));
    }
    let id = correlation.code_call(code.id.clone());
    Ok(claude::ContentBlockParam::ToolUse(claude::ToolUseBlock {
        id,
        input: code_input(code)?,
        name: "bash".into(),
        type_: claude::ToolUseBlockType::ToolUse,
        cache_control: None,
        caller: super::caller(signature),
        rest,
    }))
}

pub(super) fn request_result(
    result: gemini::CodeExecutionResult,
    correlation: &mut Correlation,
    rest: claude::JsonObject,
) -> Result<claude::ContentBlockParam, TransformError> {
    if !rest.is_empty() || !result.rest.is_empty() {
        return Err(TransformError::unsupported(
            "Gemini code result",
            "rest fields",
        ));
    }
    Ok(claude::ContentBlockParam::ToolResult(
        claude::ToolResultBlock {
            tool_use_id: correlation.code_result(result.id)?,
            type_: claude::ToolResultBlockType::ToolResult,
            cache_control: None,
            content: result.output.map(claude::ToolResultContent::Text),
            is_error: outcome_error(&result.outcome),
            rest: Default::default(),
        },
    ))
}

pub(super) fn response_call(
    code: gemini::ExecutableCode,
    correlation: &mut Correlation,
    rest: claude::JsonObject,
    signature: Option<String>,
) -> Result<claude::ContentBlock, TransformError> {
    let id = correlation.code_call(code.id.clone());
    Ok(claude::ResponseContentBlock::ToolUse(
        claude::ResponseToolUseBlock {
            id,
            input: code_input(code)?,
            name: "bash".into(),
            type_: claude::ToolUseBlockType::ToolUse,
            caller: super::caller(signature),
            rest,
        },
    ))
}

pub(super) fn response_result(
    result: gemini::CodeExecutionResult,
    correlation: &mut Correlation,
    rest: claude::JsonObject,
) -> Result<claude::ContentBlock, TransformError> {
    let tool_use_id = correlation.code_result(result.id)?;
    let failed = outcome_error(&result.outcome)
        .ok_or_else(|| TransformError::shape("Gemini code result", "outcome is unspecified"))?;
    let output = result
        .output
        .ok_or_else(|| TransformError::shape("Gemini code result", "output is missing"))?;
    Ok(claude::ResponseContentBlock::BashCodeExecutionToolResult(
        claude::ResponseBashCodeExecutionToolResultBlock {
            content: claude::ResponseBashCodeExecutionToolResultContent::Result(
                claude::BashCodeExecutionResultBlock {
                    content: Vec::new(),
                    return_code: if failed { 1 } else { 0 },
                    stderr: if failed {
                        output.clone()
                    } else {
                        String::new()
                    },
                    stdout: if failed { String::new() } else { output },
                    type_: claude::BashCodeExecutionResultBlockType::BashCodeExecutionResult,
                    rest: result.rest,
                },
            ),
            tool_use_id,
            type_: claude::BashCodeExecutionToolResultBlockType::BashCodeExecutionToolResult,
            rest,
        },
    ))
}

fn code_input(code: gemini::ExecutableCode) -> Result<claude::JsonObject, TransformError> {
    let mut input = code.rest;
    input.insert("command".into(), code.code.into());
    input.insert("language".into(), serde_json::to_value(code.language)?);
    Ok(input)
}

fn outcome_error(outcome: &gemini::CodeExecutionOutcome) -> Option<bool> {
    match outcome {
        gemini::CodeExecutionOutcome::Known(gemini::CodeExecutionOutcomeKnown::OutcomeOk) => {
            Some(false)
        }
        gemini::CodeExecutionOutcome::Known(
            gemini::CodeExecutionOutcomeKnown::OutcomeFailed
            | gemini::CodeExecutionOutcomeKnown::OutcomeDeadlineExceeded,
        ) => Some(true),
        gemini::CodeExecutionOutcome::Known(
            gemini::CodeExecutionOutcomeKnown::OutcomeUnspecified,
        )
        | gemini::CodeExecutionOutcome::Unknown(_) => None,
        _ => None,
    }
}
