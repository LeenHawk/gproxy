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
            return Err(TransformError::unsupported(
                field,
                "Gemini context/policy requires a target adapter",
            ));
        }
    }
    out.store = input.store.map(Some);
    if let Some(config) = &input.generation_config {
        if config.candidate_count.is_some_and(|n| n <= 0)
            || config.max_output_tokens.is_some_and(|n| n <= 0)
        {
            return Err(TransformError::shape(
                "generation_config",
                "candidate count and token budget must be positive",
            ));
        }
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
                return Err(TransformError::unsupported(
                    field,
                    "Chat lacks an equivalent generation control",
                ));
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
                return Err(TransformError::unsupported(
                    "thinking.include_thoughts",
                    "Chat has no thought output channel",
                ));
            }
            out.reasoning_effort = match (thinking.thinking_budget, thinking.thinking_level.clone())
            {
                (Some(0), None) => Some(Some(c::ReasoningEffort::None)),
                (Some(_), _) => {
                    return Err(TransformError::unsupported(
                        "thinking_budget",
                        "Chat effort cannot preserve exact thinking budget",
                    ));
                }
                (None, Some(level)) => Some(Some(match level {
                    g::ThinkingLevel::Minimal => c::ReasoningEffort::Minimal,
                    g::ThinkingLevel::Low => c::ReasoningEffort::Low,
                    g::ThinkingLevel::Medium => c::ReasoningEffort::Medium,
                    g::ThinkingLevel::High => c::ReasoningEffort::High,
                    g::ThinkingLevel::Unspecified => {
                        return Err(TransformError::shape(
                            "thinking_level",
                            "unspecified thinking level",
                        ));
                    }
                })),
                (None, None) => None,
            };
        }
        let typed = config
            .response_schema
            .as_ref()
            .map(|schema| super::super::gemini_schema::to_json(schema, Default::default()))
            .transpose()?;
        if let Some(typed) = &typed {
            report.diagnostics.extend(typed.report.diagnostics.clone());
        }
        let raw = config
            .response_json_schema
            .as_ref()
            .or(config.response_json_schema_internal.as_ref());
        if let (Some(a), Some(b)) = (
            &config.response_json_schema,
            &config.response_json_schema_internal,
        ) && a != b
        {
            return Err(TransformError::shape(
                "response_schema",
                "conflicting raw schemas",
            ));
        }
        let raw = raw
            .map(|value| {
                value.as_object().cloned().ok_or_else(|| {
                    TransformError::unsupported(
                        "response_json_schema",
                        "Chat schema must be object JSON",
                    )
                })
            })
            .transpose()?;
        if let (Some(a), Some(b)) = (&typed, &raw)
            && &a.value != b
        {
            return Err(TransformError::shape(
                "response_schema",
                "typed/raw schemas conflict",
            ));
        }
        let schema = typed.map(|v| v.value).or(raw);
        out.response_format = if let Some(schema) = schema {
            if config
                .response_mime_type
                .as_deref()
                .is_some_and(|m| m != "application/json")
            {
                return Err(TransformError::shape(
                    "response_mime_type",
                    "schema conflicts with MIME",
                ));
            }
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
                    return Err(TransformError::unsupported(
                        "response_mime_type",
                        "Chat supports text or JSON output",
                    ));
                }
            }
        };
    }
    if let Some(config) = &input.tool_config {
        if config.retrieval_config.is_some()
            || config.include_server_side_tool_invocations == Some(true)
        {
            return Err(TransformError::unsupported(
                "tool_config",
                "server retrieval needs adapter",
            ));
        }
        if let Some(function) = &config.function_calling_config {
            let mode = function
                .mode
                .clone()
                .unwrap_or(g::FunctionCallingMode::Auto);
            out.tool_choice = Some(if let Some(names) = &function.allowed_function_names {
                let selectors = names
                    .iter()
                    .map(|name| {
                        serde_json::json!({"type":"function","function":{"name":name}})
                            .as_object()
                            .unwrap()
                            .clone()
                    })
                    .collect();
                match mode {
                    g::FunctionCallingMode::Any
                    | g::FunctionCallingMode::Auto
                    | g::FunctionCallingMode::Validated => c::ToolChoice::Allowed(
                        c::AllowedToolChoice::builder(
                            c::AllowedToolChoiceType::AllowedTools,
                            c::AllowedTools::builder(
                                if mode == g::FunctionCallingMode::Any {
                                    c::AllowedToolsMode::Required
                                } else {
                                    c::AllowedToolsMode::Auto
                                },
                                selectors,
                            )
                            .build(),
                        )
                        .build(),
                    ),
                    g::FunctionCallingMode::None | g::FunctionCallingMode::Unspecified => {
                        return Err(TransformError::unsupported(
                            "function_calling_config",
                            "allowed names incompatible with mode",
                        ));
                    }
                }
            } else {
                c::ToolChoice::Mode(match mode {
                    g::FunctionCallingMode::Auto | g::FunctionCallingMode::Validated => {
                        c::ToolChoiceMode::Auto
                    }
                    g::FunctionCallingMode::Any => c::ToolChoiceMode::Required,
                    g::FunctionCallingMode::None => c::ToolChoiceMode::None,
                    g::FunctionCallingMode::Unspecified => {
                        return Err(TransformError::unsupported(
                            "function_calling_config.mode",
                            "Chat has no validated mode",
                        ));
                    }
                })
            });
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
            return Err(TransformError::unsupported(
                field,
                "Gemini needs explicit capability for this Chat behavior",
            ));
        }
    }
    if input.n.flatten().is_some_and(|n| n <= 0)
        || input
            .max_completion_tokens
            .flatten()
            .is_some_and(|n| n <= 0)
        || input.max_tokens.flatten().is_some_and(|n| n <= 0)
    {
        return Err(TransformError::shape(
            "generation_config",
            "candidate count and token budget must be positive",
        ));
    }
    if let (Some(a), Some(b)) = (
        input.max_completion_tokens.flatten(),
        input.max_tokens.flatten(),
    ) && a != b
    {
        return Err(TransformError::shape(
            "max_tokens",
            "conflicting token budgets",
        ));
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
                return Err(TransformError::unsupported(
                    "reasoning_effort",
                    "Gemini has no equivalent effort level",
                ));
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
                let schema = format.json_schema.schema.as_ref().ok_or_else(|| {
                    TransformError::shape("response_format.schema", "missing schema")
                })?;
                config.response_mime_type = Some("application/json".into());
                config.response_json_schema = Some(serde_json::Value::Object(schema.clone()));
                report.changed(
                    "response_format",
                    "Gemini raw JSON schema endpoint must support the selected schema keywords",
                );
            }
        }
    }
    if input.tool_choice.is_some() && input.function_call.is_some() {
        return Err(TransformError::shape(
            "tool_choice",
            "legacy/current choices conflict",
        ));
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
                        .map(|v| {
                            v.get("function")
                                .and_then(|v| v.get("name"))
                                .and_then(|v| v.as_str())
                                .map(str::to_owned)
                                .ok_or_else(|| {
                                    TransformError::unsupported(
                                        "tool_choice.allowed",
                                        "Gemini accepts named function selectors only",
                                    )
                                })
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                );
            }
            c::ToolChoice::Custom(_) => {
                return Err(TransformError::unsupported(
                    "tool_choice.custom",
                    "Gemini lacks custom tools",
                ));
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
