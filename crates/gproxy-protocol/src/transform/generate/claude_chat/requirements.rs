use crate::{
    transform::{Report, TransformError},
    wire::{claude::generate_content as cg, openai::chat},
};

pub(super) fn claude(
    input: &cg::GenerateContentRequestBody,
    report: &mut Report,
) -> Result<(), TransformError> {
    for (present, field) in [
        (input.container.is_some(), "container"),
        (input.context_management.is_some(), "context_management"),
        (
            input.fallback_credit_token.is_some(),
            "fallback_credit_token",
        ),
        (input.fallbacks.is_some(), "fallbacks"),
        (input.inference_geo.is_some(), "inference_geo"),
        (input.mcp_servers.is_some(), "mcp_servers"),
        (input.speed.is_some(), "speed"),
        (input.top_k.is_some(), "top_k"),
        (
            input
                .output_config
                .as_ref()
                .is_some_and(|config| config.task_budget.is_some()),
            "output_config.task_budget",
        ),
    ] {
        if present {
            report.omitted(field, "field has no target representation");
        }
    }
    for (present, field) in [
        (input.cache_control.is_some(), "cache_control"),
        (input.diagnostics.is_some(), "diagnostics"),
    ] {
        if present {
            report.omitted(field, "Chat has no equivalent advisory field");
        }
    }

    Ok(())
}

pub(super) fn chat(
    input: &chat::GenerateContentRequestBody,
    report: &mut Report,
) -> Result<(), TransformError> {
    for (present, field) in [
        (
            input.audio.as_ref().and_then(Option::as_ref).is_some(),
            "audio",
        ),
        (input.web_search_options.is_some(), "web_search_options"),
        (
            input
                .frequency_penalty
                .flatten()
                .is_some_and(|value| value != 0.0),
            "frequency_penalty",
        ),
        (
            input
                .presence_penalty
                .flatten()
                .is_some_and(|value| value != 0.0),
            "presence_penalty",
        ),
        (
            input
                .logit_bias
                .as_ref()
                .and_then(Option::as_ref)
                .is_some_and(|value| !value.is_empty()),
            "logit_bias",
        ),
        (input.logprobs.flatten() == Some(true), "logprobs"),
        (
            input.top_logprobs.flatten().is_some_and(|value| value != 0),
            "top_logprobs",
        ),
        (
            input.moderation.as_ref().and_then(Option::as_ref).is_some(),
            "moderation",
        ),
        (
            input.prediction.as_ref().and_then(Option::as_ref).is_some(),
            "prediction",
        ),
        (input.seed.flatten().is_some(), "seed"),
        (input.store.flatten() == Some(true), "store"),
        (input.verbosity.flatten().is_some(), "verbosity"),
    ] {
        if present {
            report.omitted(field, "field has no target representation");
        }
    }
    if let Some(modalities) = input.modalities.as_ref().and_then(Option::as_ref)
        && (modalities.is_empty()
            || modalities
                .iter()
                .any(|mode| !matches!(mode, chat::Modality::Text)))
    {
        report.omitted("modalities", "field has no target representation");
    }
    for (present, field) in [
        (
            input.metadata.as_ref().and_then(Option::as_ref).is_some(),
            "metadata",
        ),
        (
            input
                .prompt_cache_key
                .as_ref()
                .and_then(Option::as_ref)
                .is_some(),
            "prompt_cache_key",
        ),
        (input.prompt_cache_options.is_some(), "prompt_cache_options"),
        (
            input.prompt_cache_retention.flatten().is_some(),
            "prompt_cache_retention",
        ),
        (
            input
                .safety_identifier
                .as_ref()
                .and_then(Option::as_ref)
                .is_some(),
            "safety_identifier",
        ),
        (
            input
                .stream_options
                .as_ref()
                .and_then(Option::as_ref)
                .is_some(),
            "stream_options",
        ),
    ] {
        if present {
            report.omitted(field,"Claude has no equivalent advisory or delivery-option field; host must retain delivery options");
        }
    }
    Ok(())
}
