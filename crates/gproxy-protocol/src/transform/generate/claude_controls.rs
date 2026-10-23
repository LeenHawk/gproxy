//! Project Claude's conversation-scoped controls onto a single target request.
use crate::{
    Rest,
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
                c::MessageContent::Absent => true,
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

/// Newer models reject disabled thinking and forced tool choice. Sonnet 5.5
/// replaces disabled thinking with between-tools thinking.
/// Apply only to cross-protocol output; native Claude requests stay untouched.
pub(crate) fn target(
    messages: &mut Vec<c::Message>,
    model: &str,
    thinking: &mut Option<cc::ThinkingConfig>,
    choice: &mut Option<cc::ToolChoice>,
    report: &mut Report,
) {
    target_messages(messages, report);
    let model = model.to_ascii_lowercase();
    // Haiku 5.5 keeps disabled thinking and forced tools, but removes budgets.
    if crate::transform::instructions::family(&model, "claude-haiku-5-5") {
        if let Some(cc::ThinkingConfig::Enabled(config)) = thinking.as_ref() {
            *thinking = Some(cc::ThinkingConfig::Adaptive(cc::ThinkingAdaptive {
                block_binding: config.block_binding.clone(),
                display: config.display,
                rest: Rest::new(),
            }));
            report.changed(
                "thinking.type",
                "Haiku 5.5 uses adaptive thinking instead of manual budgets",
            );
            report.omitted(
                "thinking.budget_tokens",
                "Haiku 5.5 does not accept a thinking token budget",
            );
        }
        return;
    }
    let sonnet = crate::transform::instructions::family(&model, "claude-sonnet-5-5");
    if !sonnet
        && !["claude-opus-5-5", "claude-fable-5-1", "claude-mythos-5-1"]
            .iter()
            .any(|name| crate::transform::instructions::family(&model, name))
    {
        return;
    }
    match thinking.take() {
        Some(cc::ThinkingConfig::Disabled(_)) if sonnet => {
            *thinking = Some(cc::ThinkingConfig::BetweenTools);
            report.changed(
                "thinking.type",
                "Sonnet 5.5 uses between_tools for its lowest thinking setting",
            );
        }
        Some(cc::ThinkingConfig::Disabled(_)) => {
            report.omitted("thinking", "target model always uses adaptive thinking");
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

fn target_messages(messages: &mut Vec<c::Message>, report: &mut Report) {
    let mut output = Vec::with_capacity(messages.len());
    for (index, mut message) in messages.drain(..).enumerate() {
        if let c::MessageContent::Blocks(blocks) = &mut message.content {
            if message.role == c::Role::User {
                for block in blocks.iter_mut() {
                    user_block(block, report);
                }
            }
            if message.role == c::Role::Assistant
                && blocks
                    .iter()
                    .any(|block| matches!(block, c::ContentBlock::Document(_)))
            {
                // Keep every block in its original order, moving only direct
                // documents to user messages. Nested tool results stay intact.
                for mut block in blocks.drain(..) {
                    let role = if matches!(block, c::ContentBlock::Document(_)) {
                        user_block(&mut block, report);
                        report.changed(
                            format!("messages[{index}].content.document"),
                            "Claude API 2026-10-21 requires direct documents in user messages",
                        );
                        c::Role::User
                    } else {
                        c::Role::Assistant
                    };
                    output.push(
                        c::Message::builder(role, c::MessageContent::Blocks(vec![block])).build(),
                    );
                }
                continue;
            }
        }
        output.push(message);
    }
    *messages = output;
}

fn text(block: &mut c::TextBlock, report: &mut Report) {
    if block
        .citations
        .as_ref()
        .is_some_and(|citations| !citations.is_empty())
    {
        block.citations = None;
        report.omitted(
            "messages.user.content.text.citations",
            "Claude API 2026-10-21 rejects citations on user text blocks",
        );
    }
}

fn search(block: &mut c::SearchResultBlock, report: &mut Report) {
    for block in &mut block.content {
        text(block, report);
    }
}

fn document(block: &mut c::DocumentBlock, report: &mut Report) {
    if let c::DocumentSource::Content(source) = &mut block.source
        && let c::DocumentContent::Blocks(blocks) = &mut source.content
    {
        for block in blocks {
            if let c::DocumentContentBlock::Text(block) = block {
                text(block, report);
            }
        }
    }
}

fn user_block(block: &mut c::ContentBlock, report: &mut Report) {
    match block {
        c::ContentBlock::Text(block) => text(block, report),
        c::ContentBlock::SearchResult(block) => search(block, report),
        c::ContentBlock::Document(block) => document(block, report),
        c::ContentBlock::McpToolResult(block) => {
            if let Some(c::McpResultContent::Blocks(blocks)) = &mut block.content {
                for block in blocks {
                    text(block, report);
                }
            }
        }
        c::ContentBlock::WebFetchToolResult(block) => {
            if let c::WebFetchResultContent::Result(result) = &mut block.content {
                document(&mut result.content, report);
            }
        }
        c::ContentBlock::ToolResult(block) => {
            if let Some(c::ToolResultContent::Blocks(blocks)) = &mut block.content {
                for block in blocks {
                    match block {
                        c::ToolResultContentBlock::Text(block) => text(block, report),
                        c::ToolResultContentBlock::SearchResult(block) => search(block, report),
                        c::ToolResultContentBlock::Document(block) => document(block, report),
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn current_api_moves_documents_and_strips_only_user_text_citations() {
        use super::*;
        use crate::claude::generate_content::GenerateContentRequestBody;
        let source = serde_json::json!({
            "model":"claude-sonnet-4-6", "max_tokens":32,
            "messages":[
                {"role":"assistant", "content":[
                    {"type":"text", "text":"before"},
                    {"type":"document", "source":{"type":"content", "content":[{"type":"text", "text":"document", "citations":[{"type":"char_location", "cited_text":"doc", "document_index":0, "document_title":"title", "start_char_index":0, "end_char_index":3}]}]}, "citations":{"enabled":true}},
                    {"type":"text", "text":"after", "citations":[{"type":"char_location", "cited_text":"answer", "document_index":0, "document_title":"title", "start_char_index":0, "end_char_index":6}]}
                ]},
                {"role":"user", "content":[{"type":"search_result", "source":"https://example.com", "title":"result", "citations":{"enabled":true}, "content":[
                    {"type":"text", "text":"quoted", "citations":[{"type":"char_location", "cited_text":"quote", "document_index":0, "document_title":"title", "start_char_index":0, "end_char_index":5}]}
                ]}]}
            ]
        });
        let mut request: GenerateContentRequestBody = serde_json::from_value(source).unwrap();
        let mut report = Report::default();
        target(
            &mut request.messages,
            &request.model,
            &mut request.thinking,
            &mut request.tool_choice,
            &mut report,
        );
        let output = serde_json::to_value(&request).unwrap();
        assert_eq!(output["messages"][0]["content"][0]["text"], "before");
        assert_eq!(output["messages"][1]["role"], "user");
        assert_eq!(
            output["messages"][1]["content"][0]["source"]["content"][0]["text"],
            "document"
        );
        assert!(
            output["messages"][1]["content"][0]["source"]["content"][0]
                .get("citations")
                .is_none()
        );
        assert_eq!(
            output["messages"][1]["content"][0]["citations"]["enabled"],
            true
        );
        assert_eq!(output["messages"][2]["content"][0]["text"], "after");
        assert!(output["messages"][2]["content"][0]["citations"].is_array());
        assert_eq!(
            output["messages"][3]["content"][0]["citations"]["enabled"],
            true
        );
        assert!(
            output["messages"][3]["content"][0]["content"][0]
                .get("citations")
                .is_none()
        );
    }
}
