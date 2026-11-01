//! Mend the tool exchange of a converted request for upstreams that validate it
//! strictly. Results answering one assistant turn follow it in the calls'
//! order, a second result for the same call is dropped (the first stands), and a
//! result answering no call of the turn before it becomes plain user content.
//! An unanswered call is left as it came: no result is made up for it.

use crate::wire::{
    claude::{content as cc, generate_content as c},
    openai::chat as h,
};

fn label(id: &str, error: bool) -> String {
    if error {
        format!("Tool error ({id})")
    } else {
        format!("Tool result ({id})")
    }
}

/// Chat: tool messages answering the calls of the assistant message before
/// them go in the calls' order; one answering none becomes a user message.
pub(super) fn chat(messages: &mut Vec<h::ChatMessage>) {
    if !messages
        .iter()
        .any(|message| matches!(message, h::ChatMessage::Tool(_)))
    {
        return;
    }
    #[derive(Default)]
    struct Exchange {
        calls: Vec<String>,
        answered: Vec<bool>,
        held: Vec<(usize, h::ToolMessage)>,
        orphans: Vec<h::ChatMessage>,
    }
    impl Exchange {
        /// Emit the results held so far in the calls' order; the exchange stays open.
        fn emit(&mut self, out: &mut Vec<h::ChatMessage>) {
            self.held.sort_by_key(|(index, _)| *index);
            out.extend(
                self.held
                    .drain(..)
                    .map(|(_, tool)| h::ChatMessage::Tool(tool)),
            );
        }
        fn flush(&mut self, out: &mut Vec<h::ChatMessage>) {
            self.emit(out);
            out.append(&mut self.orphans);
            self.calls.clear();
            self.answered.clear();
        }
    }
    let mut out = Vec::with_capacity(messages.len());
    let mut exchange = Exchange::default();
    for message in std::mem::take(messages) {
        match message {
            h::ChatMessage::Tool(tool) => {
                match exchange
                    .calls
                    .iter()
                    .position(|id| id == &tool.tool_call_id)
                {
                    Some(index) if !exchange.answered[index] => {
                        exchange.answered[index] = true;
                        exchange.held.push((index, tool));
                    }
                    Some(_) => {}
                    None => exchange.orphans.push(chat_orphan(tool)),
                }
            }
            // A legacy function result answers by name, outside the exchange.
            message @ h::ChatMessage::Function(_) => {
                exchange.emit(&mut out);
                out.push(message);
            }
            message => {
                exchange.flush(&mut out);
                if let h::ChatMessage::Assistant(assistant) = &message {
                    exchange.calls = assistant
                        .tool_calls
                        .iter()
                        .flatten()
                        .map(|call| match call {
                            h::MessageToolCall::Function(call) => call.id.clone(),
                            h::MessageToolCall::Custom(call) => call.id.clone(),
                        })
                        .collect();
                    exchange.answered = vec![false; exchange.calls.len()];
                }
                out.push(message);
            }
        }
    }
    exchange.flush(&mut out);
    *messages = out;
}

fn chat_orphan(tool: h::ToolMessage) -> h::ChatMessage {
    let label = label(&tool.tool_call_id, false);
    let content = match tool.content {
        h::TextContent::Text(text) if text.is_empty() => h::UserContent::Text(label),
        h::TextContent::Text(text) => h::UserContent::Text(format!("{label}: {text}")),
        h::TextContent::Parts(parts) => h::UserContent::Parts(
            std::iter::once(
                h::TextPart::builder(h::TextPartType::Text, format!("{label}:")).build(),
            )
            .chain(parts)
            .map(h::UserContentPart::Text)
            .collect(),
        ),
    };
    let mut user = h::UserMessage::builder(h::UserRole::User, content).build();
    user.rest = tool.rest;
    h::ChatMessage::User(user)
}

