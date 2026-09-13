use crate::transform::TransformError;
use serde_json::Value;

pub(crate) fn parse_tool_arguments(raw: &str, field: &str) -> Result<Value, TransformError> {
    let value: Value = serde_json::from_str(raw).map_err(|error| {
        TransformError::shape(field, format!("tool arguments are invalid JSON: {error}"))
    })?;
    if !value.is_object() {
        return Err(TransformError::shape(
            field,
            "tool arguments must be a JSON object",
        ));
    }
    Ok(value)
}

pub(crate) fn parse_result_arguments(raw: &str, field: &str) -> Result<Value, TransformError> {
    let value: Value = serde_json::from_str(raw).map_err(|error| {
        TransformError::invalid_result(field, format!("tool arguments are invalid JSON: {error}"))
    })?;
    if !value.is_object() {
        return Err(TransformError::invalid_result(
            field,
            "tool arguments must be a JSON object",
        ));
    }
    Ok(value)
}

pub(super) fn request_id(value: &str) -> Result<String, TransformError> {
    if value.is_empty() {
        return Err(TransformError::shape(
            "tool_call.id",
            "tool identity must not be empty",
        ));
    }
    Ok(value.to_owned())
}
pub(super) fn response_id(value: &str) -> Result<String, TransformError> {
    if value.is_empty() {
        return Err(TransformError::invalid_result(
            "tool_call.id",
            "tool identity must not be empty",
        ));
    }
    Ok(value.to_owned())
}
