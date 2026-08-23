use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::{Correlation, merge};

pub(super) fn function_call_block(
    call: gemini::FunctionCall,
    signature: Option<String>,
    rest: claude::JsonObject,
    correlation: &mut Correlation,
) -> Result<claude::ContentBlockParam, TransformError> {
    reject_rest(&rest, &call.rest, "Gemini function call")?;
    let input = call
        .args
        .ok_or_else(|| TransformError::shape("Gemini function call", "args is missing"))?;
    let id = correlation.function_call(call.id, &call.name);
    Ok(claude::ContentBlockParam::ToolUse(claude::ToolUseBlock {
        id,
        input,
        name: call.name,
        type_: claude::ToolUseBlockType::ToolUse,
        cache_control: None,
        caller: super::caller(signature),
        rest: merge(call.rest, rest),
    }))
}

pub(super) fn function_result_block(
    response: gemini::FunctionResponse,
    rest: claude::JsonObject,
    correlation: &mut Correlation,
) -> Result<claude::ContentBlockParam, TransformError> {
    if response.parts.is_some() || response.will_continue.is_some() || response.scheduling.is_some()
    {
        return Err(TransformError::unsupported(
            "Gemini function response",
            "parts, willContinue, or scheduling",
        ));
    }
    reject_rest(&rest, &response.rest, "Gemini function response")?;
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
    rest: claude::JsonObject,
    correlation: &mut Correlation,
) -> Result<claude::ContentBlockParam, TransformError> {
    reject_rest(&rest, &call.rest, "Gemini server tool call")?;
    let name = crate::models::common::wire_string(&call.tool_type)?;
    let input = call
        .args
        .ok_or_else(|| TransformError::shape("Gemini server tool call", "args is missing"))?;
    let id = correlation.function_call(call.id, &name);
    Ok(claude::ContentBlockParam::ToolUse(claude::ToolUseBlock {
        id,
        input,
        name,
        type_: claude::ToolUseBlockType::ToolUse,
        cache_control: None,
        caller: None,
        rest: merge(call.rest, rest),
    }))
}

pub(super) fn server_result_block(
    response: gemini::ToolResponse,
    rest: claude::JsonObject,
    correlation: &mut Correlation,
) -> Result<claude::ContentBlockParam, TransformError> {
    reject_rest(&rest, &response.rest, "Gemini server tool response")?;
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
            rest: merge(response.rest, rest),
        },
    ))
}

pub(super) fn text_block(text: String, rest: claude::JsonObject) -> claude::ContentBlockParam {
    claude::ContentBlockParam::Text(claude::TextBlock {
        text,
        type_: claude::TextBlockType::Text,
        cache_control: None,
        citations: None,
        rest,
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

fn reject_rest(
    outer: &claude::JsonObject,
    inner: &claude::JsonObject,
    wire: &'static str,
) -> Result<(), TransformError> {
    if !outer.is_empty() || !inner.is_empty() {
        return Err(TransformError::unsupported(wire, "rest fields"));
    }
    Ok(())
}
