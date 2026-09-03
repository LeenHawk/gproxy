use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::Correlation;

pub(super) fn function_call_block(
    call: gemini::FunctionCall,
    correlation: &mut Correlation,
) -> Result<claude::ContentBlockParam, TransformError> {
    let input = call.args.unwrap_or_default();
    let id = correlation.function_call(call.id, &call.name);
    Ok(claude::ContentBlockParam::ToolUse(claude::ToolUseBlock {
        id,
        input,
        name: call.name,
        type_: claude::ToolUseBlockType::ToolUse,
        cache_control: None,
        caller: None,
        rest: Default::default(),
    }))
}

pub(super) fn function_result_block(
    response: gemini::FunctionResponse,
    correlation: &mut Correlation,
) -> Result<claude::ContentBlockParam, TransformError> {
    let id = correlation.function_result(response.id, &response.name)?;
    let (content, is_error) = function_response_text(response.response)?;
    Ok(claude::ContentBlockParam::ToolResult(
        claude::ToolResultBlock {
            tool_use_id: id,
            type_: claude::ToolResultBlockType::ToolResult,
            cache_control: None,
            content,
            is_error,
            rest: Default::default(),
        },
    ))
}

pub(super) fn server_call_block(
    call: gemini::ToolCall,
    correlation: &mut Correlation,
) -> Result<claude::ContentBlockParam, TransformError> {
    let name = crate::models::common::wire_string(&call.tool_type)?;
    let input = call.args.unwrap_or_default();
    let id = correlation.function_call(call.id, &name);
    Ok(claude::ContentBlockParam::ToolUse(claude::ToolUseBlock {
        id,
        input,
        name,
        type_: claude::ToolUseBlockType::ToolUse,
        cache_control: None,
        caller: None,
        rest: Default::default(),
    }))
}

pub(super) fn server_result_block(
    response: gemini::ToolResponse,
    correlation: &mut Correlation,
) -> Result<claude::ContentBlockParam, TransformError> {
    let name = crate::models::common::wire_string(&response.tool_type)?;
    let id = correlation.function_result(response.id, &name)?;
    let content = response.response.map(response_text).transpose()?.flatten();
    Ok(claude::ContentBlockParam::ToolResult(
        claude::ToolResultBlock {
            tool_use_id: id,
            type_: claude::ToolResultBlockType::ToolResult,
            cache_control: None,
            content,
            is_error: None,
            rest: Default::default(),
        },
    ))
}

pub(super) fn text_block(text: String) -> claude::ContentBlockParam {
    claude::ContentBlockParam::Text(claude::TextBlock {
        text,
        type_: claude::TextBlockType::Text,
        cache_control: None,
        citations: None,
        rest: Default::default(),
    })
}

fn response_text(
    response: gemini::JsonMap,
) -> Result<Option<claude::ToolResultContent>, TransformError> {
    if response.is_empty() {
        return Ok(None);
    }
    if response.len() == 1
        && let Some(text) = response.get("output").and_then(serde_json::Value::as_str)
    {
        return Ok(Some(claude::ToolResultContent::Text(text.to_owned())));
    }
    Ok(Some(claude::ToolResultContent::Text(
        serde_json::to_string(&response)?,
    )))
}

fn function_response_text(
    response: gemini::JsonMap,
) -> Result<(Option<claude::ToolResultContent>, Option<bool>), TransformError> {
    if response.len() == 1 {
        if let Some(text) = response.get("error").and_then(serde_json::Value::as_str) {
            return Ok((
                Some(claude::ToolResultContent::Text(text.to_owned())),
                Some(true),
            ));
        }
        if let Some(text) = response.get("output").and_then(serde_json::Value::as_str) {
            return Ok((
                Some(claude::ToolResultContent::Text(text.to_owned())),
                Some(false),
            ));
        }
    }
    Ok((response_text(response)?, None))
}
