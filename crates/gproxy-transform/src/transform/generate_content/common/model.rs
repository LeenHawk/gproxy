use crate::protocol::{claude, openai};

pub(in crate::transform::generate_content) const DEFAULT_CLAUDE_MAX_TOKENS: u64 = 16_384;
pub(in crate::transform::generate_content) const DEFAULT_OPENAI_MODEL: &str = "unknown";

pub(in crate::transform::generate_content) fn openai_model_string(
    model: openai::OpenAiModelId,
) -> String {
    serde_json::to_value(model)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub(in crate::transform::generate_content) fn claude_model_string(
    model: claude::ClaudeModel,
) -> String {
    serde_json::to_value(model)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}
