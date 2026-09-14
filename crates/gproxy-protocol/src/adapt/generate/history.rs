//! Rebuild declared histories with exact saved aliases; source extensions are
//! removed before either traversal or storage. Tool names come from history first.
use super::{GenerationStateAccess, GenerationToolReplay};
use crate::{
    Dialect,
    capability::StateStore,
    transform::TransformError,
    wire::{
        DeclaredFields,
        claude::{content as cc, generate_content as c},
        gemini as g,
        openai::{chat as h, responses as r},
    },
};
use std::collections::{BTreeMap, BTreeSet};
fn name(names: &mut BTreeMap<String, String>, id: &str, value: &str) -> Result<(), TransformError> {
    if id.is_empty() || value.is_empty() || names.get(id).is_some_and(|old| old != value) {
        return Err(TransformError::shape(
            "history.tool_identity",
            "empty or contradictory tool ID/name",
        ));
    }
    names.insert(id.into(), value.into());
    Ok(())
}
pub(super) fn merge_names(
    target: &mut BTreeMap<String, String>,
    incoming: BTreeMap<String, String>,
) -> Result<(), TransformError> {
    for (id, value) in incoming {
        name(target, &id, &value)?;
    }
    Ok(())
}
fn restore(id: &mut String, replay: &GenerationToolReplay) {
    // Keep client identities until Chat legacy/modern form selection. An
    // actual native modern ID may collide with another client's legacy alias.
    if replay.chat_forms.contains_key(id) {
        return;
    }
    if let Some(original) = replay.original_call_ids.get(id) {
        *id = original.clone();
    }
}
async fn facts<S: StateStore>(
    state: &GenerationStateAccess<'_, S>,
    ids: BTreeSet<String>,
    names: BTreeMap<String, String>,
) -> Result<GenerationToolReplay, TransformError> {
    let mut replay = state
        .recover_tools_inner(
            &ids.into_iter().collect::<Vec<_>>(),
            &names,
            state.target.dialect == Dialect::Gemini,
        )
        .await?;
    if state.target.dialect != Dialect::OpenAiChat {
        for (client, original) in &replay.original_call_ids {
            if let Some(value) = replay.names.get(client).cloned() {
                name(&mut replay.names, original, &value)?;
            }
        }
        for (client, original) in &replay.original_call_ids {
            if let Some(kind) = replay.kinds.get(client).copied() {
                replay.kinds.insert(original.clone(), kind);
            }
        }
    }
    Ok(replay)
}
fn require_result_bindings<S: StateStore>(
    state: &GenerationStateAccess<'_, S>,
    results: &BTreeSet<String>,
    declared: &BTreeSet<String>,
    replay: &GenerationToolReplay,
) -> Result<(), TransformError> {
    if state.target.dialect != Dialect::Gemini
        && results
            .iter()
            .any(|id| !declared.contains(id) && !replay.kinds.contains_key(id))
    {
        return Err(TransformError::new(
            crate::transform::TransformErrorKind::MissingState,
            "history.tool_result",
            "orphan result requires a scoped native call record or actual declared call history",
        ));
    }
    Ok(())
}
fn kind(
    replay: &GenerationToolReplay,
    id: &str,
    expected: super::ToolCallKind,
) -> Result<(), TransformError> {
    if replay.kinds.get(id).is_some_and(|kind| *kind != expected) {
        return Err(TransformError::shape(
            "history.tool_kind",
            "declared tool kind differs from actual native call",
        ));
    }
    Ok(())
}
pub(super) async fn chat<S: StateStore>(
    input: h::GenerateContentRequestBody,
    state: &GenerationStateAccess<'_, S>,
) -> Result<(h::GenerateContentRequestBody, GenerationToolReplay), TransformError> {
    let mut input = input.into_declared();
    let mut names = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut declared = BTreeSet::new();
    let mut results = BTreeSet::new();
    for message in &input.messages {
        match message {
            h::ChatMessage::Assistant(message) => {
                for call in message.tool_calls.iter().flatten() {
                    match call {
                        h::MessageToolCall::Function(call) => {
                            name(&mut names, &call.id, &call.function.name)?;
                            ids.insert(call.id.clone());
                            declared.insert(call.id.clone());
                        }
                        h::MessageToolCall::Custom(call) => {
                            name(&mut names, &call.id, &call.custom.name)?;
                            ids.insert(call.id.clone());
                            declared.insert(call.id.clone());
                        }
                    }
                }
            }
            h::ChatMessage::Tool(message) => {
                ids.insert(message.tool_call_id.clone());
                results.insert(message.tool_call_id.clone());
            }
            _ => {}
        }
    }
    let mut replay = facts(state, ids, names).await?;
    require_result_bindings(state, &results, &declared, &replay)?;
    for message in &input.messages {
        if let h::ChatMessage::Assistant(message) = message {
            for call in message.tool_calls.iter().flatten() {
                let (id, kind) = match call {
                    h::MessageToolCall::Function(call) => (&call.id, super::ToolCallKind::Function),
                    h::MessageToolCall::Custom(call) => (&call.id, super::ToolCallKind::Custom),
                };
                if replay
                    .kinds
                    .get(id)
                    .is_some_and(|existing| *existing != kind)
                {
                    return Err(TransformError::shape(
                        "history.tool_kind",
                        "declared tool kind differs from saved identity",
                    ));
                }
                replay.kinds.insert(id.clone(), kind);
            }
        }
    }

    for message in &mut input.messages {
        match message {
            h::ChatMessage::Assistant(message) => {
                for call in message.tool_calls.iter_mut().flatten() {
                    match call {
                        h::MessageToolCall::Function(call) => restore(&mut call.id, &replay),
                        h::MessageToolCall::Custom(call) => restore(&mut call.id, &replay),
                    }
                }
            }
            h::ChatMessage::Tool(message) => restore(&mut message.tool_call_id, &replay),
            _ => {}
        }
    }
    Ok((input, replay))
}
pub(super) async fn claude<S: StateStore>(
    input: c::GenerateContentRequestBody,
    state: &GenerationStateAccess<'_, S>,
) -> Result<(c::GenerateContentRequestBody, GenerationToolReplay), TransformError> {
    let mut input = input.into_declared();
    let mut names = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut declared = BTreeSet::new();
    let mut results = BTreeSet::new();
    for message in &input.messages {
        if let cc::MessageContent::Blocks(blocks) = &message.content {
            for b in blocks {
                match b {
                    cc::ContentBlock::ToolUse(b) => {
                        name(&mut names, &b.id, &b.name)?;
                        ids.insert(b.id.clone());
                        declared.insert(b.id.clone());
                    }
                    cc::ContentBlock::ToolResult(b) => {
                        ids.insert(b.tool_use_id.clone());
                        results.insert(b.tool_use_id.clone());
                    }
                    _ => {}
                }
            }
        }
    }
    let replay = facts(state, ids, names).await?;
    require_result_bindings(state, &results, &declared, &replay)?;
    for message in &mut input.messages {
        if let cc::MessageContent::Blocks(blocks) = &mut message.content {
            for b in blocks {
                match b {
                    cc::ContentBlock::ToolUse(b) => {
                        kind(&replay, &b.id, super::ToolCallKind::Function)?;
                        restore(&mut b.id, &replay);
                    }
                    cc::ContentBlock::ToolResult(b) => {
                        kind(&replay, &b.tool_use_id, super::ToolCallKind::Function)?;
                        restore(&mut b.tool_use_id, &replay);
                    }
                    _ => {}
                }
            }
        }
    }
    Ok((input, replay))
}
pub(super) async fn gemini<S: StateStore>(
    input: g::GenerateContentRequestBody,
    state: &GenerationStateAccess<'_, S>,
) -> Result<(g::GenerateContentRequestBody, GenerationToolReplay), TransformError> {
    let mut input = input.into_declared();
    let mut names = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut declared = BTreeSet::new();
    let mut results = BTreeSet::new();
    for p in input.contents.iter().flat_map(|c| c.parts.iter().flatten()) {
        if let Some(call) = &p.function_call
            && let Some(id) = &call.id
        {
            name(&mut names, id, &call.name)?;
            ids.insert(id.clone());
            declared.insert(id.clone());
        }
        if let Some(result) = &p.function_response
            && let Some(id) = &result.id
        {
            name(&mut names, id, &result.name)?;
            ids.insert(id.clone());
            results.insert(id.clone());
        }
    }
    let replay = facts(state, ids, names).await?;
    require_result_bindings(state, &results, &declared, &replay)?;
    for p in input
        .contents
        .iter_mut()
        .flat_map(|c| c.parts.iter_mut().flatten())
    {
        if let Some(call) = &mut p.function_call
            && let Some(id) = &mut call.id
        {
            kind(&replay, id, super::ToolCallKind::Function)?;
            restore(id, &replay);
        }
        if let Some(result) = &mut p.function_response
            && let Some(id) = &mut result.id
        {
            kind(&replay, id, super::ToolCallKind::Function)?;
            restore(id, &replay);
        }
    }
    Ok((input, replay))
}
pub(super) async fn responses<S: StateStore>(
    input: r::GenerateContentRequestBody,
    state: &GenerationStateAccess<'_, S>,
) -> Result<(r::GenerateContentRequestBody, GenerationToolReplay), TransformError> {
    let mut input = input.into_declared();
    let mut names = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut declared = BTreeSet::new();
    let mut results = BTreeSet::new();
    if let Some(r::Input::Items(items)) = &input.input {
        for item in items {
            match item {
                r::InputItem::FunctionCall(b) => {
                    name(&mut names, &b.call_id, &b.name)?;
                    ids.insert(b.call_id.clone());
                    declared.insert(b.call_id.clone());
                }
                r::InputItem::CustomToolCall(b) => {
                    name(&mut names, &b.call_id, &b.name)?;
                    ids.insert(b.call_id.clone());
                    declared.insert(b.call_id.clone());
                }
                r::InputItem::FunctionCallOutput(b) => {
                    if let Some(Some(value)) = &b.name {
                        name(&mut names, &b.call_id, value)?;
                    }
                    ids.insert(b.call_id.clone());
                    results.insert(b.call_id.clone());
                }
                r::InputItem::CustomToolCallOutput(b) => {
                    ids.insert(b.call_id.clone());
                    results.insert(b.call_id.clone());
                }
                _ => {}
            }
        }
    }
    let replay = facts(state, ids, names).await?;
    require_result_bindings(state, &results, &declared, &replay)?;
    if let Some(r::Input::Items(items)) = &mut input.input {
        for item in items {
            match item {
                r::InputItem::FunctionCall(b) => {
                    kind(&replay, &b.call_id, super::ToolCallKind::Function)?;
                    b.id = replay
                        .original_item_ids
                        .get(&b.call_id)
                        .cloned()
                        .or_else(|| b.id.clone());
                    restore(&mut b.call_id, &replay);
                }
                r::InputItem::CustomToolCall(b) => {
                    kind(&replay, &b.call_id, super::ToolCallKind::Custom)?;
                    b.id = replay
                        .original_item_ids
                        .get(&b.call_id)
                        .cloned()
                        .or_else(|| b.id.clone());
                    restore(&mut b.call_id, &replay);
                }
                r::InputItem::FunctionCallOutput(b) => {
                    kind(&replay, &b.call_id, super::ToolCallKind::Function)?;
                    if let Some(value) = replay.names.get(&b.call_id) {
                        b.name = Some(Some(value.clone()));
                    }
                    restore(&mut b.call_id, &replay);
                }
                r::InputItem::CustomToolCallOutput(b) => {
                    kind(&replay, &b.call_id, super::ToolCallKind::Custom)?;
                    restore(&mut b.call_id, &replay);
                }
                _ => {}
            }
        }
    }
    Ok((input, replay))
}
