//! Current Responses controls projected onto target dialects without carrying
//! OpenAI cache identities or asynchronous execution contracts across providers.
use crate::{
    transform::{Report, TransformError},
    wire::openai::{
        chat as c,
        responses::{generate as r, input as i, tools as t},
    },
};

pub(crate) fn project_input(
    input: &mut Option<i::Input>,
    reasoning: &mut Option<Option<i::ReasoningConfig>>,
    report: &mut Report,
) {
    if let Some(i::Input::Items(items)) = input {
        items.retain(|item| {
            if let i::InputItem::ConfigurationUpdate(update) = item {
                if let Some(effort) = update.reasoning.as_ref().and_then(|v| v.effort) {
                    reasoning
                        .get_or_insert_with(|| Some(i::ReasoningConfig::builder().build()))
                        .get_or_insert_with(|| i::ReasoningConfig::builder().build())
                        .effort = Some(effort);
                    report.changed(
                        "input.configuration_update",
                        "latest reasoning effort applied to target generation",
                    );
                }
                return false;
            }
            let asynchronous = match item {
                i::InputItem::FunctionCall(v) => v.async_,
                i::InputItem::CustomToolCall(v) => v.async_,
                _ => None,
            };
            if asynchronous == Some(true) {
                report.omitted(
                    "input.async",
                    "target tool calls have no asynchronous continuation marker",
                );
            }
            true
        });
    }
}

pub(crate) fn project_request(
    input: &mut r::GenerateContentRequestBody,
    report: &mut Report,
) -> Result<(), TransformError> {
    if let Some(cache) = &input.prompt_cache_options {
        if cache.prewarm == Some(true) {
            return Err(TransformError::unsupported(
                "prompt_cache_options.prewarm",
                "cache-only warmup has no equivalent cross-protocol generation operation",
            ));
        }
        if cache
            .comparison_response_id
            .as_ref()
            .is_some_and(Option::is_some)
        {
            report.omitted(
                "prompt_cache_options.comparison_response_id",
                "cache comparisons are bound to the original OpenAI response",
            );
        }
    }
    project_input(&mut input.input, &mut input.reasoning, report);
    for tool in input.tools.as_ref().into_iter().flatten() {
        let asynchronous = match tool {
            t::Tool::Function(v) => v.async_ == Some(true),
            t::Tool::Custom(v) => v.async_ == Some(true),
            t::Tool::Namespace(v) => v.tools.iter().any(|tool| match tool {
                t::NamespaceToolDefinition::Function(v) => v.async_ == Some(true),
                t::NamespaceToolDefinition::Custom(v) => v.async_ == Some(true),
            }),
            _ => false,
        };
        if asynchronous {
            report.omitted(
                "tools.async",
                "tool definition retained without OpenAI asynchronous scheduling",
            );
        }
    }
    Ok(())
}

fn gpt6(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    ["gpt-6-astra", "gpt-6-sol", "gpt-6-luna"]
        .iter()
        .any(|name| crate::transform::instructions::family(&model, name))
}

pub(crate) fn target_responses(out: &mut r::GenerateContentRequestBody, report: &mut Report) {
    let model = out
        .model
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !gpt6(&model) {
        return;
    }
    target_reasoning(&model, &mut out.reasoning, report);
    let effort = out
        .reasoning
        .as_ref()
        .and_then(Option::as_ref)
        .and_then(|v| v.effort.flatten());
    if effort != Some(i::ReasoningEffort::None) {
        for (present, field) in [
            (out.temperature.take().is_some(), "temperature"),
            (out.top_p.take().is_some(), "top_p"),
            (out.top_logprobs.take().is_some(), "top_logprobs"),
        ] {
            if present {
                report.omitted(
                    field,
                    "GPT-6 reasoning does not support sampling controls or log probabilities",
                );
            }
        }
        if let Some(Some(include)) = &mut out.include {
            include.retain(|v| {
                if *v == r::ResponseIncludable::OutputLogprobs {
                    report.omitted(
                        "include.message.output_text.logprobs",
                        "GPT-6 reasoning does not expose log probabilities",
                    );
                    false
                } else {
                    true
                }
            });
        }
    }
}

pub(crate) fn target_chat(out: &mut c::GenerateContentRequestBody, report: &mut Report) {
    let model = out.model.to_ascii_lowercase();
    if !gpt6(&model) {
        return;
    }
    let astra = crate::transform::instructions::family(&model, "gpt-6-astra");
    if out.reasoning_effort == Some(Some(c::ReasoningEffort::Minimal))
        || astra && out.reasoning_effort == Some(Some(c::ReasoningEffort::None))
    {
        out.reasoning_effort = Some(Some(c::ReasoningEffort::Low));
        report.changed(
            "reasoning_effort",
            "GPT-6 target uses low for unsupported none/minimal effort",
        );
    }
    if out.reasoning_effort.flatten() != Some(c::ReasoningEffort::None) {
        for (present, field) in [
            (out.temperature.take().is_some(), "temperature"),
            (out.top_p.take().is_some(), "top_p"),
            (out.top_logprobs.take().is_some(), "top_logprobs"),
            (out.logprobs.take().is_some(), "logprobs"),
        ] {
            if present {
                report.omitted(
                    field,
                    "GPT-6 reasoning does not support sampling controls or log probabilities",
                );
            }
        }
    }
    if astra || out.reasoning_effort.flatten() != Some(c::ReasoningEffort::None) {
        for (present, field) in [
            (out.tools.take().is_some(), "tools"),
            (out.tool_choice.take().is_some(), "tool_choice"),
            (
                out.parallel_tool_calls.take().is_some(),
                "parallel_tool_calls",
            ),
            (out.functions.take().is_some(), "functions"),
            (out.function_call.take().is_some(), "function_call"),
        ] {
            if present {
                report.omitted(
                    field,
                    "tool calling for this GPT-6 reasoning mode requires Responses",
                );
            }
        }
    }
}

pub(crate) fn target_reasoning(
    model: &str,
    reasoning: &mut Option<Option<i::ReasoningConfig>>,
    report: &mut Report,
) {
    let model = model.to_ascii_lowercase();
    if !gpt6(&model) {
        return;
    }
    let astra = crate::transform::instructions::family(&model, "gpt-6-astra");
    if let Some(Some(reasoning)) = reasoning
        && (matches!(reasoning.effort, Some(Some(i::ReasoningEffort::Minimal)))
            || astra && reasoning.effort == Some(Some(i::ReasoningEffort::None)))
    {
        reasoning.effort = Some(Some(i::ReasoningEffort::Low));
        report.changed(
            "reasoning.effort",
            "GPT-6 target uses low for unsupported none/minimal effort",
        );
    }
}
