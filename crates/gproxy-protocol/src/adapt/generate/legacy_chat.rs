//! Restore only explicitly attested native legacy Chat function forms. No call
//! declaration or arguments are invented for an orphan result.
use super::GenerationToolReplay;
use crate::{
    transform::{Report, TransformError},
    wire::openai::chat as h,
};
use std::collections::BTreeMap;
pub(super) fn restore(
    request: &mut h::GenerateContentRequestBody,
    replay: &GenerationToolReplay,
    report: &mut Report,
) -> Result<(), TransformError> {
    let legacy = replay.legacy_chat_calls();
    if legacy.is_empty() && replay.original_call_ids.is_empty() {
        return Ok(());
    }
    let mut messages = Vec::new();
    let mut changed = false;
    for message in std::mem::take(&mut request.messages) {
        match message {
            h::ChatMessage::Tool(tool) if legacy.contains_key(&tool.tool_call_id) => {
                let content = match tool.content {
                    h::TextContent::Text(text) => text,
                    h::TextContent::Parts(parts) => parts
                        .into_iter()
                        .map(|v| v.text)
                        .collect::<Vec<_>>()
                        .join(""),
                };
                messages.push(h::ChatMessage::Function(
                    h::FunctionMessage::builder(
                        h::FunctionRole::Function,
                        Some(content),
                        legacy[&tool.tool_call_id].clone(),
                    )
                    .build(),
                ));
                changed = true;
            }
            h::ChatMessage::Assistant(assistant)
                if assistant
                    .tool_calls
                    .as_ref()
                    .is_some_and(|calls| calls.iter().any(|c| legacy.contains_key(call_id(c)))) =>
            {
                messages.extend(declarations(assistant, &legacy, &replay.original_call_ids)?);
                changed = true;
            }
            h::ChatMessage::Tool(mut tool) => {
                if let Some(original) = replay.original_call_ids.get(&tool.tool_call_id) {
                    tool.tool_call_id = original.clone();
                }
                messages.push(h::ChatMessage::Tool(tool));
            }
            h::ChatMessage::Assistant(mut assistant) => {
                for call in assistant.tool_calls.iter_mut().flatten() {
                    remap(call, &replay.original_call_ids);
                }
                messages.push(h::ChatMessage::Assistant(assistant));
            }
            other => messages.push(other),
        }
    }
    request.messages = messages;
    if changed {
        report.changed(
            "history.function_call",
            "restored actual native Chat legacy assistant/result forms from scoped call-form facts",
        );
    }
    Ok(())
}
fn call_id(call: &h::MessageToolCall) -> &str {
    match call {
        h::MessageToolCall::Function(v) => &v.id,
        h::MessageToolCall::Custom(v) => &v.id,
    }
}
fn declarations(
    mut original: h::AssistantMessage,
    legacy: &BTreeMap<String, String>,
    originals: &BTreeMap<String, String>,
) -> Result<Vec<h::ChatMessage>, TransformError> {
    if original
        .function_call
        .as_ref()
        .and_then(Option::as_ref)
        .is_some()
    {
        return Err(TransformError::shape(
            "history.function_call",
            "legacy and modern fields coexist",
        ));
    }
    let mut groups: Vec<h::AssistantMessage> = Vec::new();
    for mut call in original.tool_calls.take().unwrap_or_default() {
        if let Some(name) = legacy.get(call_id(&call)) {
            let h::MessageToolCall::Function(call) = call else {
                return Err(TransformError::shape(
                    "history.tool_kind",
                    "legacy Chat function cannot be a custom tool",
                ));
            };
            if &call.function.name != name {
                return Err(TransformError::shape(
                    "history.tool_name",
                    "declared legacy function differs from actual native name",
                ));
            }
            let mut message = h::AssistantMessage::builder(h::AssistantRole::Assistant).build();
            message.function_call = Some(Some(call.function));
            groups.push(message);
        } else {
            remap(&mut call, originals);
            if groups
                .last()
                .is_none_or(|message| message.function_call.is_some())
            {
                groups.push(h::AssistantMessage::builder(h::AssistantRole::Assistant).build());
            }
            groups
                .last_mut()
                .expect("just allocated")
                .tool_calls
                .get_or_insert_with(Vec::new)
                .push(call);
        }
    }
    let first = groups
        .first_mut()
        .ok_or_else(|| TransformError::shape("history.tool_calls", "missing declared call"))?;
    first.content = original.content;
    first.audio = original.audio;
    first.refusal = original.refusal;
    first.name = original.name;
    Ok(groups.into_iter().map(h::ChatMessage::Assistant).collect())
}

fn remap(call: &mut h::MessageToolCall, originals: &BTreeMap<String, String>) {
    let id = match call {
        h::MessageToolCall::Function(v) => &mut v.id,
        h::MessageToolCall::Custom(v) => &mut v.id,
    };
    if let Some(original) = originals.get(id) {
        *id = original.clone();
    }
}