fn tool_calls(message: &cc::Message) -> Vec<String> {
    match &message.content {
        cc::MessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|block| match block {
                cc::ContentBlock::ToolUse(block) => Some(block.id.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn has_result(message: &cc::Message) -> bool {
    matches!(&message.content, cc::MessageContent::Blocks(blocks)
        if blocks.iter().any(|block| matches!(block, cc::ContentBlock::ToolResult(_))))
}

/// Claude: the tool_result blocks answering the assistant message before them
/// lead the user turn after it, in the calls' order; Anthropic reads
/// consecutive same-role messages as one turn, so user messages carrying
/// results are joined. A result answering no call of that turn becomes its content.
pub(super) fn claude(body: &mut c::GenerateContentRequestBody) {
    if !body.messages.iter().any(has_result) {
        return;
    }
    let mut out: Vec<cc::Message> = Vec::with_capacity(body.messages.len());
    let mut calls: Vec<String> = Vec::new();
    let mut run: Vec<cc::Message> = Vec::new();
    for message in std::mem::take(&mut body.messages) {
        if message.role == cc::Role::User {
            run.push(message);
            continue;
        }
        // Consecutive assistant messages are one turn to Anthropic too.
        let joined = run.is_empty()
            && message.role == cc::Role::Assistant
            && out
                .last()
                .is_some_and(|last| last.role == cc::Role::Assistant);
        close_run(&mut out, &mut run, &calls);
        if !joined {
            calls.clear();
        }
        if message.role == cc::Role::Assistant {
            calls.extend(tool_calls(&message));
        }
        out.push(message);
    }
    close_run(&mut out, &mut run, &calls);
    body.messages = out;
}

fn close_run(out: &mut Vec<cc::Message>, run: &mut Vec<cc::Message>, calls: &[String]) {
    if !run.iter().any(has_result) {
        out.append(run);
        return;
    }
    if calls.is_empty() {
        // No turn to answer: each message keeps its place, its results as content.
        for mut message in run.drain(..) {
            if let cc::MessageContent::Blocks(blocks) = &mut message.content {
                *blocks = std::mem::take(blocks)
                    .into_iter()
                    .flat_map(|block| match block {
                        cc::ContentBlock::ToolResult(result) => claude_orphan(result),
                        block => vec![block],
                    })
                    .collect();
            }
            out.push(message);
        }
        return;
    }
    let mut held: Vec<Option<cc::ToolResultBlock>> = vec![None; calls.len()];
    let mut rest = Vec::new();
    let mut messages = run.drain(..);
    let Some(mut first) = messages.next() else {
        return;
    };
    let mut contents = vec![std::mem::replace(
        &mut first.content,
        cc::MessageContent::Blocks(Vec::new()),
    )];
    for message in messages {
        first.rest.extend(message.rest);
        contents.push(message.content);
    }
    for content in contents {
        match content {
            cc::MessageContent::Absent => {}
            cc::MessageContent::Text(text) => rest.push(cc::ContentBlock::Text(
                cc::TextBlock::builder(cc::TextBlockType::Tag, text).build(),
            )),
            cc::MessageContent::Blocks(blocks) => {
                for block in blocks {
                    match block {
                        cc::ContentBlock::ToolResult(result) => {
                            match calls.iter().position(|id| id == &result.tool_use_id) {
                                Some(index) if held[index].is_none() => {
                                    held[index] = Some(result);
                                }
                                Some(_) => {}
                                None => rest.extend(claude_orphan(result)),
                            }
                        }
                        block => rest.push(block),
                    }
                }
            }
        }
    }
    let mut blocks: Vec<_> = held
        .into_iter()
        .flatten()
        .map(cc::ContentBlock::ToolResult)
        .collect();
    blocks.append(&mut rest);
    first.content = cc::MessageContent::Blocks(blocks);
    out.push(first);
}

fn claude_orphan(result: cc::ToolResultBlock) -> Vec<cc::ContentBlock> {
    let label = label(&result.tool_use_id, result.is_error == Some(true));
    let text = |text: String| {
        cc::ContentBlock::Text(cc::TextBlock::builder(cc::TextBlockType::Tag, text).build())
    };
    let mut blocks = match result.content {
        None => vec![text(label)],
        Some(cc::ToolResultContent::Text(value)) if value.is_empty() => vec![text(label)],
        Some(cc::ToolResultContent::Text(value)) => vec![text(format!("{label}: {value}"))],
        Some(cc::ToolResultContent::Blocks(parts)) => std::iter::once(text(format!("{label}:")))
            .chain(parts.into_iter().map(|part| match part {
                cc::ToolResultContentBlock::Text(block) => cc::ContentBlock::Text(block),
                cc::ToolResultContentBlock::Image(block) => cc::ContentBlock::Image(block),
                cc::ToolResultContentBlock::Document(block) => cc::ContentBlock::Document(block),
                cc::ToolResultContentBlock::SearchResult(block) => {
                    cc::ContentBlock::SearchResult(block)
                }
                part @ (cc::ToolResultContentBlock::BrowserState(_)
                | cc::ToolResultContentBlock::ToolReference(_)) => {
                    text(serde_json::to_string(&part).unwrap_or_default())
                }
            }))
            .collect(),
    };
    if let Some(control) = result.cache_control {
        let slot = match blocks.last_mut() {
            Some(cc::ContentBlock::Text(block)) => &mut block.cache_control,
            Some(cc::ContentBlock::Image(block)) => &mut block.cache_control,
            Some(cc::ContentBlock::Document(block)) => &mut block.cache_control,
            Some(cc::ContentBlock::SearchResult(block)) => &mut block.cache_control,
            _ => return blocks,
        };
        slot.get_or_insert(control);
    }
    blocks
}
