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
        report.omitted(
            "context management must be applied before counting target input",
            "field has no target representation",
        );
    }
    if input.speed.is_some() {
        report.omitted("speed", "field has no target representation");
    }
    if input
        .output_config
        .as_ref()
        .is_some_and(|v| v.task_budget.is_some())
    {
        report.omitted("task_budget", "field has no target representation");
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
        Some(c::ThinkingConfig::BetweenTools) => {
            effort = Some(o::ReasoningEffort::None);
            report.changed(
                "thinking.type",
                "between_tools mapped to no reasoning; target has no between-tool progress mode",
            );
        }
        Some(c::ThinkingConfig::Disabled(_)) => {
            effort = Some(o::ReasoningEffort::None);
        }
        Some(c::ThinkingConfig::Enabled(_)) => {
            report.omitted("thinking.budget", "target has no thinking budget");
        }
        Some(c::ThinkingConfig::Adaptive(v)) => {
            if effort.is_none() {
                report.omitted(
                    "adaptive target effort",
                    "field has no target representation",
                );
            }
            if v.display.is_some() {
                report.omitted("thinking.display", "field has no target representation");
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

    if let Some(format) = config.or(input.output_format.as_ref())
        && let Some(schema) = format.schema.as_object()
    {
        out.text = Some(Some(
            o::TextConfig::builder()
                .format(o::TextFormat::JsonSchema(
                    o::TextFormatJsonSchema::builder("count_output".into(), schema.clone())
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
    if let Some(config) = input.reasoning.as_ref().and_then(Option::as_ref)
        && let Some(effort) = config.effort.flatten()
    {
        match effort {
            o::ReasoningEffort::None => {
                out.thinking = Some(c::ThinkingConfig::Disabled(
                    c::ThinkingDisabled::builder().build(),
                ))
            }
            o::ReasoningEffort::Minimal | o::ReasoningEffort::Numeric(_) => {
                report.omitted("effort", "target has no matching effort");
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
                    // The enclosing arm includes only named Claude effort levels.
                    o::ReasoningEffort::None
                    | o::ReasoningEffort::Minimal
                    | o::ReasoningEffort::Numeric(_) => unreachable!(),
                };
                out.thinking = Some(c::ThinkingConfig::Adaptive(
                    c::ThinkingAdaptive::builder().build(),
                ));
                out.output_config = Some(c::OutputConfig::builder().effort(effort).build());
            }
        }
    }
    if let Some(text) = input.text.as_ref().and_then(Option::as_ref) {
        if text.verbosity.flatten().is_some() {
            report.omitted("verbosity", "field has no target representation");
        }
        if let Some(format) = &text.format {
            match format {
                o::TextFormat::Text(_) => {}
                o::TextFormat::JsonObject(_) => {
                    report.omitted("text.format", "target has no matching format");
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
    let Some(config) = &input.generation_config else {
        return Ok(());
    };
    if let Some(thinking) = &config.thinking_config {
        if thinking.include_thoughts == Some(true) {
            report.omitted("include_thoughts", "field has no target representation");
        }
        let effort = match thinking.thinking_level {
            Some(g::ThinkingLevel::Minimal) => Some(o::ReasoningEffort::Minimal),
            Some(g::ThinkingLevel::Low) => Some(o::ReasoningEffort::Low),
            Some(g::ThinkingLevel::Medium) => Some(o::ReasoningEffort::Medium),
            Some(g::ThinkingLevel::High) => Some(o::ReasoningEffort::High),
            _ if thinking.thinking_budget == Some(0) => Some(o::ReasoningEffort::None),
            _ => None,
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
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    if let Some(v) = &typed {
        report.diagnostics.extend(v.report.diagnostics.clone());
    }

    let raw = config
        .response_json_schema
        .as_ref()
        .or(config.response_json_schema_internal.as_ref())
        .and_then(|v| v.as_object().cloned());
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
            Some(_) => None,
        }
    };
    if format.is_some()
        && config
            .response_mime_type
            .as_deref()
            .is_some_and(|v| !matches!(v, "application/json" | "text/plain"))
    {
        report.omitted("response_mime_type", "field has no target representation");
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
        report.omitted("parallel_tool_calls", "field has no target representation");
    }
    let mut config = g::GenerationConfig::builder().build();
    let mut present = false;
    if let Some(reasoning) = input.reasoning.as_ref().and_then(Option::as_ref)
        && let Some(effort) = reasoning.effort.flatten()
    {
        let mut thinking = g::ThinkingConfig::builder().build();
        match effort {
            o::ReasoningEffort::None => thinking.thinking_budget = Some(0),
            o::ReasoningEffort::Minimal => {
                thinking.thinking_level = Some(g::ThinkingLevel::Minimal)
            }
            o::ReasoningEffort::Low => thinking.thinking_level = Some(g::ThinkingLevel::Low),
            o::ReasoningEffort::Medium => thinking.thinking_level = Some(g::ThinkingLevel::Medium),
            o::ReasoningEffort::High => thinking.thinking_level = Some(g::ThinkingLevel::High),
            o::ReasoningEffort::Xhigh
            | o::ReasoningEffort::Max
            | o::ReasoningEffort::Numeric(_) => {
                report.omitted("reasoning.effort", "target has no matching effort");
            }
        }
        if thinking.thinking_budget.is_some() || thinking.thinking_level.is_some() {
            config.thinking_config = Some(thinking);
            present = true;
        }
    }
    if let Some(text) = input.text.as_ref().and_then(Option::as_ref) {
        if text.verbosity.flatten().is_some() {
            report.omitted("verbosity", "field has no target representation");
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
