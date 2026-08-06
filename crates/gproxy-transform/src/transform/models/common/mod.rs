use serde::Serialize;

use crate::{
    protocol::{claude, gemini, openai},
    transform::TransformError,
};

pub(in crate::transform::models) mod claude_to_gemini;
pub(in crate::transform::models) mod claude_to_openai;
pub(in crate::transform::models) mod gemini_to_claude;
pub(in crate::transform::models) mod gemini_to_openai;
pub(in crate::transform::models) mod openai_to_claude;
pub(in crate::transform::models) mod openai_to_gemini;

pub(in crate::transform::models) const DEFAULT_CREATED_AT: &str = "1970-01-01T00:00:00Z";
pub(in crate::transform::models) const DEFAULT_OPENAI_OWNED_BY: &str = "unknown";

pub(in crate::transform::models) fn wire_string<T: Serialize>(
    value: &T,
    field: &'static str,
) -> Result<String, TransformError> {
    let value = serde_json::to_value(value).map_err(|error| TransformError::Serialization {
        reason: error.to_string(),
    })?;
    value
        .as_str()
        .map(str::to_owned)
        .ok_or(TransformError::InvalidInput {
            reason: format!("{field} did not serialize as a string"),
        })
}

pub(in crate::transform::models) const fn openai_model_object() -> openai::ModelObjectType {
    openai::ModelObjectType::Model
}

pub(in crate::transform::models) const fn claude_model_object() -> claude::ModelObjectType {
    claude::ModelObjectType::Known(claude::ModelObjectTypeKnown::Model)
}

pub(in crate::transform::models) fn u64_to_i32_default(value: u64) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

pub(in crate::transform::models) fn i32_to_u64_default(value: i32) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

pub(in crate::transform::models) fn claude_model_id(input: &claude::ModelInfo) -> String {
    match &input.id {
        claude::ClaudeModel::Known(known) => serde_json::to_value(known)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default(),
        claude::ClaudeModel::Unknown(value) => value.clone(),
        _ => {
            unreachable!("new non-exhaustive protocol variant requires a lockstep transform update")
        }
    }
}

pub(in crate::transform::models) fn gemini_model_id(input: &gemini::Model) -> String {
    input
        .base_model_id
        .clone()
        .or_else(|| input.name.clone())
        .unwrap_or_default()
}
