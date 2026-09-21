//! Fixed instruction-role compatibility. Never relocate an instruction after
//! conversation history starts, merge turns, or manufacture assistant/tool turns.

use super::{Report, TransformError};
use crate::wire::{
    claude::content as c,
    gemini as g,
    openai::{chat as h, responses as r},
};

#[derive(Default)]
pub(crate) struct Position {
    started: bool,
}

impl Position {
    /// Call for every source turn, including turns whose content is empty.
    pub(crate) fn leading(&mut self, instruction: bool) -> bool {
        self.started |= !instruction;
        !self.started
    }
}

// Recognize model families with vendor prefixes and version/date suffixes,
// without treating e.g. opus-4-80 as opus-4-8.
fn family(model: &str, name: &str) -> bool {
    model.match_indices(name).any(|(start, _)| {
        (start == 0 || matches!(model.as_bytes()[start - 1], b'/' | b'.' | b':'))
            && model[start + name.len()..]
                .bytes()
                .next()
                .is_none_or(|next| matches!(next, b'-' | b'.' | b'@' | b':'))
    })
}

pub(crate) fn claude(messages: &mut [c::Message], model: &str, report: &mut Report) {
    let model = model.to_ascii_lowercase();
    // Sonnet 5 does not support this feature. Unknown models must not inherit it
    // simply because they are absent from an old-model denylist.
    let supported = [
        "claude-opus-4-8",
        "claude-opus-5",
        "claude-fable-5",
        "claude-mythos-5",
    ]
    .iter()
    .any(|name| family(&model, name));
    // Validate each consecutive system run against its surrounding turns.
    // https://platform.claude.com/docs/en/build-with-claude/mid-conversation-system-messages
    let mut start = 0;
    while start < messages.len() {
        if messages[start].role != c::Role::System {
            start += 1;
            continue;
        }
        let end = start
            + messages[start..]
                .iter()
                .take_while(|m| m.role == c::Role::System)
                .count();
        let previous_allowed = start.checked_sub(1).is_some_and(|index| {
            let previous = &messages[index];
            previous.role == c::Role::User
                || (previous.role == c::Role::Assistant && ends_in_server_tool_result(previous))
        });
        let next_allowed = messages
            .get(end)
            .is_none_or(|next| next.role == c::Role::Assistant);
        let reason = if !supported {
            Some("target model has no verified mid-conversation system support")
        } else if !previous_allowed {
            Some("system must follow a user turn or an assistant ending in a server tool result")
        } else if !next_allowed {
            Some("system must precede an assistant turn or end the request")
        } else {
            None
        };
        if let Some(reason) = reason {
            for (offset, message) in messages[start..end].iter_mut().enumerate() {
                message.role = c::Role::User;
                report.changed(
                    format!("messages[{}].role", start + offset),
                    format!("system downgraded to user in place: {reason}"),
                );
            }
        }
        start = end;
    }
}

fn ends_in_server_tool_result(message: &c::Message) -> bool {
    let c::MessageContent::Blocks(blocks) = &message.content else {
        return false;
    };
    matches!(
        blocks.last(),
        Some(
            c::ContentBlock::WebSearchToolResult(_)
                | c::ContentBlock::WebFetchToolResult(_)
                | c::ContentBlock::AdvisorToolResult(_)
                | c::ContentBlock::CodeExecutionToolResult(_)
                | c::ContentBlock::BashCodeExecutionToolResult(_)
                | c::ContentBlock::TextEditorCodeExecutionToolResult(_)
                | c::ContentBlock::ToolSearchToolResult(_)
                | c::ContentBlock::McpToolResult(_)
        )
    )
}

