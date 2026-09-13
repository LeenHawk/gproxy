use super::{
    content::{claude_message_to_openai, openai_message_to_claude},
    tools::{
        claude_choice_to_openai, claude_tools_to_openai, openai_choice_to_claude,
        openai_tools_to_claude,
    },
};
use crate::wire::claude::count_tokens as cc;
use crate::{
    Rest,
    transform::{Converted, Report, TransformError},
    wire::{claude::generate_content as cg, openai::chat},
};

pub fn claude_to_openai(
    input: &cg::GenerateContentRequestBody,
    target_model: impl Into<String>,
) -> Result<Converted<chat::GenerateContentRequestBody>, TransformError> {
    let mut report = Report::default();
    super::requirements::claude(input, &mut report)?;
    let mut messages = Vec::new();
    if let Some(system) = &input.system {
        let text = match system {
            cc::SystemPrompt::Text(text) => text.clone(),
            cc::SystemPrompt::Blocks(blocks) => blocks
                .iter()
                .map(|block| block.text.clone())
                .collect::<Vec<_>>()
                .join("\n"),
        };
        messages.push(chat::ChatMessage::System(chat::SystemMessage {
            role: chat::SystemRole::System,
            content: chat::TextContent::Text(text),
            name: None,
            rest: Rest::new(),
        }));
    }
    for message in &input.messages {
        messages.extend(claude_message_to_openai(message, &mut report)?);
    }
    let tool_choice = claude_choice_to_openai(input.tool_choice.as_ref())?;
    let parallel_tool_calls = input.tool_choice.as_ref().and_then(|choice| match choice {
        cc::ToolChoice::Auto(choice) => choice.disable_parallel_tool_use.map(|value| !value),
        cc::ToolChoice::Any(choice) => choice.disable_parallel_tool_use.map(|value| !value),
        cc::ToolChoice::Tool(choice) => choice.disable_parallel_tool_use.map(|value| !value),
        cc::ToolChoice::None(_) => None,
    });
    if let (Some(legacy), Some(current)) = (
        input.output_format.as_ref(),
        input
            .output_config
            .as_ref()
            .and_then(|config| config.format.as_ref()),
    ) && legacy.schema != current.schema
    {
        return Err(TransformError::shape(
            "output_format",
            "legacy and current output schemas conflict",
        ));
    }
    let format = input
        .output_config
        .as_ref()
        .and_then(|config| config.format.as_ref())
        .or(input.output_format.as_ref());
    let response_format = if let Some(format) = format {
        let schema = format.schema.as_object().cloned().ok_or_else(|| {
            TransformError::unsupported("output_format", "JSON schema must be an object")
        })?;
        Some(chat::ResponseFormat::JsonSchema(
            chat::JsonSchemaResponseFormat {
                type_: chat::JsonSchemaResponseType::JsonSchema,
                json_schema: chat::JsonSchemaFormat {
                    name: "claude_output".into(),
                    description: None,
                    schema: Some(schema),
                    strict: Some(Some(true)),
                    rest: Rest::new(),
                },
                rest: Rest::new(),
            },
        ))
    } else {
        None
    };
    let mut reasoning_effort = input
        .output_config
        .as_ref()
        .and_then(|config| config.effort.as_ref())
        .map(|effort| match effort {
            cc::Effort::Low => chat::ReasoningEffort::Low,
            cc::Effort::Medium => chat::ReasoningEffort::Medium,
            cc::Effort::High => chat::ReasoningEffort::High,
            cc::Effort::Xhigh => chat::ReasoningEffort::XHigh,
            cc::Effort::Max => chat::ReasoningEffort::Max,
        })
        .map(Some);
    match input.thinking.as_ref() {
        Some(cc::ThinkingConfig::Disabled(_)) => {
            if reasoning_effort.is_some() {
                return Err(TransformError::shape(
                    "thinking",
                    "disabled thinking conflicts with requested effort",
                ));
            }
            reasoning_effort = Some(Some(chat::ReasoningEffort::None));
        }
        Some(cc::ThinkingConfig::Enabled(_)) => {
            return Err(TransformError::unsupported(
                "thinking.budget_tokens",
                "Chat effort cannot preserve an exact Claude thinking token budget",
            ));
        }
        Some(cc::ThinkingConfig::Adaptive(config)) => {
            if config.display.is_some() {
                return Err(TransformError::unsupported(
                    "thinking.display",
                    "Chat has no equivalent thinking-display control",
                ));
            }
            if reasoning_effort.is_none() {
                return Err(TransformError::missing_metadata(
                    "adaptive thinking requires explicit target effort",
                ));
            }
        }
        None => {}
    }
    Ok(Converted {
        value: chat::GenerateContentRequestBody {
            messages,
            model: target_model.into(),
            audio: None,
            frequency_penalty: None,
            function_call: None,
            functions: None,
            logit_bias: None,
            logprobs: None,
            max_completion_tokens: Some(Some(input.max_tokens)),
            max_tokens: None,
            metadata: None,
            modalities: None,
            moderation: None,
            n: None,
            parallel_tool_calls,
            prediction: None,
            presence_penalty: None,
            prompt_cache_key: None,
            prompt_cache_options: None,
            prompt_cache_retention: None,
            reasoning_effort,
            response_format,
            safety_identifier: None,
            seed: None,
            service_tier: input.service_tier.map(|tier| {
                Some(match tier {
                    cg::ServiceTier::Auto => chat::ServiceTier::Auto,
                    cg::ServiceTier::StandardOnly => chat::ServiceTier::Default,
                })
            }),
            stop: input
                .stop_sequences
                .clone()
                .map(chat::Stop::Sequences)
                .map(Some),
            store: None,
            stream: input.stream.map(Some),
            stream_options: None,
            temperature: input.temperature.map(Some),
            tool_choice,
            tools: claude_tools_to_openai(input.tools.as_ref(), &mut report)?,
            top_logprobs: None,
            top_p: input.top_p.map(Some),
            user: input
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.user_id.clone()),
            verbosity: None,
            web_search_options: None,
            rest: Rest::new(),
        },
        report,
    })
}

