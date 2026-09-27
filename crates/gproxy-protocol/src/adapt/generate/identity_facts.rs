//! Only declared identity/name facts: this is not a content representation.

use crate::{
    Dialect,
    wire::{
        claude::generate_content as c,
        gemini as g,
        openai::{chat as h, responses as r},
    },
};

#[derive(Debug, Clone)]
pub(super) struct ToolIdentity {
    pub kind: super::ToolCallKind,
    pub chat_form: Option<super::ChatCallForm>,
    pub call_id: Option<String>,
    pub name: String,
}

pub(super) trait IdentityFacts {
    fn dialect(&self) -> Dialect;
    fn tools(&self) -> Vec<ToolIdentity>;
}

impl IdentityFacts for c::GenerateContentResponseBody {
    fn dialect(&self) -> Dialect {
        Dialect::Claude
    }
    fn tools(&self) -> Vec<ToolIdentity> {
        self.content
            .iter()
            .filter_map(|b| match b {
                c::ResponseContentBlock::ToolUse(b) => Some(ToolIdentity {
                    kind: super::ToolCallKind::Function,
                    chat_form: None,
                    call_id: Some(b.id.clone()),
                    name: b.name.clone(),
                }),
                _ => None,
            })
            .collect()
    }
}

impl IdentityFacts for h::GenerateContentResponseBody {
    fn dialect(&self) -> Dialect {
        Dialect::OpenAiChat
    }
    fn tools(&self) -> Vec<ToolIdentity> {
        self.choices
            .iter()
            .flat_map(|choice| {
                let legacy = choice
                    .message
                    .function_call
                    .iter()
                    .map(|call| ToolIdentity {
                        kind: super::ToolCallKind::Function,
                        chat_form: Some(super::ChatCallForm::LegacyFunction),
                        call_id: None,
                        name: call.name.clone(),
                    });
                let current = choice
                    .message
                    .tool_calls
                    .iter()
                    .flatten()
                    .map(|call| match call {
                        h::MessageToolCall::Function(call) => ToolIdentity {
                            kind: super::ToolCallKind::Function,
                            chat_form: Some(super::ChatCallForm::Modern),
                            call_id: Some(call.id.clone()),
                            name: call.function.name.clone(),
                        },
                        h::MessageToolCall::Custom(call) => ToolIdentity {
                            kind: super::ToolCallKind::Custom,
                            chat_form: Some(super::ChatCallForm::Modern),
                            call_id: Some(call.id.clone()),
                            name: call.custom.name.clone(),
                        },
                    });
                legacy.chain(current)
            })
            .collect()
    }
}

impl IdentityFacts for g::GenerateContentResponseBody {
    fn dialect(&self) -> Dialect {
        Dialect::Gemini
    }
    fn tools(&self) -> Vec<ToolIdentity> {
        self.candidates
            .iter()
            .flatten()
            .filter_map(|c| c.content.as_ref())
            .flat_map(|c| c.parts.iter().flatten())
            .filter_map(|p| p.function_call.as_ref())
            .map(|b| ToolIdentity {
                kind: super::ToolCallKind::Function,
                chat_form: None,
                call_id: b.id.clone(),
                name: b.name.clone(),
            })
            .collect()
    }
}

impl IdentityFacts for r::GenerateContentResponseBody {
    fn dialect(&self) -> Dialect {
        Dialect::OpenAi
    }
    fn tools(&self) -> Vec<ToolIdentity> {
        self.output
            .iter()
            .filter_map(|b| match b {
                r::ResponseOutputItem::FunctionCall(b) => Some(ToolIdentity {
                    kind: super::ToolCallKind::Function,
                    chat_form: None,
                    call_id: Some(b.call_id.clone()),
                    name: b.namespace.as_ref().map_or_else(
                        || b.name.clone(),
                        |ns| crate::transform::generate::client_tools::qualified(ns, &b.name),
                    ),
                }),
                r::ResponseOutputItem::ShellCall(b) => Some(ToolIdentity {
                    kind: super::ToolCallKind::Function,
                    chat_form: None,
                    call_id: Some(b.call_id.clone()),
                    name: crate::transform::generate::client_tools::SHELL.into(),
                }),
                r::ResponseOutputItem::ApplyPatchCall(b) => Some(ToolIdentity {
                    kind: super::ToolCallKind::Function,
                    chat_form: None,
                    call_id: Some(b.call_id.clone()),
                    name: crate::transform::generate::client_tools::PATCH.into(),
                }),
                r::ResponseOutputItem::ToolSearchCall(b)
                    if b.execution == r::ToolExecution::Client =>
                {
                    Some(ToolIdentity {
                        kind: super::ToolCallKind::Function,
                        chat_form: None,
                        call_id: b.call_id.clone(),
                        name: crate::transform::generate::client_tools::SEARCH.into(),
                    })
                }
                r::ResponseOutputItem::CustomToolCall(b) => Some(ToolIdentity {
                    kind: super::ToolCallKind::Custom,
                    chat_form: None,
                    call_id: Some(b.call_id.clone()),
                    name: b.name.clone(),
                }),
                _ => None,
            })
            .collect()
    }
}
