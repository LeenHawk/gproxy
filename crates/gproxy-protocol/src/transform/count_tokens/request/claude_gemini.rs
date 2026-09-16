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
    let mut position = crate::transform::instructions::Position::default();
    for (index, message) in input.messages.into_iter().enumerate() {
        let leading = position.leading(message.role == cc::Role::System);
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
                crate::transform::instructions::gemini(
                    parts,
                    leading,
                    &mut system,
                    &mut contents,
                    format!("messages[{index}].role"),
                    &mut report,
                )?;
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
            system.push(cc::TextBlock::builder(cc::TextBlockType::Tag, part.text.unwrap()).build());
        }
    }
    let mut position = crate::transform::instructions::Position::default();
    for content in input.contents {
        let role = match content.role.as_deref() {
            None | Some("user") => cc::Role::User,
            Some("model") => cc::Role::Assistant,
            Some("system") => cc::Role::System,
            Some(_) => return Err(TransformError::shape("contents.role", "unknown role")),
        };
        let leading = position.leading(role == cc::Role::System);
        let blocks = pair::history::to_claude(
            content.parts.unwrap_or_default(),
            &mut calls,
            &mut ids,
            policy,
            &mut report,
        )?;
        if role == cc::Role::System && leading {
            system.extend(blocks.into_iter().filter_map(|block| match block {
                cc::ContentBlock::Text(text) => Some(text),
                _ => None,
            }));
            continue;
        }
        out.messages
            .push(cc::Message::builder(role, cc::MessageContent::Blocks(blocks)).build());
    }
    if !system.is_empty() {
        out.system = Some(c::SystemPrompt::Blocks(system));
    }
    crate::transform::instructions::claude(&mut out.messages, &out.model, &mut report);
    *flow = ids;
    Ok(Converted { value: out, report })
}
fn to_gemini_config(
    input: &c::CountTokensRequestBody,
) -> Result<Option<g::GenerationConfig>, TransformError> {
    let mut out = g::GenerationConfig::builder().build();
    let mut present = false;

    if let Some(thinking) = &input.thinking {
        let mut config = g::ThinkingConfig::builder().build();
        match thinking {
            c::ThinkingConfig::Disabled(_) => config.thinking_budget = Some(0),
            c::ThinkingConfig::Enabled(v) => {
                config.thinking_budget = Some(v.budget_tokens);
            }
            c::ThinkingConfig::Adaptive(_v) => {}
        }
        out.thinking_config = Some(config);
        present = true;
    }
    if let Some(effort) = input.output_config.as_ref().and_then(|v| v.effort) {
        let level = match effort {
            c::Effort::Low => Some(g::ThinkingLevel::Low),
            c::Effort::Medium => Some(g::ThinkingLevel::Medium),
            c::Effort::High => Some(g::ThinkingLevel::High),
            c::Effort::Xhigh | c::Effort::Max => None,
        };
        if let Some(level) = level {
            out.thinking_config
                .get_or_insert_with(|| g::ThinkingConfig::builder().build())
                .thinking_level = Some(level);
            present = true;
        }
    }
    let current = input.output_config.as_ref().and_then(|v| v.format.as_ref());

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
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    if let Some(v) = &typed {
        report.diagnostics.extend(v.report.diagnostics.clone());
    }

    let raw = input
        .response_json_schema
        .as_ref()
        .or(input.response_json_schema_internal.as_ref());

    let schema = raw
        .cloned()
        .or_else(|| typed.map(|v| serde_json::Value::Object(v.value)));
    if let Some(schema) = schema {
        out.output_format =
            Some(c::JsonOutputFormat::builder(c::JsonOutputFormatType::JsonSchema, schema).build());
    }
    report.omitted(
        "generation_config.sampling",
        "output sampling not needed for input-token counting",
    );
    Ok(())
}
