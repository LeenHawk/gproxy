use gproxy_protocol::{claude, gemini};

use crate::TransformError;

mod bash;

pub(super) use bash::{request_bash_result, response_bash_result};

pub(super) fn call(
    id: String,
    input: claude::JsonObject,
    rest: serde_json::Map<String, serde_json::Value>,
) -> Result<gemini::Part, TransformError> {
    let code = input
        .get("command")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            input.get("commands").and_then(|value| {
                value.as_array().map(|commands| {
                    commands
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .collect::<Vec<_>>()
                        .join("\n")
                })
            })
        })
        .unwrap_or(serde_json::to_string(&input)?);
    // Gemini code_execution only exposes Python; keep the native call/result
    // lifecycle and stable id even when a Claude bash/editor call is approximate.
    Ok(part(gemini::PartData::ExecutableCode {
        executable_code: gemini::ExecutableCode {
            id: Some(id),
            language: gemini::ExecutableCodeLanguage::Known(
                gemini::ExecutableCodeLanguageKnown::Python,
            ),
            code,
            rest,
        },
        rest: Default::default(),
    }))
}

pub(super) fn result(block: claude::ToolResultBlock) -> Result<gemini::Part, TransformError> {
    let output = block.content.map(result_text).transpose()?;
    let outcome = match block.is_error {
        Some(false) => gemini::CodeExecutionOutcomeKnown::OutcomeOk,
        Some(true) => gemini::CodeExecutionOutcomeKnown::OutcomeFailed,
        None => gemini::CodeExecutionOutcomeKnown::OutcomeUnspecified,
    };
    Ok(part(gemini::PartData::CodeExecutionResult {
        code_execution_result: gemini::CodeExecutionResult {
            id: Some(block.tool_use_id),
            outcome: gemini::CodeExecutionOutcome::Known(outcome),
            output,
            rest: block.rest,
        },
        rest: Default::default(),
    }))
}

pub(super) fn result_text(content: claude::ToolResultContent) -> Result<String, TransformError> {
    Ok(match content {
        claude::ToolResultContent::Text(text) => text,
        claude::ToolResultContent::Blocks(blocks) => {
            let mut text = Vec::new();
            for block in blocks {
                let claude::ToolResultContentBlock::Text(block) = block else {
                    return Err(TransformError::unsupported(
                        "Claude tool result",
                        "non-text result block",
                    ));
                };
                if block.cache_control.is_some()
                    || block.citations.is_some()
                    || !block.rest.is_empty()
                {
                    return Err(TransformError::unsupported(
                        "Claude tool result text",
                        "cache, citations, or rest",
                    ));
                }
                text.push(block.text);
            }
            text.join("\n")
        }
        claude::ToolResultContent::Raw(raw) => {
            return Err(TransformError::unsupported(
                "Claude tool result",
                raw.to_string(),
            ));
        }
        _ => {
            return Err(TransformError::unsupported(
                "Claude tool result",
                "future content",
            ));
        }
    })
}

fn part(data: gemini::PartData) -> gemini::Part {
    gemini::Part {
        thought: None,
        thought_signature: None,
        part_metadata: None,
        media_resolution: None,
        data: Some(data),
        metadata: None,
        rest: Default::default(),
    }
}
