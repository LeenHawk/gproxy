use crate::{
    transform::{Report, TransformError},
    wire::{claude::count_tokens as c, gemini as g, openai::count_tokens as o},
};
pub(super) fn claude_to_openai(
    input: &c::CountTokensRequestBody,
    out: &mut o::CountTokensRequestBody,
    report: &mut Report,
) -> Result<(), TransformError> {
    if input
        .context_management
        .as_ref()
        .and_then(|v| v.edits.as_ref())
        .is_some_and(|v| !v.is_empty())
    {
        return Err(TransformError::missing_metadata(
            "context management must be applied before counting target input",
        ));
    }
    if input.speed.is_some() {
        return Err(TransformError::unsupported(
            "speed",
            "count target has no equivalent speed contract",
        ));
    }
    if input
        .output_config
        .as_ref()
        .is_some_and(|v| v.task_budget.is_some())
    {
        return Err(TransformError::unsupported(
            "task_budget",
            "no equivalent target count task budget",
        ));
    }
    let mut effort = input
        .output_config
        .as_ref()
        .and_then(|v| v.effort)
        .map(|v| match v {
            c::Effort::Low => o::ReasoningEffort::Low,
            c::Effort::Medium => o::ReasoningEffort::Medium,
            c::Effort::High => o::ReasoningEffort::High,
            c::Effort::Xhigh => o::ReasoningEffort::Xhigh,
            c::Effort::Max => o::ReasoningEffort::Max,
        });
    match &input.thinking {
        Some(c::ThinkingConfig::Disabled(_)) => {
            if effort.is_some() {
                return Err(TransformError::shape(
                    "thinking",
                    "disabled thinking conflicts with effort",
                ));
            }
            effort = Some(o::ReasoningEffort::None);
        }
        Some(c::ThinkingConfig::Enabled(_)) => {
            return Err(TransformError::unsupported(
                "thinking.budget",
                "count target cannot preserve exact thinking budget",
            ));
        }
        Some(c::ThinkingConfig::Adaptive(v)) => {
            if effort.is_none() {
                return Err(TransformError::missing_metadata("adaptive target effort"));
            }
            if v.display.is_some() {
                return Err(TransformError::unsupported(
                    "thinking.display",
                    "no equivalent count thinking display",
                ));
            }
        }
        None => {}
    }
    if let Some(effort) = effort {
        out.reasoning = Some(Some(
            o::ReasoningConfig::builder().effort(Some(effort)).build(),
        ));
    }
    let config = input.output_config.as_ref().and_then(|v| v.format.as_ref());
    if let (Some(a), Some(b)) = (config, input.output_format.as_ref())
        && a.schema != b.schema
    {
        return Err(TransformError::shape(
            "output_format",
            "conflicting schemas",
        ));
    }
    if let Some(format) = config.or(input.output_format.as_ref()) {
        let schema = format.schema.as_object().cloned().ok_or_else(|| {
            TransformError::unsupported("schema", "OpenAI count requires object schema")
        })?;
        out.text = Some(Some(
            o::TextConfig::builder()
                .format(o::TextFormat::JsonSchema(
                    o::TextFormatJsonSchema::builder("count_output".into(), schema)
                        .strict(Some(true))
                        .build(),
                ))
                .build(),
        ));
    }
    if input.cache_control.is_some() {
        report.omitted("cache_control", "OpenAI count has no Claude cache marker");
    }
    Ok(())
}
pub(super) fn openai_to_claude(
    input: &o::CountTokensRequestBody,
    out: &mut c::CountTokensRequestBody,
    report: &mut Report,
) -> Result<(), TransformError> {
    if let Some(config) = input.reasoning.as_ref().and_then(Option::as_ref) {
        reasoning_fields(config)?;
        if let Some(effort) = config.effort.flatten() {
            match effort {
                o::ReasoningEffort::None => {
                    out.thinking = Some(c::ThinkingConfig::Disabled(
                        c::ThinkingDisabled::builder().build(),
                    ))
                }
                o::ReasoningEffort::Minimal => {
                    return Err(TransformError::unsupported(
                        "effort",
                        "Claude lacks minimal effort",
                    ));
                }
                o::ReasoningEffort::Low
                | o::ReasoningEffort::Medium
                | o::ReasoningEffort::High
                | o::ReasoningEffort::Xhigh
                | o::ReasoningEffort::Max => {
                    let effort = match effort {
                        o::ReasoningEffort::Low => c::Effort::Low,
                        o::ReasoningEffort::Medium => c::Effort::Medium,
                        o::ReasoningEffort::High => c::Effort::High,
                        o::ReasoningEffort::Xhigh => c::Effort::Xhigh,
                        o::ReasoningEffort::Max => c::Effort::Max,
                        o::ReasoningEffort::None | o::ReasoningEffort::Minimal => unreachable!(),
                    };
                    out.thinking = Some(c::ThinkingConfig::Adaptive(
                        c::ThinkingAdaptive::builder().build(),
                    ));
                    out.output_config = Some(c::OutputConfig::builder().effort(effort).build());
                }
            }
        }
    }
    if let Some(text) = input.text.as_ref().and_then(Option::as_ref) {
        if text.verbosity.flatten().is_some() {
            return Err(TransformError::unsupported(
                "verbosity",
                "Claude count has no verbosity field",
            ));
        }
        if let Some(format) = &text.format {
            match format {
                o::TextFormat::Text(_) => {}
                o::TextFormat::JsonObject(_) => {
                    return Err(TransformError::unsupported(
                        "json_object",
                        "no verified Claude arbitrary JSON schema equivalent",
                    ));
                }
                o::TextFormat::JsonSchema(schema) => {
                    out.output_format = Some(
                        c::JsonOutputFormat::builder(
                            c::JsonOutputFormatType::JsonSchema,
                            serde_json::Value::Object(schema.schema.clone()),
                        )
                        .build(),
                    );
                    report.changed(
                        "schema.wrapper",
                        "Claude counts schema without name/strict wrapper",
                    );
                }
            }
        }
    }
    Ok(())
}
pub(super) fn gemini_to_openai(
    input: &g::EmbeddedGenerateContentRequest,
    out: &mut o::CountTokensRequestBody,
    report: &mut Report,
) -> Result<(), TransformError> {
    gemini_policy(input)?;
    let Some(config) = &input.generation_config else {
        return Ok(());
    };
    if let Some(thinking) = &config.thinking_config {
        if thinking.include_thoughts == Some(true) {
            return Err(TransformError::unsupported(
                "include_thoughts",
                "no full-thought output count control",
            ));
        }
        let effort = match (thinking.thinking_budget, thinking.thinking_level.as_ref()) {
            (Some(0), None) => Some(o::ReasoningEffort::None),
            (Some(_), _) => {
                return Err(TransformError::unsupported(
                    "thinking_budget",
                    "target has no exact budget",
                ));
            }
            (None, Some(level)) => Some(match level {
                g::ThinkingLevel::Minimal => o::ReasoningEffort::Minimal,
                g::ThinkingLevel::Low => o::ReasoningEffort::Low,
                g::ThinkingLevel::Medium => o::ReasoningEffort::Medium,
                g::ThinkingLevel::High => o::ReasoningEffort::High,
                g::ThinkingLevel::Unspecified => {
                    return Err(TransformError::shape("thinking_level", "unspecified"));
                }
            }),
            (None, None) => None,
        };
        if let Some(effort) = effort {
            out.reasoning = Some(Some(
                o::ReasoningConfig::builder().effort(Some(effort)).build(),
            ));
        }
    }
    let typed = config
        .response_schema
        .as_ref()
        .map(|s| crate::transform::generate::gemini_schema::to_json(s, Default::default()))
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
            v.as_object()
                .cloned()
                .ok_or_else(|| TransformError::unsupported("schema", "object schema required"))
        })
        .transpose()?;
    if let (Some(a), Some(b)) = (&typed, &raw)
        && &a.value != b
    {
        return Err(TransformError::shape("schema", "typed/raw conflict"));
    }
    let schema = typed.map(|v| v.value).or(raw);
    let format = if let Some(schema) = schema {
        Some(o::TextFormat::JsonSchema(
            o::TextFormatJsonSchema::builder("count_output".into(), schema)
                .strict(Some(true))
                .build(),
        ))
    } else {
        match config.response_mime_type.as_deref() {
            None | Some("text/plain") => None,
            Some("application/json") => Some(o::TextFormat::JsonObject(
                o::TextFormatJsonObject::builder().build(),
            )),
            Some(_) => {
                return Err(TransformError::unsupported(
                    "response_mime",
                    "no count equivalent",
                ));
            }
        }
    };
    if format.is_some()
        && config
            .response_mime_type
            .as_deref()
            .is_some_and(|v| !matches!(v, "application/json" | "text/plain"))
    {
        return Err(TransformError::unsupported(
            "response_mime_type",
            "schema MIME has no count equivalent",
        ));
    }
    if let Some(format) = format {
        out.text = Some(Some(o::TextConfig::builder().format(format).build()));
    }
    report.omitted(
        "generation_config.sampling",
        "output sampling does not affect input token count",
    );
    Ok(())
}
pub(super) fn openai_to_gemini(
    input: &o::CountTokensRequestBody,
    report: &mut Report,
) -> Result<Option<g::GenerationConfig>, TransformError> {
    if input.parallel_tool_calls.flatten() == Some(false) {
        return Err(TransformError::unsupported(
            "parallel_tool_calls",
            "Gemini lacks disable-parallel policy",
        ));
    }
    let mut config = g::GenerationConfig::builder().build();
    let mut present = false;
    if let Some(reasoning) = input.reasoning.as_ref().and_then(Option::as_ref) {
        reasoning_fields(reasoning)?;
        if let Some(effort) = reasoning.effort.flatten() {
            let mut thinking = g::ThinkingConfig::builder().build();
            match effort {
                o::ReasoningEffort::None => thinking.thinking_budget = Some(0),
                o::ReasoningEffort::Minimal => {
                    thinking.thinking_level = Some(g::ThinkingLevel::Minimal)
                }
                o::ReasoningEffort::Low => thinking.thinking_level = Some(g::ThinkingLevel::Low),
                o::ReasoningEffort::Medium => {
                    thinking.thinking_level = Some(g::ThinkingLevel::Medium)
                }
                o::ReasoningEffort::High => thinking.thinking_level = Some(g::ThinkingLevel::High),
                o::ReasoningEffort::Xhigh | o::ReasoningEffort::Max => {
                    return Err(TransformError::unsupported(
                        "reasoning.effort",
                        "Gemini lacks level",
                    ));
                }
            }
            config.thinking_config = Some(thinking);
            present = true;
        }
    }
    if let Some(text) = input.text.as_ref().and_then(Option::as_ref) {
        if text.verbosity.flatten().is_some() {
            return Err(TransformError::unsupported(
                "verbosity",
                "Gemini count has no verbosity",
            ));
        }
        if let Some(format) = &text.format {
            present = true;
            match format {
                o::TextFormat::Text(_) => config.response_mime_type = Some("text/plain".into()),
                o::TextFormat::JsonObject(_) => {
                    config.response_mime_type = Some("application/json".into())
                }
                o::TextFormat::JsonSchema(schema) => {
                    config.response_mime_type = Some("application/json".into());
                    config.response_json_schema =
                        Some(serde_json::Value::Object(schema.schema.clone()));
                    report.changed(
                        "schema",
                        "selected Gemini count endpoint must support raw schema keywords",
                    );
                }
            }
        }
    }
    Ok(present.then_some(config))
}
fn reasoning_fields(input: &o::ReasoningConfig) -> Result<(), TransformError> {
    if input.summary.flatten().is_some()
        || input.generate_summary.flatten().is_some()
        || input.mode.is_some()
        || input.context.as_ref().and_then(Option::as_ref).is_some()
    {
        return Err(TransformError::unsupported(
            "reasoning",
            "summary/context policy requires host adaptation",
        ));
    }
    Ok(())
}
pub(super) fn gemini_policy(
    input: &g::EmbeddedGenerateContentRequest,
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
            "safety/store",
            "target-specific host policy required",
        ));
    }
    Ok(())
}
