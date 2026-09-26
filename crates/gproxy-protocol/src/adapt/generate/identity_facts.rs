//! Only declared identity/name facts: this is not a content representation.

use crate::{
    Dialect,
    transform::identity::{IdentityRole, OutputItemKind},
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
    pub item_id: Option<String>,
    pub name: String,
}

pub(super) trait IdentityFacts {
    fn dialect(&self) -> Dialect;
    fn response_id(&self) -> Option<&str>;
    fn tools(&self) -> Vec<ToolIdentity>;
    /// Native custom-tool wrappers that cannot supply a raw client input.
    /// Positions refer to the unfiltered native tool list (including signatures).
    fn omitted_custom_tools(&self) -> Vec<usize> {
        Vec::new()
    }
    fn signed_claude(&self, _index: u64) -> Option<crate::wire::claude::content::ThinkingBlock> {
        None
    }
    fn signed_gemini_reasoning(&self, _index: u64) -> Option<g::Part> {
        None
    }
    fn signed_gemini_image(&self, _index: u64) -> Option<g::Part> {
        None
    }
    fn signed_gemini_tool(&self, _index: usize) -> Option<g::Part> {
        None
    }
    /// The whole signed thought run closing at `index`, merged into the one
    /// part a Claude client's thinking block replays as.
    fn signed_gemini_thinking(&self, _index: u64) -> Option<g::Part> {
        None
    }
    fn native_model(&self) -> Option<&str>;
    fn items(&self) -> Vec<(IdentityRole, String)> {
        Vec::new()
    }
}

impl IdentityFacts for c::GenerateContentResponseBody {
    fn omitted_custom_tools(&self) -> Vec<usize> {
        self.content
            .iter()
            .filter_map(|block| match block {
                c::ResponseContentBlock::ToolUse(call) => Some(call),
                _ => None,
            })
            .enumerate()
            .filter_map(|(index, call)| {
                (call.name.starts_with("gproxy_custom_")
                    && !call
                        .input
                        .get("input")
                        .is_some_and(serde_json::Value::is_string))
                .then_some(index)
            })
            .collect()
    }
    fn native_model(&self) -> Option<&str> {
        Some(&self.model)
    }
    fn signed_claude(&self, index: u64) -> Option<crate::wire::claude::content::ThinkingBlock> {
        match self.content.get(usize::try_from(index).ok()?)? {
            c::ResponseContentBlock::Thinking(block) if !block.signature.is_empty() => {
                Some(block.clone())
            }
            _ => None,
        }
    }
    fn dialect(&self) -> Dialect {
        Dialect::Claude
    }
    /// Thinking blocks signed with a gproxy handle name a saved Gemini part.
    fn items(&self) -> Vec<(IdentityRole, String)> {
        self.content
            .iter()
            .filter_map(|block| match block {
                c::ResponseContentBlock::Thinking(block) => {
                    crate::transform::generate::claude_gemini::thinking_handle_id(&block.signature)
                }
                _ => None,
            })
            .map(|id| {
                (
                    IdentityRole::OutputItem(OutputItemKind::Reasoning),
                    id.to_owned(),
                )
            })
            .collect()
    }
    fn response_id(&self) -> Option<&str> {
        Some(&self.id)
    }
    fn tools(&self) -> Vec<ToolIdentity> {
        self.content
            .iter()
            .filter_map(|b| match b {
                c::ResponseContentBlock::ToolUse(b) => Some(ToolIdentity {
                    kind: super::ToolCallKind::Function,
                    chat_form: None,
                    call_id: Some(b.id.clone()),
                    item_id: None,
                    name: b.name.clone(),
                }),
                _ => None,
            })
            .collect()
    }
}

impl IdentityFacts for h::GenerateContentResponseBody {
    fn native_model(&self) -> Option<&str> {
        Some(&self.model)
    }
    fn dialect(&self) -> Dialect {
        Dialect::OpenAiChat
    }
    fn response_id(&self) -> Option<&str> {
        Some(&self.id)
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
                        item_id: None,
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
                            item_id: None,
                            name: call.function.name.clone(),
                        },
                        h::MessageToolCall::Custom(call) => ToolIdentity {
                            kind: super::ToolCallKind::Custom,
                            chat_form: Some(super::ChatCallForm::Modern),
                            call_id: Some(call.id.clone()),
                            item_id: None,
                            name: call.custom.name.clone(),
                        },
                    });
                legacy.chain(current)
            })
            .collect()
    }
}

