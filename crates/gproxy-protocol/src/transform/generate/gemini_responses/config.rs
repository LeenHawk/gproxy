use crate::{
    transform::{Report, TransformError},
    wire::{
        gemini as g,
        openai::responses::{generate as r, input as i},
    },
};
pub(super) fn to_responses(
    input: &g::GenerateContentRequestBody,
    out: &mut r::GenerateContentRequestBody,
    report: &mut Report,
) -> Result<(), TransformError> {
    for (present, field) in [
        (input.cached_content.is_some(), "cached_content"),
        (input.safety_settings.is_some(), "safety_settings"),
        (input.service_tier.is_some(), "service_tier"),
    ] {
        if present {
            return Err(TransformError::unsupported(
                field,
                "Gemini context/policy requires Responses host adaptation",
            ));
        }
    }
    out.store = input.store.map(Some);
    let Some(config) = &input.generation_config else {
        return Ok(());
    };
    for (present, field) in [
        (
            config.candidate_count.is_some_and(|n| n != 1),
            "candidate_count",
        ),
        (config.stop_sequences.is_some(), "stop_sequences"),
        (config.seed.is_some(), "seed"),
        (config.top_k.is_some(), "top_k"),
        (
            config.presence_penalty.is_some_and(|n| n != 0.0),
            "presence_penalty",
        ),
        (
            config.frequency_penalty.is_some_and(|n| n != 0.0),
            "frequency_penalty",
        ),
        (config.speech_config.is_some(), "speech_config"),
        (config.image_config.is_some(), "image_config"),
        (config.media_resolution.is_some(), "media_resolution"),
        (config.response_format.is_some(), "response_format"),
        (
            config.enable_enhanced_civic_answers.is_some(),
            "civic_answers",
        ),
        (
            config
                .response_modalities
                .as_ref()
                .is_some_and(|v| v.iter().any(|v| !matches!(v, g::Modality::Text))),
            "response_modalities",
        ),
    ] {
        if present {
            return Err(TransformError::unsupported(
                field,
                "Responses lacks this direct control; fanout/media/capability adapter required",
            ));
        }
    }
    out.max_output_tokens = config.max_output_tokens.map(Some);
    out.temperature = number(config.temperature)?;
    out.top_p = number(config.top_p)?;
    out.top_logprobs = config.logprobs.map(Some);
    if config.response_logprobs == Some(true) {
        out.include = Some(Some(vec![r::ResponseIncludable::OutputLogprobs]));
    }
    if let Some(thinking) = &config.thinking_config {
        let mut reasoning = i::ReasoningConfig::builder().build();
        reasoning.effort = match (thinking.thinking_budget, thinking.thinking_level.as_ref()) {
            (Some(0), None) => Some(Some(i::ReasoningEffort::None)),
            (Some(_), _) => {
                return Err(TransformError::unsupported(
                    "thinking_budget",
                    "Responses cannot preserve exact budget",
                ));
            }
            (None, Some(level)) => Some(Some(match level {
                g::ThinkingLevel::Minimal => i::ReasoningEffort::Minimal,
                g::ThinkingLevel::Low => i::ReasoningEffort::Low,
                g::ThinkingLevel::Medium => i::ReasoningEffort::Medium,
                g::ThinkingLevel::High => i::ReasoningEffort::High,
                g::ThinkingLevel::Unspecified => {
                    return Err(TransformError::shape("thinking_level", "unspecified"));
                }
            })),
            (None, None) => None,
        };
        if thinking.include_thoughts == Some(true) {
            return Err(TransformError::unsupported(
                "include_thoughts",
                "Responses summary is not equivalent full thought output",
            ));
        }
        out.reasoning = Some(Some(reasoning));
    }
    let typed = config
        .response_schema
        .as_ref()
        .map(|s| super::super::gemini_schema::to_json(s, Default::default()))
        .transpose()?;
    if let Some(v) = &typed {
        report.diagnostics.extend(v.report.diagnostics.clone());
    }
    if let (Some(a), Some(b)) = (
        &config.response_json_schema,
        &config.response_json_schema_internal,
    ) && a != b
    {
        return Err(TransformError::shape("schema", "conflicting raw schemas"));
    }
    let raw = config
        .response_json_schema
        .as_ref()
        .or(config.response_json_schema_internal.as_ref())
        .map(|v| {
            v.as_object().cloned().ok_or_else(|| {
                TransformError::unsupported("schema", "Responses requires object schema")
            })
        })
        .transpose()?;
    if let (Some(a), Some(b)) = (&typed, &raw)
        && &a.value != b
    {
        return Err(TransformError::shape("schema", "typed/raw schema conflict"));
    }
    let schema = typed.map(|v| v.value).or(raw);
    let format = if let Some(schema) = schema {
        if config
            .response_mime_type
            .as_deref()
            .is_some_and(|v| v != "application/json")
        {
            return Err(TransformError::shape(
                "response_mime_type",
                "schema conflicts MIME",
            ));
        }
        Some(i::TextFormat::JsonSchema(
            i::TextFormatJsonSchema::builder("gemini_response".into(), schema)
                .strict(Some(true))
                .build(),
        ))
    } else {
        match config.response_mime_type.as_deref() {
            None | Some("text/plain") => None,
            Some("application/json") => Some(i::TextFormat::JsonObject(
                i::TextFormatJsonObject::builder().build(),
            )),
            Some(_) => {
                return Err(TransformError::unsupported(
                    "response_mime_type",
                    "no target representation",
                ));
            }
        }
    };
    if let Some(format) = format {
        out.text = Some(i::TextConfig::builder().format(format).build());
    }
    Ok(())
}
pub(super) fn to_gemini(
    input: &r::GenerateContentRequestBody,
    out: &mut g::GenerateContentRequestBody,
    report: &mut Report,
) -> Result<(), TransformError> {
    for (present, field) in [
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
                "{field} resolved context"
            )));
        }
    }
    for (present, field) in [
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
        (input.service_tier.flatten().is_some(), "service_tier"),
        (
            input.parallel_tool_calls.flatten() == Some(false),
            "parallel_tool_calls",
        ),
        (
            input.truncation.flatten() == Some(r::GenerateTruncation::Auto),
            "truncation",
        ),
    ] {
        if present {
            return Err(TransformError::unsupported(
                field,
                "Gemini needs a host adapter for this policy",
            ));
        }
    }
    let mut config = g::GenerationConfig::builder().build();
    config.max_output_tokens = input.max_output_tokens.flatten();
    config.temperature = float(&input.temperature)?;
    config.top_p = float(&input.top_p)?;
    config.logprobs = input.top_logprobs.flatten();
    if input
        .include
        .as_ref()
        .and_then(Option::as_ref)
        .is_some_and(|v| v.contains(&r::ResponseIncludable::OutputLogprobs))
    {
        config.response_logprobs = Some(true);
    }
    if let Some(reasoning) = input.reasoning.as_ref().and_then(Option::as_ref) {
        if reasoning.mode.is_some()
            || reasoning.summary.flatten().is_some()
            || reasoning.generate_summary.flatten().is_some()
            || reasoning
                .context
                .as_ref()
                .and_then(Option::as_ref)
                .is_some()
        {
            return Err(TransformError::unsupported(
                "reasoning",
                "Gemini lacks matching summary/context policy",
            ));
        }
        if let Some(effort) = reasoning.effort.flatten() {
            let mut thinking = g::ThinkingConfig::builder().build();
            match effort {
                i::ReasoningEffort::None => thinking.thinking_budget = Some(0),
                i::ReasoningEffort::Minimal => {
                    thinking.thinking_level = Some(g::ThinkingLevel::Minimal)
                }
                i::ReasoningEffort::Low => thinking.thinking_level = Some(g::ThinkingLevel::Low),
                i::ReasoningEffort::Medium => {
                    thinking.thinking_level = Some(g::ThinkingLevel::Medium)
                }
                i::ReasoningEffort::High => thinking.thinking_level = Some(g::ThinkingLevel::High),
                i::ReasoningEffort::Xhigh | i::ReasoningEffort::Max => {
                    return Err(TransformError::unsupported(
                        "reasoning.effort",
                        "Gemini has no matching effort",
                    ));
                }
            }
            config.thinking_config = Some(thinking);
        }
    }
    if let Some(text) = &input.text {
        if text.verbosity.flatten().is_some() {
            return Err(TransformError::unsupported(
                "verbosity",
                "Gemini lacks verbosity setting",
            ));
        }
        if let Some(format) = &text.format {
            match format {
                i::TextFormat::Text(_) => config.response_mime_type = Some("text/plain".into()),
                i::TextFormat::JsonObject(_) => {
                    config.response_mime_type = Some("application/json".into())
                }
                i::TextFormat::JsonSchema(schema) => {
                    config.response_mime_type = Some("application/json".into());
                    config.response_json_schema =
                        Some(serde_json::Value::Object(schema.schema.clone()));
                    report.changed("text.format","raw Gemini schema supported-keyword contract must be checked for selected model");
                }
            }
        }
    }
    out.generation_config = Some(config);
    out.store = input.store.flatten();
    for (present, field) in [
        (input.metadata.is_some(), "metadata"),
        (input.prompt_cache_key.is_some(), "prompt_cache_key"),
        (input.prompt_cache_options.is_some(), "prompt_cache_options"),
        (
            input.prompt_cache_retention.is_some(),
            "prompt_cache_retention",
        ),
        (input.user.is_some(), "user"),
        (input.safety_identifier.is_some(), "safety_identifier"),
        (input.stream_options.is_some(), "stream_options"),
        (input.stream.is_some(), "stream"),
        (input.include.is_some(), "include"),
    ] {
        if present {
            report.omitted(
                field,
                "host retains source metadata/delivery options unavailable in Gemini body",
            );
        }
    }
    Ok(())
}
fn number(v: Option<f64>) -> Result<Option<Option<serde_json::Number>>, TransformError> {
    v.map(|v| {
        serde_json::Number::from_f64(v)
            .ok_or_else(|| TransformError::shape("sampling", "nonfinite"))
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
