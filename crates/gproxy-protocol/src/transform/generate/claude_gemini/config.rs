use crate::{
    transform::{Report, TransformError},
    wire::{
        claude::{count_tokens as cc, generate_content as c},
        gemini as g,
    },
};
pub(super) fn to_gemini(
    input: &c::GenerateContentRequestBody,
    report: &mut Report,
) -> Result<g::GenerationConfig, TransformError> {
    for (present, name) in [
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
                "Claude execution requirement needs Gemini invocation capability",
            ));
        }
    }
    if input.max_tokens <= 0 {
        return Err(TransformError::shape(
            "max_tokens",
            "positive budget required",
        ));
    }
    for number in [input.temperature, input.top_p].into_iter().flatten() {
        if !number.is_finite() {
            return Err(TransformError::shape("sampling", "non-finite value"));
        }
    }
    let mut config = g::GenerationConfig::builder()
        .max_output_tokens(input.max_tokens)
        .build();
    config.temperature = input.temperature;
    config.top_p = input.top_p;
    config.top_k = input.top_k;
    config.stop_sequences = input.stop_sequences.clone();
    let effort = input.output_config.as_ref().and_then(|v| v.effort);
    config.thinking_config = match &input.thinking {
        Some(cc::ThinkingConfig::Disabled(_)) => {
            if effort.is_some() {
                return Err(TransformError::shape(
                    "thinking",
                    "disabled thinking conflicts with effort",
                ));
            }
            Some(g::ThinkingConfig::builder().thinking_budget(0).build())
        }
        Some(cc::ThinkingConfig::Enabled(v)) => {
            if v.budget_tokens <= 0 || effort.is_some() {
                return Err(TransformError::shape(
                    "thinking",
                    "positive budget without conflicting effort required",
                ));
            }
            let mut thinking = g::ThinkingConfig::builder()
                .thinking_budget(v.budget_tokens)
                .build();
            thinking.include_thoughts = v.display.map(display);
            Some(thinking)
        }
        Some(cc::ThinkingConfig::Adaptive(v)) => {
            let mut thinking = g::ThinkingConfig::builder().build();
            thinking.thinking_level = effort.map(level).transpose()?;
            if thinking.thinking_level.is_none() {
                thinking.thinking_budget = Some(-1);
            }
            thinking.include_thoughts = v.display.map(display);
            Some(thinking)
        }
        None => effort
            .map(|e| level(e).map(|l| g::ThinkingConfig::builder().thinking_level(l).build()))
            .transpose()?,
    };
    let current = input.output_config.as_ref().and_then(|v| v.format.as_ref());
    if let (Some(a), Some(b)) = (current, input.output_format.as_ref())
        && a.schema != b.schema
    {
        return Err(TransformError::shape(
            "output_format",
            "conflicting output schemas",
        ));
    }
    if let Some(format) = current.or(input.output_format.as_ref()) {
        config.response_mime_type = Some("application/json".into());
        config.response_json_schema = Some(format.schema.clone());
        report.changed(
            "response_json_schema",
            "selected Gemini endpoint must support the requested raw schema keywords",
        );
    }
    for (present, name) in [
        (input.cache_control.is_some(), "cache_control"),
        (input.diagnostics.is_some(), "diagnostics"),
        (input.metadata.is_some(), "metadata"),
        (input.service_tier.is_some(), "service_tier"),
        (input.stream.is_some(), "stream"),
    ] {
        if present {
            report.omitted(name,"Gemini has no equivalent body field; invocation retains delivery and metadata options");
        }
    }
    Ok(config)
}
fn display(v: cc::ThinkingDisplay) -> bool {
    match v {
        cc::ThinkingDisplay::Summarized => true,
        cc::ThinkingDisplay::Omitted => false,
    }
}
fn level(v: cc::Effort) -> Result<g::ThinkingLevel, TransformError> {
    match v {
        cc::Effort::Low => Ok(g::ThinkingLevel::Low),
        cc::Effort::Medium => Ok(g::ThinkingLevel::Medium),
        cc::Effort::High => Ok(g::ThinkingLevel::High),
        cc::Effort::Xhigh | cc::Effort::Max => Err(TransformError::unsupported(
            "effort",
            "Gemini has no xhigh/max effort level",
        )),
    }
}
pub(super) fn to_claude(
    input: &g::GenerateContentRequestBody,
    out: &mut c::GenerateContentRequestBody,
    report: &mut Report,
) -> Result<(), TransformError> {
    if input.cached_content.is_some() {
        return Err(TransformError::missing_metadata(
            "cached_content resolved history",
        ));
    }
    if input
        .safety_settings
        .as_ref()
        .is_some_and(|v| !v.is_empty())
        || input.store == Some(true)
    {
        return Err(TransformError::unsupported(
            "safety_settings/store",
            "Claude needs explicit host enforcement",
        ));
    }
    if input.service_tier.is_some() {
        report.omitted(
            "service_tier",
            "Claude has no equivalent Gemini service tier field",
        );
    }
    let Some(config) = &input.generation_config else {
        return Ok(());
    };
    if config.candidate_count.is_some_and(|n| n != 1) {
        return Err(TransformError::unsupported(
            "candidate_count",
            "multiple Claude candidates require fan-out invocation",
        ));
    }
    for (present, name) in [
        (config.seed.is_some(), "seed"),
        (
            config.presence_penalty.is_some_and(|n| n != 0.0),
            "presence_penalty",
        ),
        (
            config.frequency_penalty.is_some_and(|n| n != 0.0),
            "frequency_penalty",
        ),
        (
            config.response_logprobs == Some(true) || config.logprobs.is_some(),
            "logprobs",
        ),
        (
            config.enable_enhanced_civic_answers.is_some(),
            "enhanced_civic_answers",
        ),
        (config.speech_config.is_some(), "speech_config"),
        (config.image_config.is_some(), "image_config"),
        (config.media_resolution.is_some(), "media_resolution"),
        (config.response_format.is_some(), "response_format"),
    ] {
        if present {
            return Err(TransformError::unsupported(
                name,
                "Claude has no equivalent generation control",
            ));
        }
    }
    if config
        .response_modalities
        .as_ref()
        .is_some_and(|v| v.iter().any(|v| !matches!(v, g::Modality::Text)))
    {
        return Err(TransformError::unsupported(
            "response_modalities",
            "Claude only emits this text/tool response contract",
        ));
    }
    for number in [config.temperature, config.top_p].into_iter().flatten() {
        if !number.is_finite() {
            return Err(TransformError::shape("sampling", "non-finite value"));
        }
    }
    out.temperature = config.temperature;
    out.top_p = config.top_p;
    out.top_k = config.top_k;
    out.stop_sequences = config.stop_sequences.clone();
    if let Some(thinking) = &config.thinking_config {
        if thinking.thinking_budget.is_some() && thinking.thinking_level.is_some() {
            return Err(TransformError::shape(
                "thinking_config",
                "budget and level conflict",
            ));
        }
        let display = thinking.include_thoughts.map(|v| {
            if v {
                cc::ThinkingDisplay::Summarized
            } else {
                cc::ThinkingDisplay::Omitted
            }
        });
        out.thinking = match thinking.thinking_budget {
            Some(0) => Some(cc::ThinkingConfig::Disabled(
                cc::ThinkingDisabled::builder().build(),
            )),
            Some(-1) => Some(cc::ThinkingConfig::Adaptive(cc::ThinkingAdaptive {
                display,
                rest: Default::default(),
            })),
            Some(n) if n > 0 => Some(cc::ThinkingConfig::Enabled(cc::ThinkingEnabled {
                budget_tokens: n,
                display,
                rest: Default::default(),
            })),
            Some(_) => {
                return Err(TransformError::shape(
                    "thinking_budget",
                    "invalid negative budget",
                ));
            }
            None => match thinking.thinking_level {
                Some(g::ThinkingLevel::Low | g::ThinkingLevel::Medium | g::ThinkingLevel::High) => {
                    let effort = match thinking.thinking_level {
                        Some(g::ThinkingLevel::Low) => cc::Effort::Low,
                        Some(g::ThinkingLevel::Medium) => cc::Effort::Medium,
                        Some(g::ThinkingLevel::High) => cc::Effort::High,
                        Some(g::ThinkingLevel::Minimal | g::ThinkingLevel::Unspecified) | None => {
                            unreachable!()
                        }
                    };
                    out.output_config = Some(cc::OutputConfig::builder().effort(effort).build());
                    Some(cc::ThinkingConfig::Adaptive(cc::ThinkingAdaptive {
                        display,
                        rest: Default::default(),
                    }))
                }
                Some(g::ThinkingLevel::Minimal) => {
                    return Err(TransformError::unsupported(
                        "thinking_level",
                        "Claude has no minimal effort level",
                    ));
                }
                Some(g::ThinkingLevel::Unspecified) | None => {
                    if display.is_some() {
                        return Err(TransformError::missing_metadata(
                            "effective thinking configuration",
                        ));
                    }
                    None
                }
            },
        };
    }
    let typed = config
        .response_schema
        .as_ref()
        .map(|s| crate::transform::generate::gemini_schema::to_json(s, Default::default()))
        .transpose()?;
    if let Some(v) = &typed {
        report.diagnostics.extend(v.report.diagnostics.clone());
    }
    let raw = config
        .response_json_schema
        .as_ref()
        .or(config.response_json_schema_internal.as_ref());
    if config
        .response_json_schema
        .as_ref()
        .zip(config.response_json_schema_internal.as_ref())
        .is_some_and(|(a, b)| a != b)
        || typed
            .as_ref()
            .zip(raw)
            .is_some_and(|(a, b)| b.as_object() != Some(&a.value))
    {
        return Err(TransformError::shape(
            "response_schema",
            "conflicting schema representations",
        ));
    }
    let schema = raw
        .cloned()
        .or_else(|| typed.map(|v| serde_json::Value::Object(v.value)));
    match config.response_mime_type.as_deref() {
        Some("application/json") if schema.is_none() => {
            return Err(TransformError::unsupported(
                "response_mime_type",
                "free JSON has no verified Claude strict-output equivalent",
            ));
        }
        Some("application/json" | "text/plain") | None => {}
        Some(_) => {
            return Err(TransformError::unsupported(
                "response_mime_type",
                "Claude cannot produce requested MIME",
            ));
        }
    }
    if let Some(schema) = schema {
        if config.response_mime_type.as_deref() == Some("text/plain") {
            return Err(TransformError::shape(
                "response_schema",
                "schema conflicts with text/plain output",
            ));
        }
        out.output_format = Some(
            cc::JsonOutputFormat::builder(cc::JsonOutputFormatType::JsonSchema, schema).build(),
        );
    }
    Ok(())
}
