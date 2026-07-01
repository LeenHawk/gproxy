use crate::protocol::{claude, gemini, openai};

use super::reasoning::openai_reasoning_effort_to_gemini;
use super::response_format::apply_openai_response_format;
use super::thinking::claude_thinking_to_gemini;
use crate::transform::count_tokens::common::scalar::u64_to_i32;
use crate::transform::count_tokens::common::util::json_value;

pub(in crate::transform::count_tokens) fn openai_generation_config_to_gemini(
    reasoning: Option<openai::ReasoningConfig>,
    text: Option<openai::TextConfig>,
) -> Option<gemini::GenerationConfig> {
    let mut config = gemini::GenerationConfig::default();

    if let Some(reasoning) = reasoning {
        config.thinking_config = Some(gemini::ThinkingConfig {
            include_thoughts: None,
            thinking_budget: None,
            thinking_level: reasoning.effort.map(openai_reasoning_effort_to_gemini),
            extra: Default::default(),
        });
    }

    if let Some(text) = text.and_then(|text| text.format) {
        apply_openai_response_format(&mut config, text);
    }

    non_empty_generation_config(config)
}

pub(in crate::transform::count_tokens) fn claude_generation_config_to_gemini(
    output_config: Option<claude::OutputConfig>,
    output_format: Option<claude::JsonSchemaFormat>,
    thinking: Option<claude::ThinkingConfig>,
) -> Option<gemini::GenerationConfig> {
    let mut config = gemini::GenerationConfig::default();

    let output_format = output_config
        .as_ref()
        .and_then(|config| config.format.clone())
        .or(output_format);
    if let Some(format) = output_format {
        config.response_mime_type = Some(gemini::ResponseMimeType::Known(
            gemini::ResponseMimeTypeKnown::ApplicationJson,
        ));
        config.response_json_schema = Some(json_value(format.schema));
    }

    if let Some(task_budget) = output_config.and_then(|config| config.task_budget) {
        config.max_output_tokens = Some(u64_to_i32(task_budget.total));
    }

    if let Some(thinking) = thinking {
        config.thinking_config = Some(claude_thinking_to_gemini(thinking));
    }

    non_empty_generation_config(config)
}

fn non_empty_generation_config(
    config: gemini::GenerationConfig,
) -> Option<gemini::GenerationConfig> {
    if config == gemini::GenerationConfig::default() {
        None
    } else {
        Some(config)
    }
}
