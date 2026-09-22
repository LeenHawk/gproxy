//! Project Claude's conversation-scoped controls onto a single target request.
use crate::{
    transform::Report,
    wire::claude::{content as c, count_tokens as cc, generate_content as g, tools as t},
};

pub(super) fn project(input: &mut g::GenerateContentRequestBody, report: &mut Report) {
    if input.compaction.is_some() {
        report.omitted(
            "compaction",
            "target has no Claude on-demand compaction operation",
        );
    }
    let binding = match &input.thinking {
        Some(cc::ThinkingConfig::Adaptive(v)) => v.block_binding.as_ref(),
        Some(cc::ThinkingConfig::Enabled(v)) => v.block_binding.as_ref(),
        _ => None,
    };
    if binding.is_some() {
        report.omitted(
            "thinking.block_binding",
            "binding checks belong to the originating Claude server",
        );
    }
    project_messages(
        &mut input.messages,
        &mut input.tools,
        &mut input.output_config,
        report,
    );
}

pub(crate) fn project_messages(
    input_messages: &mut Vec<c::Message>,
    input_tools: &mut Option<Vec<t::ToolUnion>>,
    output_config: &mut Option<cc::OutputConfig>,
    report: &mut Report,
) {
    let last_user = input_messages.iter().rposition(|m| m.role == c::Role::User);
    let mut tools = input_tools.take().unwrap_or_default();
    let mut definitions = tools
        .iter()
        .filter_map(|tool| match tool {
            t::ToolUnion::Custom(v) => Some((v.name.clone(), v.clone())),
            _ => None,
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut messages = Vec::new();
    for (index, mut message) in input_messages.drain(..).enumerate() {
        if message.role == c::Role::System {
            if let Some(effort) = message.output_config.as_ref().and_then(|c| c.effort) {
                output_config
                    .get_or_insert_with(|| cc::OutputConfig::builder().build())
                    .effort = Some(effort);
                report.changed(
                    "messages.output_config.effort",
                    "latest message effort applies to the target generation",
                );
            }
            // Turn-scoped system messages render no text after a later user message.
            let cleared = message.clear_at == Some(c::SystemMessageClearAt::NextUserMessage)
                && last_user.is_some_and(|last| last > index);
            match &mut message.content {
                c::MessageContent::Text(text) if cleared => text.clear(),
                c::MessageContent::Blocks(blocks) => {
                    project_blocks(blocks, &mut tools, &mut definitions, cleared, report);
                }
                _ => {}
            }
            let empty = match &message.content {
                c::MessageContent::Text(text) => text.is_empty(),
                c::MessageContent::Blocks(blocks) => blocks.is_empty(),
            };
            if empty {
                continue;
            }
        }
        messages.push(message);
    }
    *input_messages = messages;
    *input_tools = (!tools.is_empty()).then_some(tools);
}

fn project_blocks(
    blocks: &mut Vec<c::ContentBlock>,
    tools: &mut Vec<t::ToolUnion>,
    definitions: &mut std::collections::BTreeMap<String, t::Tool>,
    cleared: bool,
    report: &mut Report,
) {
    blocks.retain(|block| match block {
        c::ContentBlock::Text(_) => !cleared,
        c::ContentBlock::ToolAddition(v) => {
            match &v.tool {
                c::ToolAdditionReference::Definition(v) => {
                    if let t::ToolUnion::Custom(tool) = v.definition.as_ref() {
                        tools.retain(
                            |t| !matches!(t, t::ToolUnion::Custom(old) if old.name == tool.name),
                        );
                        definitions.insert(tool.name.clone(), tool.clone());
                        tools.push(t::ToolUnion::Custom(tool.clone()));
                        report.changed(
                            "tool_addition",
                            "inline definition projected into target tools",
                        );
                    } else {
                        report.omitted(
                            "tool_addition",
                            "native server tools and toolsets have no matching target definition",
                        );
                    }
                }
                c::ToolAdditionReference::Tool(reference) => {
                    if let Some(mut tool) = definitions.get(&reference.name).cloned() {
                        tool.defer_loading = None;
                        tools.retain(
                            |t| !matches!(t, t::ToolUnion::Custom(v) if v.name == reference.name),
                        );
                        tools.push(t::ToolUnion::Custom(tool));
                    } else {
                        report
                            .omitted("tool_addition", "referenced tool definition is unavailable");
                    }
                }
                _ => report.omitted(
                    "tool_addition",
                    "MCP tool changes have no matching target control",
                ),
            }
            false
        }
        c::ContentBlock::ToolRemoval(v) => {
            if let c::ToolChangeReference::Tool(reference) = &v.tool {
                tools.retain(|t| !matches!(t, t::ToolUnion::Custom(v) if v.name == reference.name));
                report.changed("tool_removal", "removed tool from target tools");
            } else {
                report.omitted(
                    "tool_removal",
                    "MCP tool changes have no matching target control",
                );
            }
            false
        }
        _ => true,
    });
}

/// These newer models reject disabled/manual thinking and forced tool choice.
/// Apply only to cross-protocol output; native Claude requests stay untouched.
pub(crate) fn target(
    model: &str,
    thinking: &mut Option<cc::ThinkingConfig>,
    choice: &mut Option<cc::ToolChoice>,
    report: &mut Report,
) {
    let model = model.to_ascii_lowercase();
    if !["claude-opus-5-5", "claude-fable-5-1", "claude-mythos-5-1"]
        .iter()
        .any(|name| crate::transform::instructions::family(&model, name))
    {
        return;
    }
    match thinking.take() {
        Some(cc::ThinkingConfig::Disabled(_)) => {
            report.omitted("thinking", "target model always uses adaptive thinking");
        }
        Some(cc::ThinkingConfig::Enabled(v)) => {
            *thinking = Some(cc::ThinkingConfig::Adaptive(cc::ThinkingAdaptive {
                display: v.display,
                block_binding: v.block_binding,
                rest: Default::default(),
            }));
            report.omitted(
                "thinking.budget_tokens",
                "target model uses adaptive thinking without a manual budget",
            );
        }
        other => *thinking = other,
    }
    let disable_parallel = match choice.as_ref() {
        Some(cc::ToolChoice::Any(v)) => Some(v.disable_parallel_tool_use),
        Some(cc::ToolChoice::Tool(v)) => Some(v.disable_parallel_tool_use),
        _ => None,
    };
    if let Some(disable_parallel_tool_use) = disable_parallel {
        *choice = Some(cc::ToolChoice::Auto(cc::ToolChoiceAuto {
            disable_parallel_tool_use,
            rest: Default::default(),
        }));
        report.changed(
            "tool_choice",
            "target supports auto/none only; forced choice relaxed to auto",
        );
    }
}
