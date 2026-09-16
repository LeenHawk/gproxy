use crate::{
    transform::{Report, TransformError},
    wire::{gemini as g, openai::chat as c},
};
pub(super) fn to_chat(
    input: &g::GenerateContentRequestBody,
    out: &mut c::GenerateContentRequestBody,
    report: &mut Report,
) -> Result<(), TransformError> {
    for (present, field) in [
        (input.cached_content.is_some(), "cached_content"),
        (input.safety_settings.is_some(), "safety_settings"),
        (input.service_tier.is_some(), "service_tier"),
    ] {
        if present {
            report.omitted(field, "field has no target representation");
        }
    }
    out.store = input.store.map(Some);
    if let Some(config) = &input.generation_config {
        for (present, field) in [
            (config.top_k.is_some(), "top_k"),
            (config.speech_config.is_some(), "speech_config"),
            (config.image_config.is_some(), "image_config"),
            (config.media_resolution.is_some(), "media_resolution"),
            (config.response_format.is_some(), "response_format"),
            (
                config.enable_enhanced_civic_answers.is_some(),
                "enable_enhanced_civic_answers",
            ),
            (
                config
                    .response_modalities
                    .as_ref()
                    .is_some_and(|m| m.iter().any(|m| !matches!(m, g::Modality::Text))),
                "response_modalities",
            ),
        ] {
            if present {
                report.omitted(field, "field has no target representation");
            }
        }
        out.max_completion_tokens = config.max_output_tokens.map(Some);
        out.n = config.candidate_count.map(Some);
        out.temperature = config.temperature.map(Some);
        out.top_p = config.top_p.map(Some);
        out.seed = config.seed.map(Some);
        out.presence_penalty = config.presence_penalty.map(Some);
        out.frequency_penalty = config.frequency_penalty.map(Some);
        out.logprobs = config.response_logprobs.map(Some);
        out.top_logprobs = config.logprobs.map(Some);
        out.stop = config
            .stop_sequences
            .clone()
            .map(c::Stop::Sequences)
            .map(Some);
        if let Some(thinking) = &config.thinking_config {
            if thinking.include_thoughts == Some(true) {
                report.omitted(
                    "thinking.include_thoughts",
                    "field has no target representation",
                );
            }
            out.reasoning_effort = match thinking.thinking_level {
                Some(g::ThinkingLevel::Minimal) => Some(Some(c::ReasoningEffort::Minimal)),
                Some(g::ThinkingLevel::Low) => Some(Some(c::ReasoningEffort::Low)),
                Some(g::ThinkingLevel::Medium) => Some(Some(c::ReasoningEffort::Medium)),
                Some(g::ThinkingLevel::High) => Some(Some(c::ReasoningEffort::High)),
                _ if thinking.thinking_budget == Some(0) => Some(Some(c::ReasoningEffort::None)),
                _ => None,
            };
            if thinking.thinking_budget.is_some_and(|n| n != 0) {
                report.omitted("thinking_budget", "Chat has no token budget control");
            }
        }
        let typed = config
            .response_schema
            .as_ref()
            .map(|schema| super::super::gemini_schema::to_json(schema, Default::default()))
            .map(crate::transform::optional)
            .transpose()?
            .flatten();
        if let Some(typed) = &typed {
            report.diagnostics.extend(typed.report.diagnostics.clone());
        }
        let raw = config
            .response_json_schema
            .as_ref()
            .or(config.response_json_schema_internal.as_ref());

        let raw = raw.and_then(|value| value.as_object().cloned());
        let schema = typed.map(|v| v.value).or(raw);
        out.response_format = if let Some(schema) = schema {
            let mut value = c::JsonSchemaFormat::builder("gemini_response".into()).build();
            value.schema = Some(schema);
            value.strict = Some(Some(true));
            Some(c::ResponseFormat::JsonSchema(
                c::JsonSchemaResponseFormat::builder(c::JsonSchemaResponseType::JsonSchema, value)
                    .build(),
            ))
        } else {
            match config.response_mime_type.as_deref() {
                None | Some("text/plain") => None,
                Some("application/json") => Some(c::ResponseFormat::JsonObject(
                    c::JsonObjectResponseFormat::builder(c::JsonObjectResponseType::JsonObject)
                        .build(),
                )),
                Some(_) => {
                    report.omitted("response_mime_type", "Chat has no matching MIME control");
                    None
                }
            }
        };
    }
    if let Some(config) = &input.tool_config {
        if config.retrieval_config.is_some()
            || config.include_server_side_tool_invocations == Some(true)
        {
            report.omitted("tool_config", "field has no target representation");
        }
        if let Some(function) = &config.function_calling_config {
            let mode = function
                .mode
                .clone()
                .unwrap_or(g::FunctionCallingMode::Auto);
            out.tool_choice = match mode {
                g::FunctionCallingMode::None => Some(c::ToolChoice::Mode(c::ToolChoiceMode::None)),
                g::FunctionCallingMode::Unspecified => None,
                mode => Some(if let Some(names) = &function.allowed_function_names {
                    c::ToolChoice::Allowed(c::AllowedToolChoice::builder(
                        c::AllowedToolChoiceType::AllowedTools,
                        c::AllowedTools::builder(
                            if mode == g::FunctionCallingMode::Any { c::AllowedToolsMode::Required }
                            else { c::AllowedToolsMode::Auto },
                            names.iter().map(|name| serde_json::json!({"type":"function","function":{"name":name}}).as_object().unwrap().clone()).collect(),
                        ).build(),
                    ).build())
                } else {
                    c::ToolChoice::Mode(if mode == g::FunctionCallingMode::Any {
                        c::ToolChoiceMode::Required
                    } else {
                        c::ToolChoiceMode::Auto
                    })
                }),
            };
        }
    }
    Ok(())
}
pub(super) fn to_gemini(
    input: &c::GenerateContentRequestBody,
    out: &mut g::GenerateContentRequestBody,
    report: &mut Report,
) -> Result<(), TransformError> {
    for (present, field) in [
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
            input
                .logit_bias
                .as_ref()
                .and_then(Option::as_ref)
                .is_some_and(|m| !m.is_empty()),
            "logit_bias",
        ),
        (
            input.prediction.as_ref().and_then(Option::as_ref).is_some(),
            "prediction",
        ),
        (
            input.moderation.as_ref().and_then(Option::as_ref).is_some(),
            "moderation",
        ),
        (input.service_tier.flatten().is_some(), "service_tier"),
        (input.verbosity.flatten().is_some(), "verbosity"),
        (input.web_search_options.is_some(), "web_search_options"),
        (
            input.parallel_tool_calls == Some(false),
            "parallel_tool_calls",
        ),
    ] {
        if present {
            report.omitted(field, "field has no target representation");
        }
    }

    let mut config = g::GenerationConfig::builder().build();
    config.max_output_tokens = input
        .max_completion_tokens
        .flatten()
        .or(input.max_tokens.flatten());
    config.candidate_count = input.n.flatten();
    config.temperature = input.temperature.flatten();
    config.top_p = input.top_p.flatten();
    config.seed = input.seed.flatten();
    config.presence_penalty = input.presence_penalty.flatten();
    config.frequency_penalty = input.frequency_penalty.flatten();
    config.response_logprobs = input.logprobs.flatten();
    config.logprobs = input.top_logprobs.flatten();
    config.stop_sequences = input
        .stop
        .as_ref()
        .and_then(Option::as_ref)
        .map(|v| match v {
            c::Stop::String(v) => vec![v.clone()],
            c::Stop::Sequences(v) => v.clone(),
        });
    if let Some(effort) = input.reasoning_effort.flatten() {
        let mut thinking = g::ThinkingConfig::builder().build();
        match effort {
            c::ReasoningEffort::None => thinking.thinking_budget = Some(0),
            c::ReasoningEffort::Minimal => {
                thinking.thinking_level = Some(g::ThinkingLevel::Minimal)
            }
            c::ReasoningEffort::Low => thinking.thinking_level = Some(g::ThinkingLevel::Low),
            c::ReasoningEffort::Medium => thinking.thinking_level = Some(g::ThinkingLevel::Medium),
            c::ReasoningEffort::High => thinking.thinking_level = Some(g::ThinkingLevel::High),
            c::ReasoningEffort::XHigh | c::ReasoningEffort::Max => {
                report.omitted("reasoning_effort", "Gemini has no matching effort");
            }
        }
        config.thinking_config = Some(thinking);
    }
    if let Some(format) = &input.response_format {
        match format {
            c::ResponseFormat::Text(_) => config.response_mime_type = Some("text/plain".into()),
            c::ResponseFormat::JsonObject(_) => {
                config.response_mime_type = Some("application/json".into())
            }
            c::ResponseFormat::JsonSchema(format) => {
                if let Some(schema) = format.json_schema.schema.as_ref() {
                    config.response_mime_type = Some("application/json".into());
                    config.response_json_schema = Some(serde_json::Value::Object(schema.clone()));
                    report.changed("response_format", "schema mapped to Gemini raw JSON schema");
                }
            }
        }
    }

    if let Some(choice) = &input.function_call {
        let mut function = g::FunctionCallingConfig::builder().build();
        match choice {
            c::FunctionCallChoice::Mode(c::FunctionCallMode::Auto) => {
                function.mode = Some(g::FunctionCallingMode::Auto)
            }
            c::FunctionCallChoice::Mode(c::FunctionCallMode::None) => {
                function.mode = Some(g::FunctionCallingMode::None)
            }
            c::FunctionCallChoice::Name(choice) => {
                function.mode = Some(g::FunctionCallingMode::Any);
                function.allowed_function_names = Some(vec![choice.name.clone()]);
            }
        }
        out.tool_config = Some(
            g::ToolConfig::builder()
                .function_calling_config(function)
                .build(),
        );
    }
    if let Some(choice) = &input.tool_choice {
        let mut function = g::FunctionCallingConfig::builder().build();
        match choice {
            c::ToolChoice::Mode(mode) => {
                function.mode = Some(match mode {
                    c::ToolChoiceMode::Auto => g::FunctionCallingMode::Auto,
                    c::ToolChoiceMode::Required => g::FunctionCallingMode::Any,
                    c::ToolChoiceMode::None => g::FunctionCallingMode::None,
                })
            }
            c::ToolChoice::Function(choice) => {
                function.mode = Some(g::FunctionCallingMode::Any);
                function.allowed_function_names = Some(vec![choice.function.name.clone()]);
            }
            c::ToolChoice::Allowed(choice) => {
                function.mode = Some(match choice.allowed_tools.mode {
                    c::AllowedToolsMode::Auto => g::FunctionCallingMode::Auto,
                    c::AllowedToolsMode::Required => g::FunctionCallingMode::Any,
                });
                function.allowed_function_names = Some(
                    choice
                        .allowed_tools
                        .tools
                        .iter()
                        .filter_map(|v| {
                            v.get("function")
                                .and_then(|v| v.get("name"))
                                .and_then(|v| v.as_str())
                                .map(str::to_owned)
                        })
                        .collect(),
                );
            }
            c::ToolChoice::Custom(_) => {
                report.omitted("tool_choice.custom", "Gemini has no custom tool selector");
            }
        }
        out.tool_config = Some(
            g::ToolConfig::builder()
                .function_calling_config(function)
                .build(),
        );
    }
    for (present, field) in [
        (input.metadata.is_some(), "metadata"),
        (input.user.is_some(), "user"),
        (input.prompt_cache_key.is_some(), "prompt_cache_key"),
        (input.prompt_cache_options.is_some(), "prompt_cache_options"),
        (
            input.prompt_cache_retention.is_some(),
            "prompt_cache_retention",
        ),
        (input.safety_identifier.is_some(), "safety_identifier"),
        (input.stream.is_some(), "stream"),
        (input.stream_options.is_some(), "stream_options"),
    ] {
        if present {
            report.omitted(
                field,
                "Gemini host must retain request metadata/delivery preference; no body equivalent",
            );
        }
    }
    out.store = input.store.flatten();
    out.generation_config = Some(config);
    Ok(())
}
