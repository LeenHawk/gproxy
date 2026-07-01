use crate::protocol::{claude, gemini, openai};

use super::reasoning::openai_reasoning_effort_to_claude_output;
use super::response_format::{
    gemini_generation_to_claude_output_format, openai_response_format_to_claude,
};
use super::thinking::gemini_thinking_to_claude_output_effort;
use crate::transform::count_tokens::common::scalar::i32_to_u64;

pub(in crate::transform::count_tokens) fn gemini_generation_to_claude_output_config(
    generation_config: Option<&gemini::GenerationConfig>,
) -> Option<claude::OutputConfig> {
    let config = generation_config?;
    let effort = config
        .thinking_config
        .as_ref()
        .and_then(gemini_thinking_to_claude_output_effort);
    let format = gemini_generation_to_claude_output_format(Some(config));
    let task_budget = config
        .max_output_tokens
        .map(|total| claude::TokenTaskBudget {
            total: i32_to_u64(total),
            type_: claude::TaskBudgetType::Known(claude::TaskBudgetTypeKnown::Tokens),
            remaining: None,
            extra: Default::default(),
        });

    if effort.is_none() && format.is_none() && task_budget.is_none() {
        return None;
    }

    Some(claude::OutputConfig {
        effort,
        format,
        task_budget,
        extra: Default::default(),
    })
}

pub(in crate::transform::count_tokens) fn openai_generation_to_claude_output_config(
    reasoning: Option<&openai::ReasoningConfig>,
    text: Option<&openai::TextConfig>,
) -> Option<claude::OutputConfig> {
    let effort = reasoning
        .and_then(|reasoning| reasoning.effort.as_ref())
        .map(openai_reasoning_effort_to_claude_output);
    let format = text
        .and_then(|text| text.format.as_ref())
        .and_then(openai_response_format_to_claude);

    if effort.is_none() && format.is_none() {
        return None;
    }

    Some(claude::OutputConfig {
        effort,
        format,
        task_budget: None,
        extra: Default::default(),
    })
}
