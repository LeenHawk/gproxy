use crate::protocol::{claude, openai};

pub(in crate::transform::count_tokens) const DEFAULT_MODEL: &str = "unknown";

pub(in crate::transform::count_tokens) fn u64_to_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

pub(in crate::transform::count_tokens) fn u64_to_i32(value: u64) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

pub(in crate::transform::count_tokens) fn u32_to_u64(value: u32) -> u64 {
    u64::from(value)
}

pub(in crate::transform::count_tokens) fn u32_to_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

pub(in crate::transform::count_tokens) fn i32_to_u32(value: i32) -> u32 {
    u32::try_from(value).unwrap_or_default()
}

pub(in crate::transform::count_tokens) fn i32_to_u64(value: i32) -> u64 {
    u64::try_from(value).unwrap_or_default()
}

pub(in crate::transform::count_tokens) fn openai_model_string(
    model: Option<openai::OpenAiModelId>,
) -> String {
    model
        .as_ref()
        .map(model_to_string)
        .unwrap_or_else(|| DEFAULT_MODEL.to_owned())
}

pub(in crate::transform::count_tokens) fn claude_model_string(
    model: &claude::ClaudeModel,
) -> String {
    model_to_string(model)
}

pub(in crate::transform::count_tokens) fn gemini_model_string(model: Option<String>) -> String {
    model.unwrap_or_else(|| DEFAULT_MODEL.to_owned())
}

fn model_to_string<T: serde::Serialize>(model: &T) -> String {
    let Ok(value) = serde_json::to_value(model) else {
        return DEFAULT_MODEL.to_owned();
    };
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| DEFAULT_MODEL.to_owned())
}