impl IdentityFacts for g::GenerateContentResponseBody {
    fn omitted_custom_tools(&self) -> Vec<usize> {
        self.candidates
            .iter()
            .flatten()
            .filter_map(|candidate| candidate.content.as_ref())
            .flat_map(|content| content.parts.iter().flatten())
            .filter_map(|part| part.function_call.as_ref())
            .enumerate()
            .filter_map(|(index, call)| {
                (call.name.starts_with("gproxy_custom_")
                    && !call
                        .args
                        .as_ref()
                        .and_then(|args| args.get("input"))
                        .is_some_and(serde_json::Value::is_string))
                .then_some(index)
            })
            .collect()
    }
    fn native_model(&self) -> Option<&str> {
        self.model_version.as_deref()
    }
    fn signed_gemini_reasoning(&self, index: u64) -> Option<g::Part> {
        let candidate = self.candidates.as_ref()?.first()?;
        let part = candidate
            .content
            .as_ref()?
            .parts
            .as_ref()?
            .get(usize::try_from(index).ok()?)?;
        (part.thought == Some(true) && part.thought_signature.is_some()).then(|| part.clone())
    }
    fn signed_gemini_image(&self, index: u64) -> Option<g::Part> {
        self.candidates
            .as_ref()?
            .first()?
            .content
            .as_ref()?
            .parts
            .as_ref()?
            .get(usize::try_from(index).ok()?)
            .filter(|part| {
                (part.inline_data.is_some() || part.file_data.is_some())
                    && part.thought_signature.is_some()
            })
            .cloned()
    }
    fn signed_gemini_thinking(&self, index: u64) -> Option<g::Part> {
        let parts = self
            .candidates
            .as_ref()?
            .first()?
            .content
            .as_ref()?
            .parts
            .as_ref()?;
        crate::transform::generate::claude_gemini::thinking::signed_run(
            parts,
            usize::try_from(index).ok()?,
        )
    }
    fn signed_gemini_tool(&self, index: usize) -> Option<g::Part> {
        self.candidates
            .iter()
            .flatten()
            .filter_map(|c| c.content.as_ref())
            .flat_map(|c| c.parts.iter().flatten())
            .filter(|p| p.function_call.is_some())
            .nth(index)
            .filter(|p| p.thought_signature.is_some())
            .cloned()
    }
    fn dialect(&self) -> Dialect {
        Dialect::Gemini
    }
    fn response_id(&self) -> Option<&str> {
        self.response_id.as_deref()
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
                item_id: None,
                name: b.name.clone(),
            })
            .collect()
    }
}

impl IdentityFacts for r::GenerateContentResponseBody {
    fn native_model(&self) -> Option<&str> {
        Some(&self.model)
    }
    fn dialect(&self) -> Dialect {
        Dialect::OpenAi
    }
    fn response_id(&self) -> Option<&str> {
        Some(&self.id)
    }
    fn tools(&self) -> Vec<ToolIdentity> {
        self.output
            .iter()
            .filter_map(|b| match b {
                r::ResponseOutputItem::FunctionCall(b) => Some(ToolIdentity {
                    kind: super::ToolCallKind::Function,
                    chat_form: None,
                    call_id: Some(b.call_id.clone()),
                    item_id: b.id.clone(),
                    name: b.namespace.as_ref().map_or_else(
                        || b.name.clone(),
                        |ns| crate::transform::generate::client_tools::qualified(ns, &b.name),
                    ),
                }),
                r::ResponseOutputItem::ShellCall(b) => Some(ToolIdentity {
                    kind: super::ToolCallKind::Function,
                    chat_form: None,
                    call_id: Some(b.call_id.clone()),
                    item_id: Some(b.id.clone()),
                    name: crate::transform::generate::client_tools::SHELL.into(),
                }),
                r::ResponseOutputItem::ApplyPatchCall(b) => Some(ToolIdentity {
                    kind: super::ToolCallKind::Function,
                    chat_form: None,
                    call_id: Some(b.call_id.clone()),
                    item_id: Some(b.id.clone()),
                    name: crate::transform::generate::client_tools::PATCH.into(),
                }),
                r::ResponseOutputItem::ToolSearchCall(b)
                    if b.execution == r::ToolExecution::Client =>
                {
                    Some(ToolIdentity {
                        kind: super::ToolCallKind::Function,
                        chat_form: None,
                        call_id: b.call_id.clone(),
                        item_id: Some(b.id.clone()),
                        name: crate::transform::generate::client_tools::SEARCH.into(),
                    })
                }
                r::ResponseOutputItem::CustomToolCall(b) => Some(ToolIdentity {
                    kind: super::ToolCallKind::Custom,
                    chat_form: None,
                    call_id: Some(b.call_id.clone()),
                    item_id: b.id.clone(),
                    name: b.name.clone(),
                }),
                _ => None,
            })
            .collect()
    }
    fn items(&self) -> Vec<(IdentityRole, String)> {
        self.output
            .iter()
            .filter_map(|b| match b {
                r::ResponseOutputItem::Message(b) => Some((OutputItemKind::Message, b.id.clone())),
                r::ResponseOutputItem::Reasoning(b) => {
                    Some((OutputItemKind::Reasoning, b.id.clone()))
                }
                r::ResponseOutputItem::FunctionCall(b) => {
                    b.id.clone().map(|id| (OutputItemKind::FunctionCall, id))
                }
                r::ResponseOutputItem::CustomToolCall(b) => {
                    b.id.clone().map(|id| (OutputItemKind::CustomToolCall, id))
                }
                r::ResponseOutputItem::ShellCall(b) => {
                    Some((OutputItemKind::ShellCall, b.id.clone()))
                }

                r::ResponseOutputItem::ApplyPatchCall(b) => {
                    Some((OutputItemKind::ApplyPatchCall, b.id.clone()))
                }

                r::ResponseOutputItem::ToolSearchCall(b) => {
                    Some((OutputItemKind::ToolSearchCall, b.id.clone()))
                }

                r::ResponseOutputItem::ImageGenerationCall(b) => {
                    Some((OutputItemKind::ImageGenerationCall, b.id.clone()))
                }
                _ => None,
            })
            .map(|(kind, id)| (IdentityRole::OutputItem(kind), id))
            .collect()
    }
}
