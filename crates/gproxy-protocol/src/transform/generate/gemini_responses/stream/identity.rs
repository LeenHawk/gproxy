use super::common::invalid;
use crate::{
    Dialect,
    transform::{
        TransformError,
        identity::{IdentityFlow, IdentityRole, SourceIdentity, TargetIdPolicy},
    },
    wire::{gemini as g, openai::responses::input as r},
};
use std::collections::{BTreeMap, BTreeSet};

/// Exact associations for fixed IDs in validated native signed replay. They are
/// available before yielding the corresponding Gemini part to the host.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SignedToolBindings(BTreeMap<String, String>);

impl SignedToolBindings {
    pub fn source_call_id(&self, emitted_id: &str) -> Option<&str> {
        self.0.get(emitted_id).map(String::as_str)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(a, b)| (a.as_str(), b.as_str()))
    }
}

pub(super) fn response_id(
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    source: Dialect,
    id: Option<String>,
) -> Result<String, TransformError> {
    flow.resolve_or_allocate(
        IdentityRole::Response,
        SourceIdentity::new(source, id, 0),
        policy,
    )
    .map(|h| h.emitted_id)
    .map_err(|e| invalid(e.to_string()))
}

pub(super) fn call(
    part: &mut g::Part,
    original: &r::FunctionCall,
    index: i64,
    allocator: (&mut IdentityFlow, &TargetIdPolicy),
    bindings: (
        &BTreeSet<String>,
        &mut BTreeSet<String>,
        &mut SignedToolBindings,
    ),
) -> Result<(), TransformError> {
    let (flow, policy) = allocator;
    let (reserved, used, signed) = bindings;
    let target = part
        .function_call
        .as_mut()
        .ok_or_else(|| invalid("missing projected function"))?;
    if part.thought_signature.is_some() {
        if let Some(id) = &target.id {
            if !policy.accepts_source(id) || !used.insert(id.clone()) {
                return Err(TransformError::unsupported(
                    "signature.function.id",
                    "original signed ID violates policy or collides",
                ));
            }
            signed.0.insert(id.clone(), original.call_id.clone());
        }
        return Ok(());
    }
    let mut trial = flow.clone();
    let mut handle = trial
        .resolve_or_allocate(
            IdentityRole::ToolCall,
            SourceIdentity::new(
                Dialect::OpenAi,
                Some(original.call_id.clone()),
                index as u64,
            ),
            policy,
        )
        .map_err(|e| invalid(e.to_string()))?;
    if reserved.contains(&handle.emitted_id) {
        trial = flow.clone();
        handle = trial
            .resolve_or_allocate(
                IdentityRole::ToolCall,
                SourceIdentity::new(Dialect::OpenAi, None, index as u64),
                policy,
            )
            .map_err(|e| invalid(e.to_string()))?;
        handle = trial
            .attach_source(&handle, original.call_id.clone())
            .map_err(|e| invalid(e.to_string()))?;
    }
    if reserved.contains(&handle.emitted_id) || !used.insert(handle.emitted_id.clone()) {
        return Err(invalid("projected function ID collides"));
    }
    target.id = Some(handle.emitted_id);
    *flow = trial;
    Ok(())
}

pub(super) fn normalize_text(body: &mut g::GenerateContentResponseBody) {
    for candidate in body.candidates.iter_mut().flatten() {
        if let Some(parts) = candidate.content.as_mut().and_then(|c| c.parts.as_mut()) {
            let mut out: Vec<g::Part> = Vec::new();
            for part in std::mem::take(parts) {
                if plain(&part)
                    && let Some(previous) = out.last_mut()
                    && plain(previous)
                    && previous.thought == part.thought
                {
                    previous
                        .text
                        .as_mut()
                        .expect("plain")
                        .push_str(part.text.as_deref().expect("plain"));
                } else {
                    out.push(part);
                }
            }
            *parts = out;
        }
    }
}

fn plain(p: &g::Part) -> bool {
    p.text.is_some()
        && p.thought_signature.is_none()
        && p.function_call.is_none()
        && p.function_response.is_none()
        && p.inline_data.is_none()
        && p.file_data.is_none()
        && p.executable_code.is_none()
        && p.code_execution_result.is_none()
        && p.tool_call.is_none()
        && p.tool_response.is_none()
        && p.video_metadata.is_none()
        && p.part_metadata.is_none()
        && p.media_resolution.is_none()
}