pub fn openai_to_claude(
    input: &chat::GenerateContentRequestBody,
    target_model: impl Into<String>,
) -> Result<Converted<cg::GenerateContentRequestBody>, TransformError> {
    let mut report = Report::default();
    if input.n.flatten().is_some_and(|value| value != 1) {
        return Err(TransformError::unsupported(
            "n",
            "pure pair cannot fan out multiple candidates; caller must provide an adaptation stage",
        ));
    }
    super::requirements::chat(input, &mut report)?;
    let mut messages = Vec::new();
    let mut system = Vec::new();
    for message in &input.messages {
        match message {
            chat::ChatMessage::System(message) => {
                system.push(super::content::chat_text(&message.content))
            }
            chat::ChatMessage::Developer(message) => {
                system.push(super::content::chat_text(&message.content))
            }
            chat::ChatMessage::User(_)
            | chat::ChatMessage::Assistant(_)
            | chat::ChatMessage::Tool(_)
            | chat::ChatMessage::Function(_) => {
                messages.extend(openai_message_to_claude(message, &mut report)?)
            }
        }
    }
    let max_tokens = input
        .max_completion_tokens
        .flatten()
        .or(input.max_tokens.flatten())
        .ok_or_else(|| {
            TransformError::shape("max_tokens", "Claude Messages requires a token budget")
        })?;
    if max_tokens <= 0 {
        return Err(TransformError::shape(
            "max_tokens",
            "positive token budget required",
        ));
    }
    if let (Some(a), Some(b)) = (
        input.max_completion_tokens.flatten(),
        input.max_tokens.flatten(),
    ) && a != b
    {
        return Err(TransformError::shape(
            "max_tokens",
            "conflicting maximum token budgets",
        ));
    }
    let stop_sequences = match input.stop.clone().flatten() {
        None => None,
        Some(chat::Stop::String(value)) => Some(vec![value]),
        Some(chat::Stop::Sequences(values)) => Some(values),
    };
    if input.tool_choice.is_some() && input.function_call.is_some() {
        return Err(TransformError::shape(
            "tool_choice",
            "legacy and modern tool choices conflict",
        ));
    }
    if input.tools.is_some() && input.functions.is_some() {
        return Err(TransformError::shape(
            "tools",
            "legacy and modern tool declarations conflict",
        ));
    }
    let mut tool_choice = if let Some(choice) = input.function_call.as_ref() {
        Some(super::tools::legacy_choice(choice))
    } else {
        openai_choice_to_claude(input.tool_choice.as_ref())?
    };
    if let Some(parallel) = input.parallel_tool_calls {
        tool_choice = Some(apply_parallel(tool_choice, parallel));
    }
    let output_format = match input.response_format.as_ref() {
        None => None,
        Some(chat::ResponseFormat::JsonSchema(format)) => {
            if format.json_schema.strict.flatten() != Some(true) {
                report.changed("response_format.json_schema.strict","Claude enforces the supplied schema; Chat strict preference is not independently configurable");
            }
            if format.json_schema.description.is_some() {
                report.omitted(
                    "response_format.json_schema.description",
                    "Claude format has no schema-wrapper description field",
                );
            }
            report.omitted(
                "response_format.json_schema.name",
                "Claude format has no schema-wrapper name field",
            );
            let schema = format.json_schema.schema.clone().ok_or_else(|| {
                TransformError::unsupported("response_format", "JSON schema object is required")
            })?;
            Some(cc::JsonOutputFormat {
                type_: cc::JsonOutputFormatType::JsonSchema,
                schema: serde_json::Value::Object(schema),
                rest: Rest::new(),
            })
        }
        Some(chat::ResponseFormat::Text(_)) => None,
        Some(chat::ResponseFormat::JsonObject(_)) => {
            return Err(TransformError::unsupported(
                "response_format",
                "unconstrained JSON object mode has no verified equivalent in Claude structured outputs",
            ));
        }
    };
    let (output_effort, thinking) = match input.reasoning_effort.flatten() {
        None => (None, None),
        Some(chat::ReasoningEffort::None) => (
            None,
            Some(cc::ThinkingConfig::Disabled(
                cc::ThinkingDisabled::builder().build(),
            )),
        ),
        Some(chat::ReasoningEffort::Minimal) => {
            return Err(TransformError::unsupported(
                "reasoning_effort",
                "minimal has no Claude effort equivalent",
            ));
        }
        Some(effort) => {
            let effort = match effort {
                chat::ReasoningEffort::Low => cc::Effort::Low,
                chat::ReasoningEffort::Medium => cc::Effort::Medium,
                chat::ReasoningEffort::High => cc::Effort::High,
                chat::ReasoningEffort::XHigh => cc::Effort::Xhigh,
                chat::ReasoningEffort::Max => cc::Effort::Max,
                chat::ReasoningEffort::None | chat::ReasoningEffort::Minimal => {
                    return Err(TransformError::unsupported(
                        "reasoning_effort",
                        "unknown effort",
                    ));
                }
            };
            (
                Some(effort),
                Some(cc::ThinkingConfig::Adaptive(
                    cc::ThinkingAdaptive::builder().build(),
                )),
            )
        }
    };
    Ok(Converted {
        value: cg::GenerateContentRequestBody {
            max_tokens,
            messages,
            model: target_model.into(),
            cache_control: None,
            container: None,
            context_management: None,
            diagnostics: None,
            fallback_credit_token: None,
            fallbacks: None,
            inference_geo: None,
            mcp_servers: None,
            metadata: input.user.as_ref().map(|user| {
                let mut metadata = cg::Metadata::builder().build();
                metadata.user_id = Some(user.clone());
                metadata
            }),
            output_config: output_effort.map(|effort| cc::OutputConfig {
                effort: Some(effort),
                format: None,
                task_budget: None,
                rest: Rest::new(),
            }),
            output_format,
            service_tier: match input.service_tier.flatten() {
                None => None,
                Some(chat::ServiceTier::Auto) => Some(cg::ServiceTier::Auto),
                Some(chat::ServiceTier::Default) => Some(cg::ServiceTier::StandardOnly),
                Some(
                    chat::ServiceTier::Flex
                    | chat::ServiceTier::Scale
                    | chat::ServiceTier::Priority
                    | chat::ServiceTier::Fast,
                ) => {
                    return Err(TransformError::unsupported(
                        "service_tier",
                        "requested tier has no Claude equivalent",
                    ));
                }
            },
            speed: None,
            stop_sequences,
            stream: input.stream.flatten(),
            system: (!system.is_empty()).then(|| cc::SystemPrompt::Text(system.join("\n"))),
            temperature: input.temperature.flatten(),
            thinking,
            tool_choice,
            tools: if let Some(functions) = input.functions.as_ref() {
                Some(super::tools::legacy_tools(functions)?)
            } else {
                openai_tools_to_claude(input.tools.as_ref())?
            },
            top_k: None,
            top_p: input.top_p.flatten(),
            rest: Rest::new(),
        },
        report,
    })
}

fn apply_parallel(choice: Option<cc::ToolChoice>, parallel: bool) -> cc::ToolChoice {
    let disable = Some(!parallel);
    match choice {
        Some(cc::ToolChoice::Auto(mut choice)) => {
            choice.disable_parallel_tool_use = disable;
            cc::ToolChoice::Auto(choice)
        }
        Some(cc::ToolChoice::Any(mut choice)) => {
            choice.disable_parallel_tool_use = disable;
            cc::ToolChoice::Any(choice)
        }
        Some(cc::ToolChoice::Tool(mut choice)) => {
            choice.disable_parallel_tool_use = disable;
            cc::ToolChoice::Tool(choice)
        }
        Some(choice) => choice,
        None => cc::ToolChoice::Auto(cc::ToolChoiceAuto {
            disable_parallel_tool_use: disable,
            rest: Rest::new(),
        }),
    }
}
