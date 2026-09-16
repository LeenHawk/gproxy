use crate::{
    transform::{Report, TransformError},
    wire::openai::{
        chat as c,
        responses::{generate as r, input as i},
    },
};
fn number(
    value: Option<Option<f64>>,
) -> Result<Option<Option<serde_json::Number>>, TransformError> {
    Ok(match value {
        Some(Some(v)) => serde_json::Number::from_f64(v).map(Some),
        Some(None) => Some(None),
        None => None,
    })
}
fn float(
    value: &Option<Option<serde_json::Number>>,
) -> Result<Option<Option<f64>>, TransformError> {
    Ok(match value {
        Some(Some(v)) => v.as_f64().map(Some),
        Some(None) => Some(None),
        None => None,
    })
}
pub(super) fn to_responses(
    input: &c::GenerateContentRequestBody,
    out: &mut r::GenerateContentRequestBody,
    report: &mut Report,
) -> Result<(), TransformError> {
    for (present, field) in [
        (input.n.flatten().is_some_and(|n| n != 1), "n"),
        (
            input.stop.as_ref().and_then(Option::as_ref).is_some(),
            "stop",
        ),
        (
            input.audio.as_ref().and_then(Option::as_ref).is_some(),
            "audio",
        ),
        (
            input
                .modalities
                .as_ref()
                .and_then(Option::as_ref)
                .is_some_and(|m| m.iter().any(|m| !matches!(m, c::Modality::Text))),
            "modalities",
        ),
        (
            input.frequency_penalty.flatten().is_some_and(|v| v != 0.0),
            "frequency_penalty",
        ),
        (
            input.presence_penalty.flatten().is_some_and(|v| v != 0.0),
            "presence_penalty",
        ),
        (
            input
                .logit_bias
                .as_ref()
                .and_then(Option::as_ref)
                .is_some_and(|v| !v.is_empty()),
            "logit_bias",
        ),
        (
            input.prediction.as_ref().and_then(Option::as_ref).is_some(),
            "prediction",
        ),
        (input.seed.flatten().is_some(), "seed"),
        (input.web_search_options.is_some(), "web_search_options"),
    ] {
        if present {
            report.omitted(field, "field has no target representation");
        }
    }
    out.max_output_tokens = match (input.max_completion_tokens, input.max_tokens) {
        (Some(Some(a)), _) => Some(Some(a)),
        (_, Some(Some(b))) => Some(Some(b)),
        (a, b) => a.or(b),
    };
    out.parallel_tool_calls = input.parallel_tool_calls.map(Some);
    out.temperature = number(input.temperature)?;
    out.top_p = number(input.top_p)?;
    out.top_logprobs = input.top_logprobs;
    if input.logprobs.flatten() == Some(true) {
        out.include = Some(Some(vec![r::ResponseIncludable::OutputLogprobs]));
    }
    out.service_tier = input.service_tier.map(|v| {
        v.map(|v| match v {
            c::ServiceTier::Auto => r::ServiceTier::Auto,
            c::ServiceTier::Default => r::ServiceTier::Default,
            c::ServiceTier::Flex => r::ServiceTier::Flex,
            c::ServiceTier::Scale => r::ServiceTier::Scale,
            c::ServiceTier::Priority => r::ServiceTier::Priority,
            c::ServiceTier::Fast => r::ServiceTier::Fast,
        })
    });
    out.reasoning = input.reasoning_effort.map(|effort| {
        let mut config = i::ReasoningConfig::builder().build();
        config.effort = Some(effort.map(|v| match v {
            c::ReasoningEffort::None => i::ReasoningEffort::None,
            c::ReasoningEffort::Minimal => i::ReasoningEffort::Minimal,
            c::ReasoningEffort::Low => i::ReasoningEffort::Low,
            c::ReasoningEffort::Medium => i::ReasoningEffort::Medium,
            c::ReasoningEffort::High => i::ReasoningEffort::High,
            c::ReasoningEffort::XHigh => i::ReasoningEffort::Xhigh,
            c::ReasoningEffort::Max => i::ReasoningEffort::Max,
        }));
        Some(config)
    });
    if input.response_format.is_some() || input.verbosity.is_some() {
        let mut text = i::TextConfig::builder().build();
        text.verbosity = input.verbosity.map(|v| {
            v.map(|v| match v {
                c::Verbosity::Low => i::TextVerbosity::Low,
                c::Verbosity::Medium => i::TextVerbosity::Medium,
                c::Verbosity::High => i::TextVerbosity::High,
            })
        });
        text.format = input
            .response_format
            .as_ref()
            .map(|format| {
                Ok::<_, TransformError>(match format {
                    c::ResponseFormat::Text(_) => {
                        i::TextFormat::Text(i::TextFormatText::builder().build())
                    }
                    c::ResponseFormat::JsonObject(_) => {
                        i::TextFormat::JsonObject(i::TextFormatJsonObject::builder().build())
                    }
                    c::ResponseFormat::JsonSchema(format) => {
                        let mut value = i::TextFormatJsonSchema::builder(
                            format.json_schema.name.clone(),
                            format.json_schema.schema.clone().ok_or_else(|| {
                                TransformError::shape("response_format.schema", "missing schema")
                            })?,
                        )
                        .build();
                        value.description = format.json_schema.description.clone();
                        value.strict = format.json_schema.strict;
                        i::TextFormat::JsonSchema(value)
                    }
                })
            })
            .map(crate::transform::optional)
            .transpose()?
            .flatten();
        out.text = Some(text);
    }
    if let Some(options) = input.stream_options.as_ref() {
        out.stream_options = Some(options.as_ref().map(|options| {
            let mut value = r::StreamOptions::builder().build();
            value.include_obfuscation = options.include_obfuscation;
            value
        }));
        if options.as_ref().is_some_and(|v| v.include_usage.is_some()) {
            report.omitted("stream_options.include_usage","Responses streams carry usage through terminal response; host must retain Chat delivery preference");
        }
    }
    super::shared_controls::to_responses(input, out)?;
    Ok(())
}
pub(super) fn to_chat(
    input: &r::GenerateContentRequestBody,
    out: &mut c::GenerateContentRequestBody,
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
            report.omitted(
                format!("{field} resolved history"),
                "field has no target representation",
            );
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
            input.truncation.flatten() == Some(r::GenerateTruncation::Auto),
            "truncation",
        ),
    ] {
        if present {
            report.omitted(field, "field has no target representation");
        }
    }
    out.max_completion_tokens = input.max_output_tokens;
    out.parallel_tool_calls = input.parallel_tool_calls.flatten();
    out.temperature = float(&input.temperature)?;
    out.top_p = float(&input.top_p)?;
    out.top_logprobs = input.top_logprobs;
    out.service_tier = input.service_tier.map(|v| {
        v.map(|v| match v {
            r::ServiceTier::Auto => c::ServiceTier::Auto,
            r::ServiceTier::Default => c::ServiceTier::Default,
            r::ServiceTier::Flex => c::ServiceTier::Flex,
            r::ServiceTier::Scale => c::ServiceTier::Scale,
            r::ServiceTier::Priority => c::ServiceTier::Priority,
            r::ServiceTier::Fast => c::ServiceTier::Fast,
        })
    });
    if let Some(config) = input.reasoning.as_ref().and_then(Option::as_ref) {
        if config.mode.is_some()
            || config.summary.flatten().is_some()
            || config.generate_summary.flatten().is_some()
            || config.context.as_ref().and_then(Option::as_ref).is_some()
        {
            report.omitted("reasoning", "field has no target representation");
        }
        out.reasoning_effort = config.effort.map(|v| {
            v.map(|v| match v {
                i::ReasoningEffort::None => c::ReasoningEffort::None,
                i::ReasoningEffort::Minimal => c::ReasoningEffort::Minimal,
                i::ReasoningEffort::Low => c::ReasoningEffort::Low,
                i::ReasoningEffort::Medium => c::ReasoningEffort::Medium,
                i::ReasoningEffort::High => c::ReasoningEffort::High,
                i::ReasoningEffort::Xhigh => c::ReasoningEffort::XHigh,
                i::ReasoningEffort::Max => c::ReasoningEffort::Max,
            })
        });
    }
    if let Some(text) = &input.text {
        out.verbosity = text.verbosity.map(|v| {
            v.map(|v| match v {
                i::TextVerbosity::Low => c::Verbosity::Low,
                i::TextVerbosity::Medium => c::Verbosity::Medium,
                i::TextVerbosity::High => c::Verbosity::High,
            })
        });
        out.response_format = text.format.as_ref().map(|format| match format {
            i::TextFormat::Text(_) => c::ResponseFormat::Text(
                c::TextResponseFormat::builder(c::TextResponseType::Text).build(),
            ),
            i::TextFormat::JsonObject(_) => c::ResponseFormat::JsonObject(
                c::JsonObjectResponseFormat::builder(c::JsonObjectResponseType::JsonObject).build(),
            ),
            i::TextFormat::JsonSchema(schema) => {
                let mut value = c::JsonSchemaFormat::builder(schema.name.clone()).build();
                value.schema = Some(schema.schema.clone());
                value.strict = schema.strict;
                value.description = schema.description.clone();
                c::ResponseFormat::JsonSchema(
                    c::JsonSchemaResponseFormat::builder(
                        c::JsonSchemaResponseType::JsonSchema,
                        value,
                    )
                    .build(),
                )
            }
        });
    }
    if let Some(options) = &input.stream_options {
        out.stream_options = Some(options.as_ref().map(|options| {
            let mut value = c::StreamOptions::builder().build();
            value.include_obfuscation = options.include_obfuscation;
            value
        }));
    }
    if let Some(Some(include)) = &input.include {
        for field in include {
            if matches!(field, r::ResponseIncludable::OutputLogprobs) {
                out.logprobs = Some(Some(true));
            } else {
                report.omitted(
                    "include",
                    "Responses supplementary item fields have no Chat equivalent",
                );
            }
        }
    }
    super::shared_controls::to_chat(input, out)?;
    Ok(())
}
