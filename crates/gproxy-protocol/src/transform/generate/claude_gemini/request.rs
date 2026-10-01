use super::{history::Calls, media::MediaFacts};
use crate::{
    transform::{
        Converted, Report, TransformError,
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::{
        DeclaredFields,
        claude::{content as c, count_tokens as ct, generate_content as cg},
        gemini as g,
    },
};

/// Concrete media facts obtained by the caller's resource capabilities.
#[derive(Default)]
pub struct ClaudeGeminiRequestContext {
    pub media: MediaFacts,
    /// Tool IDs and names recovered from declared history/state when history is truncated.
    pub tool_names: std::collections::BTreeMap<String, String>,
}

pub fn claude_to_gemini_request(
    input: cg::GenerateContentRequestBody,
    target_model: impl Into<String>,
    context: ClaudeGeminiRequestContext,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<g::GenerateContentRequestBody>, TransformError> {
    let mut input = input.into_declared();
    let mut ids = flow.clone();
    let mut report = Report::default();
    super::super::claude_controls::project(&mut input, &mut report);
    let config = super::config::to_gemini(&input, &mut report)?;
    let (tools, tool_config) =
        super::tools::to_gemini(input.tools, input.tool_choice, &mut report)?;
    let mut calls = Calls::default();
    for (id, name) in context.tool_names {
        calls.seed(id, name)?;
    }
    let mut system = Vec::new();
    if let Some(prompt) = input.system {
        match prompt {
            ct::SystemPrompt::Text(text) => system.push(g::Part::builder().text(text).build()),
            ct::SystemPrompt::Blocks(blocks) => {
                for block in blocks {
                    if block.cache_control.is_some() {
                        report.omitted(
                            "system.cache_control",
                            "Gemini has no per-block cache field",
                        );
                    }
                    system.push(g::Part::builder().text(block.text).build());
                }
            }
        }
    }
    let mut contents = Vec::new();
    let mut position = crate::transform::instructions::Position::default();
    for (index, message) in input.messages.into_iter().enumerate() {
        let leading = position.leading(message.role == c::Role::System);
        let blocks = match message.content {
            c::MessageContent::Text(text) => vec![c::ContentBlock::Text(
                c::TextBlock::builder(c::TextBlockType::Tag, text).build(),
            )],
            c::MessageContent::Blocks(blocks) => blocks,
        };
        let parts = super::history::to_gemini(
            blocks,
            &context.media,
            &mut calls,
            &mut ids,
            policy,
            &mut report,
        )?;
        if message.role == c::Role::System {
            crate::transform::instructions::gemini(
                parts,
                leading,
                &mut system,
                &mut contents,
                format!("messages[{index}].role"),
                &mut report,
            )?;
        } else if !parts.is_empty() {
            let role = match message.role {
                c::Role::User => "user",
                c::Role::Assistant => "model",
                // System roles are consumed by the instructions branch above.
                c::Role::System => unreachable!(),
            };
            contents.push(
                g::Content::builder()
                    .parts(parts)
                    .role(role.to_owned())
                    .build(),
            );
        }
    }
    let mut out = g::GenerateContentRequestBody::builder(contents).build();
    out.tools = tools;
    out.tool_config = tool_config;
    out.generation_config = Some(config);
    if !system.is_empty() {
        out.system_instruction = Some(g::Content::builder().parts(system).build());
    }
    *flow = ids;
    super::super::gemini_controls::for_model(&mut out, &target_model.into(), &mut report);
    Ok(Converted { value: out, report })
}

pub fn gemini_to_claude_request(
    input: g::GenerateContentRequestBody,
    target_model: impl Into<String>,
    max_tokens: Option<i64>,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<cg::GenerateContentRequestBody>, TransformError> {
    let model = target_model.into();

    let input = input.into_declared();
    let source_max = input
        .generation_config
        .as_ref()
        .and_then(|v| v.max_output_tokens);

    let max = source_max
        .or(max_tokens)
        .ok_or_else(|| TransformError::missing_metadata("max_tokens"))?;

    let mut out = cg::GenerateContentRequestBody::builder(max, Vec::new(), model).build();
    let mut report = Report::default();
    super::config::to_claude(&input, &mut out, &mut report)?;
    let (tools, choice) = super::tools::to_claude(input.tools, input.tool_config, &mut report)?;
    out.tools = tools;
    out.tool_choice = choice;
    let mut system = Vec::new();
    if let Some(content) = input.system_instruction {
        for part in content.parts.unwrap_or_default() {
            system.push(
                part.text
                    .ok_or_else(|| TransformError::shape("system_instruction", "text required"))?,
            );
        }
    }
    let mut ids = flow.clone();
    let mut calls = Calls::default();
    let mut position = crate::transform::instructions::Position::default();
    for content in input.contents {
        let role = match content.role.as_deref() {
            Some("model") => c::Role::Assistant,
            Some("user") | None => c::Role::User,
            Some("system") => c::Role::System,
            Some(_) => return Err(TransformError::shape("contents.role", "unknown role")),
        };
        let leading = position.leading(role == c::Role::System);
        let blocks = super::history::to_claude(
            content.parts.unwrap_or_default(),
            &mut calls,
            &mut ids,
            policy,
            &mut report,
        )?;
        if role == c::Role::System && leading {
            system.extend(blocks.into_iter().filter_map(|block| match block {
                c::ContentBlock::Text(text) => Some(text.text),
                _ => None,
            }));
            continue;
        }
        if !blocks.is_empty() {
            out.messages
                .push(c::Message::builder(role, c::MessageContent::Blocks(blocks)).build());
        }
    }
    if !system.is_empty() {
        out.system = Some(ct::SystemPrompt::Text(system.join("\n")));
    }
    crate::transform::instructions::claude(&mut out.messages, &out.model, &mut report);
    *flow = ids;
    crate::transform::generate::claude_controls::target(
        &out.model,
        &mut out.thinking,
        &mut out.tool_choice,
        &mut report,
    );
    Ok(Converted { value: out, report })
}
