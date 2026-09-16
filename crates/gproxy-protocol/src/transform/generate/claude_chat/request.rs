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
    let target_model = target_model.into();
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
    crate::transform::instructions::chat(&mut messages, &target_model, &mut report);
    let tool_choice = claude_choice_to_openai(input.tool_choice.as_ref())?;
    let parallel_tool_calls = input.tool_choice.as_ref().and_then(|choice| match choice {
        cc::ToolChoice::Auto(choice) => choice.disable_parallel_tool_use.map(|value| !value),
        cc::ToolChoice::Any(choice) => choice.disable_parallel_tool_use.map(|value| !value),
        cc::ToolChoice::Tool(choice) => choice.disable_parallel_tool_use.map(|value| !value),
        cc::ToolChoice::None(_) => None,
    });

    let format = input
        .output_config
        .as_ref()
        .and_then(|config| config.format.as_ref())
        .or(input.output_format.as_ref());
    let response_format = format.and_then(|format| {
        let schema = format.schema.as_object().cloned();
        if schema.is_none() {
            report.omitted("output_format", "target requires an object schema");
        }
        schema.map(|schema| {
            chat::ResponseFormat::JsonSchema(chat::JsonSchemaResponseFormat {
                type_: chat::JsonSchemaResponseType::JsonSchema,
                json_schema: chat::JsonSchemaFormat {
                    name: "claude_output".into(),
                    description: None,
                    schema: Some(schema),
                    strict: Some(Some(true)),
                    rest: Rest::new(),
                },
                rest: Rest::new(),
            })
        })
    });
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
            reasoning_effort = Some(Some(chat::ReasoningEffort::None));
        }
        Some(cc::ThinkingConfig::Enabled(_)) => {
            report.omitted("thinking.budget_tokens", "Chat has no token budget control");
        }
        Some(cc::ThinkingConfig::Adaptive(config)) if config.display.is_some() => {
            report.omitted("thinking.display", "field has no target representation");
        }
        Some(cc::ThinkingConfig::Adaptive(_)) | None => {}
    }
    Ok(Converted {
        value: chat::GenerateContentRequestBody {
            messages,
            model: target_model,
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
    let target_model = target_model.into();
    let mut report = Report::default();
    if input.n.flatten().is_some_and(|value| value != 1) {
        report.omitted("n", "field has no target representation");
    }
    super::requirements::chat(input, &mut report)?;
    let mut messages = Vec::new();
    let mut system = Vec::new();
    let mut position = crate::transform::instructions::Position::default();
    for message in &input.messages {
        let leading = position.leading(matches!(
            message,
            chat::ChatMessage::System(_) | chat::ChatMessage::Developer(_)
        ));
        match message {
            chat::ChatMessage::System(message) if leading => {
                system.push(super::content::chat_text(&message.content))
            }
            chat::ChatMessage::Developer(message) if leading => {
                system.push(super::content::chat_text(&message.content))
            }
            chat::ChatMessage::System(_)
            | chat::ChatMessage::Developer(_)
            | chat::ChatMessage::User(_)
            | chat::ChatMessage::Assistant(_)
            | chat::ChatMessage::Tool(_)
            | chat::ChatMessage::Function(_) => {
                messages.extend(openai_message_to_claude(message, &mut report)?)
            }
        }
    }
    crate::transform::instructions::claude(&mut messages, &target_model, &mut report);
    let max_tokens = input
        .max_completion_tokens
        .flatten()
        .or(input.max_tokens.flatten())
        .ok_or_else(|| {
            TransformError::shape("max_tokens", "Claude Messages requires a token budget")
        })?;

    let stop_sequences = match input.stop.clone().flatten() {
        None => None,
        Some(chat::Stop::String(value)) => Some(vec![value]),
        Some(chat::Stop::Sequences(values)) => Some(values),
    };

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
            format
                .json_schema
                .schema
                .clone()
                .map(|schema| cc::JsonOutputFormat {
                    type_: cc::JsonOutputFormatType::JsonSchema,
                    schema: serde_json::Value::Object(schema),
                    rest: Rest::new(),
                })
        }
        Some(chat::ResponseFormat::Text(_)) => None,
        Some(chat::ResponseFormat::JsonObject(_)) => {
            report.omitted("response_format", "Claude has no matching format");
            None
        }
    };
    let output_effort = match input.reasoning_effort.flatten() {
        Some(chat::ReasoningEffort::Low) => Some(cc::Effort::Low),
        Some(chat::ReasoningEffort::Medium) => Some(cc::Effort::Medium),
        Some(chat::ReasoningEffort::High) => Some(cc::Effort::High),
        Some(chat::ReasoningEffort::XHigh) => Some(cc::Effort::Xhigh),
        Some(chat::ReasoningEffort::Max) => Some(cc::Effort::Max),
        _ => None,
    };
    let thinking = if input.reasoning_effort.flatten() == Some(chat::ReasoningEffort::None) {
        Some(cc::ThinkingConfig::Disabled(
            cc::ThinkingDisabled::builder().build(),
        ))
    } else {
        output_effort.map(|_| cc::ThinkingConfig::Adaptive(cc::ThinkingAdaptive::builder().build()))
    };
    Ok(Converted {
        value: cg::GenerateContentRequestBody {
            max_tokens,
            messages,
            model: target_model,
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
                    report.omitted("service_tier", "Claude has no matching tier");
                    None
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
