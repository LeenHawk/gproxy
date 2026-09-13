use crate::{
    transform::{Report, TransformError},
    wire::{
        claude::{count_tokens as cc, generate_content as c},
        openai::responses::{generate as r, input as i},
    },
};
pub(super) fn to_responses(
    input: &c::GenerateContentRequestBody,
    out: &mut r::GenerateContentRequestBody,
    report: &mut Report,
) -> Result<(), TransformError> {
    for (present, name) in [
        (input.stop_sequences.is_some(), "stop_sequences"),
        (input.container.is_some(), "container"),
        (input.context_management.is_some(), "context_management"),
        (
            input.fallback_credit_token.is_some(),
            "fallback_credit_token",
        ),
        (input.fallbacks.is_some(), "fallbacks"),
        (input.inference_geo.is_some(), "inference_geo"),
        (input.speed.is_some(), "speed"),
        (input.top_k.is_some(), "top_k"),
        (
            input
                .output_config
                .as_ref()
                .is_some_and(|v| v.task_budget.is_some()),
            "task_budget",
        ),
    ] {
        if present {
            return Err(TransformError::unsupported(
                name,
                "Claude execution requirement needs target capability",
            ));
        }
    }
    if input.max_tokens <= 0 {
        return Err(TransformError::shape(
            "max_tokens",
            "positive token budget required",
        ));
    }
    out.max_output_tokens = Some(Some(input.max_tokens));
    out.stream = input.stream.map(Some);
    out.temperature = number(input.temperature)?;
    out.top_p = number(input.top_p)?;
    out.user = input.metadata.as_ref().and_then(|v| v.user_id.clone());
    out.service_tier = input.service_tier.map(|v| {
        Some(match v {
            c::ServiceTier::Auto => r::ServiceTier::Auto,
            c::ServiceTier::StandardOnly => r::ServiceTier::Default,
        })
    });
    let mut effort = input
        .output_config
        .as_ref()
        .and_then(|v| v.effort)
        .map(|v| match v {
            cc::Effort::Low => i::ReasoningEffort::Low,
            cc::Effort::Medium => i::ReasoningEffort::Medium,
            cc::Effort::High => i::ReasoningEffort::High,
            cc::Effort::Xhigh => i::ReasoningEffort::Xhigh,
            cc::Effort::Max => i::ReasoningEffort::Max,
        });
    match &input.thinking {
        Some(cc::ThinkingConfig::Disabled(_)) => {
            if effort.is_some() {
                return Err(TransformError::shape(
                    "thinking",
                    "disabled thinking conflicts with effort",
                ));
            }
            effort = Some(i::ReasoningEffort::None);
        }
        Some(cc::ThinkingConfig::Enabled(_)) => {
            return Err(TransformError::unsupported(
                "thinking.budget",
                "Responses effort cannot enforce exact thinking token budget",
            ));
        }
        Some(cc::ThinkingConfig::Adaptive(config)) => {
            if effort.is_none() {
                return Err(TransformError::missing_metadata(
                    "adaptive target reasoning effort",
                ));
            }
            if config.display.is_some() {
                return Err(TransformError::unsupported(
                    "thinking.display",
                    "Responses has no equivalent display contract",
                ));
            }
        }
        None => {}
    }
    if let Some(effort) = effort {
        out.reasoning = Some(Some(
            i::ReasoningConfig::builder().effort(Some(effort)).build(),
        ));
    }
    let current = input.output_config.as_ref().and_then(|v| v.format.as_ref());
    if let (Some(a), Some(b)) = (current, input.output_format.as_ref())
        && a.schema != b.schema
    {
        return Err(TransformError::shape(
            "output_format",
            "conflicting schemas",
        ));
    }
    if let Some(format) = current.or(input.output_format.as_ref()) {
        let schema = format
            .schema
            .as_object()
            .cloned()
            .ok_or_else(|| TransformError::shape("schema", "JSON object schema required"))?;
        out.text = Some(
            i::TextConfig::builder()
                .format(i::TextFormat::JsonSchema(
                    i::TextFormatJsonSchema::builder("claude_output".into(), schema)
                        .strict(Some(true))
                        .build(),
                ))
                .build(),
        );
    }
    if input.cache_control.is_some() || input.diagnostics.is_some() {
        report.omitted(
            "advisory",
            "Responses lacks Claude cache/diagnostic request metadata",
        );
    }
    Ok(())
}
pub(super) fn to_claude(
    input: &r::GenerateContentRequestBody,
    out: &mut c::GenerateContentRequestBody,
    report: &mut Report,
) -> Result<(), TransformError> {
    for (present, name) in [
        (
            input
                .previous_response_id
                .as_ref()
                .and_then(Option::as_ref)
                .is_some(),
            "previous_response_id",
        ),
        (
            input
                .conversation
                .as_ref()
                .and_then(Option::as_ref)
                .is_some(),
            "conversation",
        ),
        (
            input.prompt.as_ref().and_then(Option::as_ref).is_some(),
            "prompt",
        ),
    ] {
        if present {
            return Err(TransformError::missing_metadata(format!(
                "{name} resolved history"
            )));
        }
    }
    for (present, name) in [
        (input.background.flatten() == Some(true), "background"),
        (
            input
                .context_management
                .as_ref()
                .and_then(Option::as_ref)
                .is_some(),
            "context_management",
        ),
        (input.max_tool_calls.flatten().is_some(), "max_tool_calls"),
        (
            input.moderation.as_ref().and_then(Option::as_ref).is_some(),
            "moderation",
        ),
        (input.store.flatten() == Some(true), "store"),
        (
            input.truncation.flatten() == Some(r::GenerateTruncation::Auto),
            "truncation",
        ),
        (input.top_logprobs.flatten().is_some(), "top_logprobs"),
    ] {
        if present {
            return Err(TransformError::unsupported(
                name,
                "Responses execution policy needs Claude adapter",
            ));
        }
    }
    out.stream = input.stream.flatten();
    out.temperature = float(&input.temperature)?;
    out.top_p = float(&input.top_p)?;
    out.service_tier = match input.service_tier.flatten() {
        None => None,
        Some(r::ServiceTier::Auto) => Some(c::ServiceTier::Auto),
        Some(r::ServiceTier::Default) => Some(c::ServiceTier::StandardOnly),
        Some(
            r::ServiceTier::Flex
            | r::ServiceTier::Scale
            | r::ServiceTier::Priority
            | r::ServiceTier::Fast,
        ) => {
            return Err(TransformError::unsupported(
                "service_tier",
                "no equivalent Claude tier",
            ));
        }
    };
    if let Some(user) = &input.user {
        out.metadata = Some(c::Metadata::builder().user_id(user.clone()).build());
    }
    if let Some(config) = input.reasoning.as_ref().and_then(Option::as_ref) {
        if config.mode.is_some()
            || config.context.as_ref().and_then(Option::as_ref).is_some()
            || config.summary.flatten().is_some()
            || config.generate_summary.flatten().is_some()
        {
            return Err(TransformError::unsupported(
                "reasoning",
                "reasoning summary/context has no exact Claude control",
            ));
        }
        if let Some(effort) = config.effort.flatten() {
            match effort {
                i::ReasoningEffort::None => {
                    out.thinking = Some(cc::ThinkingConfig::Disabled(
                        cc::ThinkingDisabled::builder().build(),
                    ))
                }
                i::ReasoningEffort::Minimal => {
                    return Err(TransformError::unsupported(
                        "effort",
                        "Claude has no minimal level",
                    ));
                }
                i::ReasoningEffort::Low
                | i::ReasoningEffort::Medium
                | i::ReasoningEffort::High
                | i::ReasoningEffort::Xhigh
                | i::ReasoningEffort::Max => {
                    let effort = match effort {
                        i::ReasoningEffort::Low => cc::Effort::Low,
                        i::ReasoningEffort::Medium => cc::Effort::Medium,
                        i::ReasoningEffort::High => cc::Effort::High,
                        i::ReasoningEffort::Xhigh => cc::Effort::Xhigh,
                        i::ReasoningEffort::Max => cc::Effort::Max,
                        i::ReasoningEffort::None | i::ReasoningEffort::Minimal => unreachable!(),
                    };
                    out.output_config = Some(cc::OutputConfig::builder().effort(effort).build());
                    out.thinking = Some(cc::ThinkingConfig::Adaptive(
                        cc::ThinkingAdaptive::builder().build(),
                    ));
                }
            }
        }
    }
    if let Some(text) = &input.text {
        if text.verbosity.flatten().is_some() {
            return Err(TransformError::unsupported(
                "text.verbosity",
                "no Claude verbosity control",
            ));
        }
        if let Some(format) = &text.format {
            match format {
                i::TextFormat::Text(_) => {}
                i::TextFormat::JsonObject(_) => {
                    return Err(TransformError::unsupported(
                        "text.format",
                        "free JSON object has no verified Claude structured-output equivalent",
                    ));
                }
                i::TextFormat::JsonSchema(schema) => {
                    out.output_format = Some(
                        cc::JsonOutputFormat::builder(
                            cc::JsonOutputFormatType::JsonSchema,
                            serde_json::Value::Object(schema.schema.clone()),
                        )
                        .build(),
                    );
                    report.changed(
                        "text.format",
                        "Claude enforces schema without separate name/description/strict wrapper",
                    );
                }
            }
        }
    }
    for (present, name) in [
        (input.metadata.is_some(), "metadata"),
        (input.include.is_some(), "include"),
        (input.prompt_cache_key.is_some(), "prompt_cache_key"),
        (input.prompt_cache_options.is_some(), "prompt_cache_options"),
        (
            input.prompt_cache_retention.is_some(),
            "prompt_cache_retention",
        ),
        (input.safety_identifier.is_some(), "safety_identifier"),
        (input.stream_options.is_some(), "stream_options"),
    ] {
        if present {
            report.omitted(
                name,
                "Claude lacks equivalent metadata/delivery field; host retains invocation options",
            );
        }
    }
    Ok(())
}
fn number(v: Option<f64>) -> Result<Option<Option<serde_json::Number>>, TransformError> {
    v.map(|v| {
        serde_json::Number::from_f64(v)
            .ok_or_else(|| TransformError::shape("sampling", "non-finite number"))
    })
    .transpose()
    .map(|v| v.map(Some))
}
fn float(v: &Option<Option<serde_json::Number>>) -> Result<Option<f64>, TransformError> {
    v.as_ref()
        .and_then(Option::as_ref)
        .map(|v| {
            v.as_f64()
                .filter(|v| v.is_finite())
                .ok_or_else(|| TransformError::shape("sampling", "invalid double"))
        })
        .transpose()
}