pub(crate) fn gemini(
    parts: Vec<g::Part>,
    leading: bool,
    system: &mut Vec<g::Part>,
    contents: &mut Vec<g::Content>,
    field: String,
    report: &mut Report,
) -> Result<(), TransformError> {
    if parts.iter().any(|part| part.text.is_none()) {
        return Err(TransformError::unsupported(
            field,
            "system instructions require plain text",
        ));
    }
    if leading {
        system.extend(parts);
    } else {
        contents.push(g::Content::builder().role("user").parts(parts).build());
        report.changed(
            field,
            "instruction role downgraded to user in place: Gemini has no message-level instruction role",
        );
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OpenAiRole {
    System,
    Developer,
    User,
}

fn openai_role(model: &str, source: OpenAiRole) -> OpenAiRole {
    let model = model.to_ascii_lowercase();
    if ["o1-preview", "o1-mini"]
        .iter()
        .any(|name| family(&model, name))
    {
        OpenAiRole::User
    } else if ["o1", "o3", "o4-mini"]
        .iter()
        .any(|name| family(&model, name))
    {
        OpenAiRole::Developer
    } else if ["gpt-3.5-turbo", "gpt-4o-2024-05-13"]
        .iter()
        .any(|name| family(&model, name))
        || (family(&model, "gpt-4") && !model.contains("gpt-4."))
    {
        OpenAiRole::System
    } else {
        // Both roles belong to the OpenAI wire contract. Do not guess a
        // restriction for other vendors' compatible endpoints or modern models.
        source
    }
}

fn role_change(field: String, role: OpenAiRole, report: &mut Report) {
    let name = match role {
        OpenAiRole::System => "system",
        OpenAiRole::Developer => "developer",
        OpenAiRole::User => "user",
    };
    report.changed(
        field,
        format!("instruction role mapped to {name} in place for target model"),
    );
}

pub(crate) fn chat(messages: &mut [h::ChatMessage], model: &str, report: &mut Report) {
    for (index, message) in messages.iter_mut().enumerate() {
        let (source, content, name) = match message {
            h::ChatMessage::System(message) => {
                (OpenAiRole::System, &message.content, &message.name)
            }
            h::ChatMessage::Developer(message) => {
                (OpenAiRole::Developer, &message.content, &message.name)
            }
            _ => continue,
        };
        let role = openai_role(model, source);
        if role == source {
            continue;
        }
        let content = content.clone();
        let name = name.clone();
        *message = match role {
            OpenAiRole::System => h::ChatMessage::System(h::SystemMessage {
                role: h::SystemRole::System,
                content,
                name,
                rest: Default::default(),
            }),
            OpenAiRole::Developer => h::ChatMessage::Developer(h::DeveloperMessage {
                role: h::DeveloperRole::Developer,
                content,
                name,
                rest: Default::default(),
            }),
            OpenAiRole::User => h::ChatMessage::User(h::UserMessage {
                role: h::UserRole::User,
                content: match content {
                    h::TextContent::Text(text) => h::UserContent::Text(text),
                    h::TextContent::Parts(parts) => h::UserContent::Parts(
                        parts.into_iter().map(h::UserContentPart::Text).collect(),
                    ),
                },
                name,
                rest: Default::default(),
            }),
        };
        role_change(format!("messages[{index}].role"), role, report);
    }
}

pub(crate) fn responses(
    input: &mut Option<r::Input>,
    instructions: &mut Option<Option<String>>,
    model: &str,
    report: &mut Report,
) {
    // Models without either instruction role cannot consume a top-level
    // instruction either. Keep it at the start as a separate user turn.
    if openai_role(model, OpenAiRole::System) == OpenAiRole::User
        && let Some(Some(text)) = instructions.take()
    {
        let first = r::InputItem::Easy(
            r::EasyInputMessage::builder(r::MessageContent::Text(text), r::MessageRole::User)
                .build(),
        );
        let mut items = match input.take() {
            Some(r::Input::Items(items)) => items,
            Some(r::Input::Text(text)) => vec![r::InputItem::Easy(
                r::EasyInputMessage::builder(r::MessageContent::Text(text), r::MessageRole::User)
                    .build(),
            )],
            None => Vec::new(),
        };
        items.insert(0, first);
        *input = Some(r::Input::Items(items));
        role_change("instructions".into(), OpenAiRole::User, report);
    }
    let Some(r::Input::Items(items)) = input else {
        return;
    };
    for (index, item) in items.iter_mut().enumerate() {
        let (source, role) = match item {
            r::InputItem::Easy(message) => {
                let source = match message.role {
                    r::MessageRole::System => OpenAiRole::System,
                    r::MessageRole::Developer => OpenAiRole::Developer,
                    _ => continue,
                };
                let role = openai_role(model, source);
                message.role = match role {
                    OpenAiRole::System => r::MessageRole::System,
                    OpenAiRole::Developer => r::MessageRole::Developer,
                    OpenAiRole::User => r::MessageRole::User,
                };
                (source, role)
            }
            r::InputItem::Message(message) => {
                let source = match message.role {
                    r::InputMessageRole::System => OpenAiRole::System,
                    r::InputMessageRole::Developer => OpenAiRole::Developer,
                    _ => continue,
                };
                let role = openai_role(model, source);
                message.role = match role {
                    OpenAiRole::System => r::InputMessageRole::System,
                    OpenAiRole::Developer => r::InputMessageRole::Developer,
                    OpenAiRole::User => r::InputMessageRole::User,
                };
                (source, role)
            }
            _ => continue,
        };
        if source != role {
            role_change(format!("input[{index}].role"), role, report);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn system_can_end_after_a_server_tool_result_but_not_after_following_text() {
        for trailing_text in [false, true] {
            let mut blocks = vec![
                json!({"type":"server_tool_use","id":"srvtoolu_1","name":"web_search","input":{"query":"test"}}),
                json!({"type":"web_search_tool_result","tool_use_id":"srvtoolu_1","content":[]}),
            ];
            if trailing_text {
                blocks.push(json!({"type":"text","text":"Finished searching."}));
            }
            let mut messages: Vec<c::Message> = serde_json::from_value(json!([
                {"role":"user","content":"Search."},
                {"role":"assistant","content":blocks},
                {"role":"system","content":"Now answer in Chinese."}
            ]))
            .unwrap();
            let previous = messages[..2].to_vec();
            claude(&mut messages, "claude-opus-4-8", &mut Report::default());
            assert_eq!(messages[..2], previous);
            assert_eq!(
                messages[2].role,
                if trailing_text {
                    c::Role::User
                } else {
                    c::Role::System
                }
            );
        }
    }

    #[test]
    fn system_can_end_after_a_client_tool_result() {
        let mut messages: Vec<c::Message> = serde_json::from_value(json!([
            {"role":"user","content":"Run tests."},
            {"role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"run_tests","input":{}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"Passed."}]},
            {"role":"system","content":"Update the changelog."}
        ])).unwrap();
        let original = messages.clone();
        claude(&mut messages, "claude-opus-4-8", &mut Report::default());
        assert_eq!(messages, original);
    }
}
