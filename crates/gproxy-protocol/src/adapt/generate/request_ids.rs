//! Apply explicit host ID policy where the pure pair preserves native IDs.

use super::{
    GenerationIdentity,
    claude_chat_ids::{allocate, result_id},
};
use crate::{
    Dialect,
    transform::{
        TransformError,
        identity::{IdentityRole, SourceIdentity},
    },
    wire::gemini as g,
};
use std::collections::BTreeMap;

pub(super) fn gemini_request(
    value: &mut g::GenerateContentRequestBody,
    identities: &mut GenerationIdentity,
    source: Dialect,
) -> Result<(), TransformError> {
    let mut flow = identities.request.clone();
    let policy = &identities.request_policy;
    let mut aliases = BTreeMap::new();
    let mut index = 0;
    for part in value
        .contents
        .iter_mut()
        .flat_map(|c| c.parts.iter_mut().flatten())
    {
        if let Some(call) = &mut part.function_call {
            if let Some(id) = &mut call.id {
                let mapped =
                    allocate(&mut flow, policy, source, IdentityRole::ToolCall, id, index)?;
                aliases.insert(id.clone(), mapped.clone());
                *id = mapped;
            }
            index += 1;
        }
    }
    for part in value
        .contents
        .iter_mut()
        .flat_map(|c| c.parts.iter_mut().flatten())
    {
        if let Some(result) = &mut part.function_response
            && let Some(id) = &mut result.id
        {
            result_id(id, &aliases, policy)?;
        }
    }
    identities.request = flow;
    Ok(())
}

/// Give each converted call the client ID the response policy allocates for
/// the upstream call at the same position.
pub(super) fn gemini_response<N: super::identity_facts::IdentityFacts>(
    value: &mut g::GenerateContentResponseBody,
    identities: &mut GenerationIdentity,
    native: &N,
) -> Result<(), TransformError> {
    let mut flow = identities.response.clone();
    let policy = &identities.response_policy;
    let source = native.dialect();
    if let Some(id) = &mut value.response_id {
        *id = allocate(&mut flow, policy, source, IdentityRole::Response, id, 0)?;
    }
    let original = native.tools();
    let mut native_ids = std::collections::BTreeSet::new();
    for id in original.iter().filter_map(|call| call.call_id.as_ref()) {
        if id.is_empty() || !native_ids.insert(id) {
            return Err(TransformError::invalid_result(
                "identity",
                "empty or duplicate actual native call ID",
            ));
        }
    }
    let parts: Vec<_> = value
        .candidates
        .iter_mut()
        .flatten()
        .filter_map(|c| c.content.as_mut())
        .flat_map(|c| c.parts.iter_mut().flatten())
        .filter(|p| p.function_call.is_some())
        .collect();
    if parts.len() != original.len() {
        return Err(TransformError::invalid_result(
            "identity",
            "converted/native tool count differs",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for (index, (part, original)) in parts.into_iter().zip(original).enumerate() {
        let call = part.function_call.as_mut().expect("filtered function");
        if call.name != original.name {
            return Err(TransformError::invalid_result(
                "identity",
                "converted/native tool name differs",
            ));
        }
        let mut trial = flow.clone();
        let identity = SourceIdentity::new(source, original.call_id.clone(), index as u64);
        let handle = if original.chat_form == Some(super::ChatCallForm::LegacyFunction) {
            trial.resolve_legacy_chat_call(identity, policy)
        } else {
            trial.resolve_or_allocate(IdentityRole::ToolCall, identity, policy)
        }
        .map_err(|e| TransformError::invalid_result("identity", e.to_string()))?;
        flow = trial;
        if !seen.insert(handle.emitted_id.clone()) {
            return Err(TransformError::invalid_result(
                "identity",
                "converted call IDs collide",
            ));
        }
        call.id = Some(handle.emitted_id);
    }
    identities.response = flow;
    Ok(())
}
