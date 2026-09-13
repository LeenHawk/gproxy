use crate::{
    transform::{
        Converted, Report, TransformError,
        generate::claude_gemini as pair,
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::{
        DeclaredFields,
        claude::{content as cc, count_tokens as c},
        gemini as g,
    },
};
pub fn claude_to_gemini(
    input: c::CountTokensRequestBody,
    target_model: impl Into<String>,
    context: pair::ClaudeGeminiRequestContext,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<g::CountTokensRequestBody>, TransformError> {
    super::policy(policy, crate::Dialect::Gemini)?;
    let model = super::model(target_model)?;
    let input = input.into_declared();
    if input
        .context_management
        .as_ref()
        .and_then(|v| v.edits.as_ref())
        .is_some_and(|v| !v.is_empty())
    {
        return Err(TransformError::missing_metadata(
            "apply context edits before target counting",
        ));
    }
    if input.mcp_servers.is_some() || input.speed.is_some() {
        return Err(TransformError::unsupported(
            "mcp/speed",
            "Gemini count needs host connector policy",
        ));
    }
    let mut report = Report::default();
    let mut ids = flow.clone();
    let mut calls = pair::history::Calls::default();
    for (id, name) in context.tool_names {
        calls.seed(id, name)?;
    }
    let config = to_gemini_config(&input)?;
    let (tools, choice) = pair::tools::to_gemini(input.tools, input.tool_choice, &mut report)?;
    let mut system = Vec::new();
    if let Some(prompt) = input.system {
        match prompt {
            c::SystemPrompt::Text(text) => system.push(g::Part::builder().text(text).build()),
            c::SystemPrompt::Blocks(blocks) => {
                for block in blocks {
                    system.push(g::Part::builder().text(block.text).build());
                }
            }
        }
    }
    let mut contents = Vec::new();
    for message in input.messages {
        let blocks = match message.content {
            cc::MessageContent::Text(text) => vec![cc::ContentBlock::Text(
                cc::TextBlock::builder(cc::TextBlockType::Tag, text).build(),
            )],
            cc::MessageContent::Blocks(blocks) => blocks,
        };
        let parts = pair::history::to_gemini(
            blocks,
            &context.media,
            &mut calls,
            &mut ids,
            policy,
            &mut report,
        )?;
        match message.role {
            cc::Role::System => {
                if parts.iter().any(|part| part.text.is_none()) {
                    return Err(TransformError::unsupported("system", "plain text required"));
                }
                system.extend(parts);
            }
            cc::Role::User => {
                contents.push(g::Content::builder().role("user").parts(parts).build())
            }
            cc::Role::Assistant => {
                contents.push(g::Content::builder().role("model").parts(parts).build())
            }
        }
    }
    if input.cache_control.is_some() {
        report.omitted("cache_control", "Gemini count has no Claude cache marker");
    }
    let system = (!system.is_empty()).then(|| g::Content::builder().parts(system).build());
    let output = super::gemini_target(model, contents, system, tools, choice, config);
    *flow = ids;
    Ok(Converted {
        value: output,
        report,
    })
}
pub fn gemini_to_claude(
    input: g::CountTokensRequestBody,
    target_model: impl Into<String>,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<c::CountTokensRequestBody>, TransformError> {
    super::policy(policy, crate::Dialect::Claude)?;
    let model = super::model(target_model)?;
    let input = super::gemini_input(input.into_declared())?;
    super::controls::gemini_policy(&input)?;
    let mut report = Report::default();
    let mut ids = flow.clone();
    let mut calls = pair::history::Calls::default();
    let mut out = c::CountTokensRequestBody::builder(Vec::new(), model).build();
    to_claude_config(input.generation_config.as_ref(), &mut out, &mut report)?;
    let (tools, choice) = pair::tools::to_claude(input.tools, input.tool_config, &mut report)?;
    out.tools = tools;
    out.tool_choice = choice;
    let mut system = Vec::new();
    if let Some(content) = input.system_instruction {
        for part in content.parts.unwrap_or_default() {
            pair::history::check_part(&part)?;
            if part.thought == Some(true) || part.text.is_none() {
                return Err(TransformError::unsupported(
                    "system_instruction",
                    "plain text required",
                ));
            }
            system.push(cc::TextBlock::builder(cc::TextBlockType::Tag, part.text.unwrap()).build());
        }
    }
    for content in input.contents {
        let role = match content.role.as_deref() {
            None | Some("user") => cc::Role::User,
            Some("model") => cc::Role::Assistant,
            Some("system") => cc::Role::System,
            Some(_) => return Err(TransformError::shape("contents.role", "unknown role")),
        };
        let blocks = pair::history::to_claude(
            content.parts.unwrap_or_default(),
            &mut calls,
            &mut ids,
            policy,
            &mut report,
        )?;
        if role == cc::Role::System {
            for block in blocks {
                if let cc::ContentBlock::Text(text) = block {
                    system.push(text);
                } else {
                    return Err(TransformError::unsupported("system", "plain text required"));
                }
            }
        } else {
            out.messages
                .push(cc::Message::builder(role, cc::MessageContent::Blocks(blocks)).build());
        }
    }
    if !system.is_empty() {
        out.system = Some(c::SystemPrompt::Blocks(system));
    }
    *flow = ids;
    Ok(Converted { value: out, report })
}
fn to_gemini_config(
    input: &c::CountTokensRequestBody,
) -> Result<Option<g::GenerationConfig>, TransformError> {
    let mut out = g::GenerationConfig::builder().build();
    let mut present = false;
    if input
        .output_config
        .as_ref()
        .is_some_and(|v| v.task_budget.is_some())
    {
        return Err(TransformError::unsupported(
            "task_budget",
            "no Gemini count task budget",
        ));
    }
    if let Some(thinking) = &input.thinking {
        let mut config = g::ThinkingConfig::builder().build();
        match thinking {
            c::ThinkingConfig::Disabled(_) => config.thinking_budget = Some(0),
            c::ThinkingConfig::Enabled(v) => {
                if v.display.is_some() {
                    return Err(TransformError::unsupported(
                        "thinking.display",
                        "no identical display policy",
                    ));
                }
                config.thinking_budget = Some(v.budget_tokens);
            }
            c::ThinkingConfig::Adaptive(v) => {
                if v.display.is_some() {
                    return Err(TransformError::unsupported(
                        "thinking.display",
                        "no identical display policy",
                    ));
                }
            }
        }
        out.thinking_config = Some(config);
        present = true;
    }
    if let Some(effort) = input.output_config.as_ref().and_then(|v| v.effort) {
        if matches!(
            input.thinking,
            Some(c::ThinkingConfig::Disabled(_) | c::ThinkingConfig::Enabled(_))
        ) {
            return Err(TransformError::unsupported(
                "thinking/effort",
                "Gemini cannot combine explicit budget with effort level",
            ));
        }
        let level = match effort {
            c::Effort::Low => g::ThinkingLevel::Low,
            c::Effort::Medium => g::ThinkingLevel::Medium,
            c::Effort::High => g::ThinkingLevel::High,
            c::Effort::Xhigh | c::Effort::Max => {
                return Err(TransformError::unsupported("effort", "Gemini lacks level"));
            }
        };
        out.thinking_config
            .get_or_insert_with(|| g::ThinkingConfig::builder().build())
            .thinking_level = Some(level);
        present = true;
    }
    let current = input.output_config.as_ref().and_then(|v| v.format.as_ref());
    if let (Some(a), Some(b)) = (current, input.output_format.as_ref())
        && a.schema != b.schema
    {
        return Err(TransformError::shape("schema", "conflicting output schema"));
    }
    if let Some(format) = current.or(input.output_format.as_ref()) {
        out.response_json_schema = Some(format.schema.clone());
        out.response_mime_type = Some("application/json".into());
        present = true;
    }
    Ok(present.then_some(out))
}
fn to_claude_config(
    input: Option<&g::GenerationConfig>,
    out: &mut c::CountTokensRequestBody,
    report: &mut Report,
) -> Result<(), TransformError> {
    let Some(input) = input else { return Ok(()) };
    if let Some(thinking) = &input.thinking_config {
        if thinking.include_thoughts == Some(false) {
            return Err(TransformError::unsupported(
                "include_thoughts",
                "Claude count lacks full thought suppression",
            ));
        }
        if thinking.thinking_budget.is_some() && thinking.thinking_level.is_some() {
            return Err(TransformError::shape("thinking", "budget/level conflict"));
        }
        if let Some(budget) = thinking.thinking_budget {
            out.thinking = Some(if budget == 0 {
                c::ThinkingConfig::Disabled(c::ThinkingDisabled::builder().build())
            } else if budget > 0 {
                c::ThinkingConfig::Enabled(c::ThinkingEnabled::builder(budget).build())
            } else {
                return Err(TransformError::missing_metadata(
                    "automatic thinking budget requires target model policy",
                ));
            });
        }
        if let Some(level) = &thinking.thinking_level {
            let effort = match level {
                g::ThinkingLevel::Low => c::Effort::Low,
                g::ThinkingLevel::Medium => c::Effort::Medium,
                g::ThinkingLevel::High => c::Effort::High,
                g::ThinkingLevel::Minimal | g::ThinkingLevel::Unspecified => {
                    return Err(TransformError::unsupported(
                        "thinking_level",
                        "no Claude equivalent",
                    ));
                }
            };
            out.thinking = Some(c::ThinkingConfig::Adaptive(
                c::ThinkingAdaptive::builder().build(),
            ));
            out.output_config = Some(c::OutputConfig::builder().effort(effort).build());
        }
    }
    let typed = input
        .response_schema
        .as_ref()
        .map(|v| crate::transform::generate::gemini_schema::to_json(v, Default::default()))
        .transpose()?;
    if let Some(v) = &typed {
        report.diagnostics.extend(v.report.diagnostics.clone());
    }
    if let (Some(a), Some(b)) = (
        &input.response_json_schema,
        &input.response_json_schema_internal,
    ) && a != b
    {
        return Err(TransformError::shape("schema", "conflicting raw schemas"));
    }
    let raw = input
        .response_json_schema
        .as_ref()
        .or(input.response_json_schema_internal.as_ref());
    if let (Some(a), Some(b)) = (&typed, raw)
        && Some(&a.value) != b.as_object()
    {
        return Err(TransformError::shape("schema", "typed/raw conflict"));
    }
    let schema = raw
        .cloned()
        .or_else(|| typed.map(|v| serde_json::Value::Object(v.value)));
    if let Some(schema) = schema {
        out.output_format =
            Some(c::JsonOutputFormat::builder(c::JsonOutputFormatType::JsonSchema, schema).build());
    } else if input.response_mime_type.as_deref() == Some("application/json") {
        return Err(TransformError::unsupported(
            "response_mime_type",
            "free JSON has no verified Claude count schema equivalent",
        ));
    }
    report.omitted(
        "generation_config.sampling",
        "output sampling not needed for input-token counting",
    );
    Ok(())
}
